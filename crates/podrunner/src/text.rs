//! Text from a forge or a unit, made safe for a terminal: a control
//! character prints as an escape (`\x1b`), and a value is cut at 200
//! bytes.

/// The most bytes of a display value.
pub const MAX: usize = 200;

/// The most bytes of a line of podrunner's own text that holds values: a
/// cut at [`MAX`] would hide its fix (T-551).
pub const MAX_LINE: usize = 1000;

/// A value for a terminal.
pub fn display(s: &str) -> String {
    display_max(s, MAX)
}

/// A text for a terminal, cut at `max` bytes.
pub fn display_max(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let piece = if c.is_control() { format!("\\x{:02x}", u32::from(c) & 0xff) } else { c.to_string() };
        if out.len() + piece.len() > max {
            out.push_str("...");
            break;
        }
        out.push_str(&piece);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_escape_prints_as_text_and_a_long_value_is_cut() {
        assert_eq!(display("a\u{1b}[31mb"), "a\\x1b[31mb");
        assert_eq!(display("tab\there"), "tab\\x09here");
        let long = display(&"x".repeat(500));
        assert!(long.len() <= MAX + 3 && long.ends_with("..."), "{}", long.len());
        assert_eq!(display("plain"), "plain");
        // A line of podrunner's own text keeps its end (T-551).
        let line = display_max(&format!("{} fix: x", "y".repeat(300)), MAX_LINE);
        assert!(line.ends_with("fix: x"), "{line}");
    }
}
