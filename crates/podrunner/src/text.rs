//! Text from a forge or a unit, made safe for a terminal: a control
//! character prints as an escape (`\x1b`), and a value is cut at 200
//! bytes.

/// The most bytes of a display value.
pub const MAX: usize = 200;

/// A value for a terminal.
pub fn display(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let piece = if c.is_control() { format!("\\x{:02x}", u32::from(c) & 0xff) } else { c.to_string() };
        if out.len() + piece.len() > MAX {
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
    }
}
