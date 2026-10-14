//! Small text helpers: Apple's bits of HTML to plain text, and URL
//! escaping as the API (and brumm's Go original) expects it.

use std::collections::BTreeMap;

/// Turns an editorial note's bit of HTML into one line of text.
pub fn plain(s: &str) -> String {
    let mut stripped = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        match rest[i..].find('>') {
            Some(j) => {
                stripped.push_str(&rest[..i]);
                stripped.push(' ');
                rest = &rest[i + j + 1..];
            }
            None => break,
        }
    }
    stripped.push_str(rest);
    unescape_html(&stripped)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Decodes HTML character references, including named Unicode entities.
pub fn unescape_html(s: &str) -> String {
    htmlize::unescape(s).into_owned()
}

pub fn dedupe(list: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    list.into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

fn escape(s: &str, keep: impl Fn(u8) -> bool, space_plus: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') || keep(b) {
            out.push(b as char);
        } else if b == b' ' && space_plus {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Escapes s for a query, as Go's url.QueryEscape: space as +.
pub fn query_escape(s: &str) -> String {
    escape(s, |_| false, true)
}

/// Escapes s for one path segment, as Go's url.PathEscape.
pub fn path_escape(s: &str) -> String {
    escape(
        s,
        |b| matches!(b, b'$' | b'&' | b'+' | b':' | b'=' | b'@'),
        false,
    )
}

fn unescape_query(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(h, 16).ok()?);
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// A query string's pairs, by key in order; unreadable pairs are dropped.
pub fn parse_query(q: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for part in q.split('&').filter(|p| !p.is_empty()) {
        if part.contains(';') {
            continue;
        }
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        if let (Some(k), Some(v)) = (unescape_query(k), unescape_query(v)) {
            out.entry(k).or_default().push(v);
        }
    }
    out
}

/// Encodes pairs as a query, sorted by key, as Go's url.Values.Encode.
pub fn encode_query(pairs: &[(String, String)]) -> String {
    let mut m: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (k, v) in pairs {
        m.entry(k).or_default().push(v);
    }
    encode_map(
        &m.into_iter()
            .map(|(k, v)| (k.to_string(), v.into_iter().map(String::from).collect()))
            .collect(),
    )
}

fn encode_map(m: &BTreeMap<String, Vec<String>>) -> String {
    let mut parts = Vec::new();
    for (k, vs) in m {
        for v in vs {
            parts.push(format!("{}={}", query_escape(k), query_escape(v)));
        }
    }
    parts.join("&")
}

/// The next page's address, carrying over the page size, extensions and
/// language the first page asked for when Apple's link drops them.
pub fn inherit_page_query(previous: &str, next: &str) -> String {
    let (_, pq) = previous.split_once('?').unwrap_or((previous, ""));
    let next = next.split('#').next().unwrap_or("");
    let (path, nq) = next.split_once('?').unwrap_or((next, ""));
    let initial = parse_query(pq);
    let mut query = parse_query(nq);
    for key in ["limit", "extend", "include", "l"] {
        if !query.contains_key(key)
            && let Some(v) = initial.get(key)
        {
            query.insert(key.to_string(), v.clone());
        }
    }
    let q = encode_map(&query);
    if q.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{q}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text() {
        assert_eq!(
            plain("<b>Your</b> favourites &amp; more"),
            "Your favourites & more"
        );
        assert_eq!(
            plain("a<br/>b &#39;c&#x27; &bogus; <unclosed"),
            "a b 'c' &bogus; <unclosed"
        );
        assert_eq!(plain("  many   \n spaces&nbsp;here "), "many spaces here");
        assert_eq!(
            plain("<i>Bj&ouml;rk</i> &amp; Beyonc&eacute;"),
            "Björk & Beyoncé"
        );
        assert_eq!(plain("a &NotEqualTilde; b"), "a ≂̸ b");
    }

    #[test]
    fn escaping_as_go() {
        assert_eq!(query_escape("US123&ids=bad"), "US123%26ids%3Dbad");
        assert_eq!(query_escape("a b*~"), "a+b%2A~");
        assert_eq!(path_escape("i.a/b c:d"), "i.a%2Fb%20c:d");
        assert_eq!(
            encode_query(&[("term".into(), "x y".into()), ("limit".into(), "5".into())]),
            "limit=5&term=x+y"
        );
    }

    #[test]
    fn next_page_inherits_the_query() {
        assert_eq!(
            inherit_page_query(
                "/v1/me/library/songs?limit=100&extend=inFavorites",
                "/v1/me/library/songs?offset=100"
            ),
            "/v1/me/library/songs?extend=inFavorites&limit=100&offset=100"
        );
        assert_eq!(
            inherit_page_query("/v1/x", "/v1/x?offset=1"),
            "/v1/x?offset=1"
        );
    }
}
