use std::sync::OnceLock;

use regex::Regex;

pub fn strip_ansi_escapes(input: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\x1b\[[\x20-\x3f]*[\x40-\x7e]").unwrap());
    re.replace_all(input, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_escape_codes() {
        let input = "\x1b[31mError\x1b[0m: something failed";
        assert_eq!(strip_ansi_escapes(input), "Error: something failed");
    }

    #[test]
    fn returns_plain_string_unchanged() {
        let input = "hello world";
        assert_eq!(strip_ansi_escapes(input), "hello world");
    }

    #[test]
    fn returns_empty_string_unchanged() {
        assert_eq!(strip_ansi_escapes(""), "");
    }

    #[test]
    fn strips_multiple_ansi_codes() {
        let input = "\x1b[1m\x1b[31mBold Red\x1b[0m and \x1b[32mGreen\x1b[0m";
        assert_eq!(strip_ansi_escapes(input), "Bold Red and Green");
    }

    #[test]
    fn ansi_only_string_becomes_empty() {
        let input = "\x1b[31m\x1b[0m";
        assert_eq!(strip_ansi_escapes(input), "");
    }

    #[test]
    fn strips_csi_with_intermediate_bytes() {
        // \x1b[?25l (hide cursor), \x1b[?25h (show cursor)
        let input = "\x1b[?25lhello\x1b[?25h";
        assert_eq!(strip_ansi_escapes(input), "hello");
    }

    #[test]
    fn strips_csi_with_tilde_terminator() {
        // \x1b[3~ (Delete key sequence)
        let input = "before\x1b[3~after";
        assert_eq!(strip_ansi_escapes(input), "beforeafter");
    }
}
