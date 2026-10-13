//! Styled lines, measured and cut by display columns: the few operations
//! the screen is built with (what charmbracelet's ansi package did for the
//! Go player). Colors come only from the terminal's ANSI palette, so the
//! Omarchy theme decides them; the cover, and the accent it lends, are the
//! exceptions.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub type L = Line<'static>;

// Each has one job: magenta is where you are, green is what plays, cyan is
// a key, blue is the music itself, bright black is everything secondary.
pub const HERE: Style = Style::new().fg(Color::Magenta);
pub const PLAYS: Style = Style::new().fg(Color::Green);
pub const KEY: Style = Style::new().fg(Color::Cyan);
pub const MUSIC: Style = Style::new().fg(Color::Blue);
pub const DIM: Style = Style::new().fg(Color::DarkGray);
pub const ERR: Style = Style::new().fg(Color::Red);
pub const BOLD: Style = Style::new().add_modifier(Modifier::BOLD);
pub const PILL: Style = Style::new()
    .fg(Color::Green)
    .add_modifier(Modifier::REVERSED);
pub const PLAIN: Style = Style::new();

pub fn bold(s: Style) -> Style {
    s.add_modifier(Modifier::BOLD)
}

/// A span of text in a style.
pub fn sp(text: impl Into<String>, style: Style) -> Span<'static> {
    Span::styled(text.into(), style)
}

/// A line of one span.
pub fn one(text: impl Into<String>, style: Style) -> L {
    Line::from(sp(text, style))
}

/// Plain text as a line.
pub fn plain(text: impl Into<String>) -> L {
    Line::from(Span::raw(text.into()))
}

/// Lines from spans.
pub fn line(spans: Vec<Span<'static>>) -> L {
    Line::from(spans)
}

/// The display width of a string.
pub fn sw(s: &str) -> usize {
    s.width()
}

/// The display width of a line.
pub fn w(l: &L) -> usize {
    l.spans.iter().map(|s| s.content.width()).sum()
}

/// a followed by b.
pub fn cat(mut a: L, b: L) -> L {
    a.spans.extend(b.spans);
    a
}

/// n spaces.
pub fn spaces(n: usize) -> Span<'static> {
    Span::raw(" ".repeat(n))
}

/// The columns [left, right) of l; a wide glyph straddling either edge is
/// left out.
pub fn cut(l: &L, left: usize, right: usize) -> L {
    let mut out = Vec::new();
    let mut col = 0;
    for s in &l.spans {
        if col >= right {
            break;
        }
        let mut text = String::new();
        for g in s.content.graphemes(true) {
            let gw = g.width();
            if col >= left && col + gw <= right {
                text.push_str(g);
            }
            col += gw;
            if col >= right {
                break;
            }
        }
        if !text.is_empty() {
            out.push(Span::styled(text, s.style));
        }
    }
    Line::from(out)
}

/// l cut to at most n columns, the last of them "…" when anything was cut.
pub fn trunc(l: &L, n: usize) -> L {
    if w(l) <= n {
        return l.clone();
    }
    if n == 0 {
        return Line::default();
    }
    let mut out = cut(l, 0, n - 1);
    let style = out.spans.last().map(|s| s.style).unwrap_or_default();
    out.spans.push(Span::styled("…", style));
    out
}

/// l with n columns taken off its left, prefix put in their place when
/// anything was taken.
pub fn trunc_left(l: &L, n: usize, prefix: &str) -> L {
    if n == 0 {
        return l.clone();
    }
    let total = w(l);
    let mut rest = cut(l, n, total);
    let style = rest.spans.first().map(|s| s.style).unwrap_or_default();
    rest.spans
        .insert(0, Span::styled(prefix.to_string(), style));
    rest
}

/// l padded with spaces to n columns.
pub fn pad(mut l: L, n: usize) -> L {
    let lw = w(&l);
    if lw < n {
        l.spans.push(spaces(n - lw));
    }
    l
}

/// l cut or padded to exactly n columns.
pub fn fit(l: &L, n: usize) -> L {
    pad(trunc(l, n), n)
}

/// l and r at the two edges of n columns, l cut short when both do not fit.
pub fn spread(l: L, r: L, n: usize) -> L {
    let rw = w(&r);
    if rw == 0 {
        return l;
    }
    if n < rw + 2 + 1 {
        return trunc(&l, n);
    }
    let room = n - rw - 2;
    let l = trunc(&l, room);
    let gap = n - w(&l) - rw;
    let mut out = l;
    out.spans.push(spaces(gap));
    out.spans.extend(r.spans);
    out
}

/// Plain text broken into at most n lines of w columns, the last ending
/// with … when it does not all fit.
pub fn wrap(s: &str, width: usize, n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if sw(&cur) + 1 + sw(word) <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    let tr = |s: &str| -> String {
        trunc(&plain(s.to_string()), width)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    };
    if out.len() > n && n > 0 {
        out.truncate(n);
        let mut last = tr(&format!("{} …", out[n - 1]));
        if !last.ends_with('…') {
            last.push('…');
        }
        out[n - 1] = last;
    }
    out.iter().map(|l| tr(l)).collect()
}

/// The text of a line, unstyled.
#[cfg(test)]
pub fn text(l: &L) -> String {
    l.spans.iter().map(|s| s.content.as_ref()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_and_truncate_by_columns() {
        let l = line(vec![sp("ab", HERE), sp("cdé", DIM), sp("日本", PLAIN)]);
        assert_eq!(w(&l), 9);
        assert_eq!(text(&cut(&l, 1, 4)), "bcd");
        assert_eq!(text(&cut(&l, 5, 7)), "日", "a wide glyph inside");
        assert_eq!(
            text(&cut(&l, 6, 9)),
            "本",
            "a straddling wide glyph is left out"
        );
        assert_eq!(text(&trunc(&l, 4)), "abc…");
        assert_eq!(text(&trunc(&l, 9)), "abcdé日本");
        assert_eq!(text(&trunc_left(&l, 2, "…")), "…cdé日本");
        assert_eq!(w(&fit(&l, 12)), 12);
        assert_eq!(text(&spread(plain("left side"), plain("r"), 8)), "left…  r");
        assert_eq!(text(&spread(plain("l"), plain("r"), 8)), "l      r");
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap("one two three four", 9, 2), vec!["one two", "three …"]);
        assert_eq!(wrap("short", 10, 3), vec!["short"]);
    }
}
