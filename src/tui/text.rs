//! Text that goes to the screen. Terminals and ratatui disagree about the width of some invisible
//! or combining characters, and any disagreement shifts the rest of the row (and can push the end of
//! a line into the next row). Dropping them is lossless for reading.

use std::borrow::Cow;

const TAB_WIDTH: usize = 4;

/// Variation selectors, zero-width joiners/spaces and the BOM: invisible, yet width-ambiguous.
pub fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{FE0E}' | '\u{FE0F}' | '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}'
    )
}

pub fn sanitize(s: &str) -> Cow<'_, str> {
    if !s.chars().any(|c| is_invisible(c) || c == '\t') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\t' {
            out.push_str(&" ".repeat(TAB_WIDTH));
        } else if !is_invisible(c) {
            out.push(c);
        }
    }
    Cow::Owned(out)
}
