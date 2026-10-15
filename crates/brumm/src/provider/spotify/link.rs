//! Spotify's share links: open.spotify.com addresses and spotify: URIs.

use super::valid_id;

/// The kind and id a link names: track, album, playlist or artist. Only
/// https links on open.spotify.com itself count.
pub(super) fn parse(link: &str) -> Option<(&'static str, String)> {
    let link = link.trim();
    let kind_of = |k: &str| match k {
        "track" => Some("track"),
        "album" => Some("album"),
        "playlist" => Some("playlist"),
        "artist" => Some("artist"),
        _ => None,
    };
    if let Some(rest) = link.strip_prefix("spotify:") {
        let (k, id) = rest.split_once(':')?;
        return Some((kind_of(k)?, valid_id(id).then(|| id.to_string())?));
    }
    let u = url::Url::parse(link).ok()?;
    if u.scheme() != "https" || u.host_str() != Some("open.spotify.com") || u.port().is_some() {
        return None;
    }
    let mut parts = u.path_segments()?.filter(|p| !p.is_empty());
    let mut k = parts.next()?;
    if k.starts_with("intl-") {
        k = parts.next()?;
    }
    let kind = kind_of(k)?;
    let id = parts.next()?;
    valid_id(id).then(|| (kind, id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_name_a_kind_and_an_id() {
        let id = "4uLU6hMCjMI75M1A2tKUQC";
        for (link, kind) in [
            (
                format!("https://open.spotify.com/track/{id}?si=abc"),
                "track",
            ),
            (
                format!("https://open.spotify.com/intl-de/album/{id}"),
                "album",
            ),
            (format!("spotify:playlist:{id}"), "playlist"),
            (format!("https://open.spotify.com/artist/{id}"), "artist"),
        ] {
            assert_eq!(parse(&link), Some((kind, id.to_string())), "{link}");
        }
        for link in [
            format!("http://open.spotify.com/track/{id}"),
            format!("https://open.spotify.com.evil.example/track/{id}"),
            format!("https://evilopen.spotify.com/track/{id}"),
            format!("https://open.spotify.com:444/track/{id}"),
            format!("https://open.spotify.com/show/{id}"),
            "https://open.spotify.com/track/short".into(),
            "spotify:track:../../me".into(),
            format!("file://open.spotify.com/track/{id}"),
        ] {
            assert_eq!(parse(&link), None, "{link}");
        }
    }
}
