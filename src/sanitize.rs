//! Strips control characters from text output, so text that comes from the
//! API (labels, explanations, error messages) cannot move the cursor, change
//! colours or otherwise drive the user's terminal. JSON output needs no
//! filtering: serde_json escapes control characters.

use std::borrow::Cow;
use std::io::{self, Write};

/// Newlines and tabs are layout; every other control character is dropped,
/// including ESC, DEL and the C1 range.
fn allowed(c: char) -> bool {
    !c.is_control() || c == '\n' || c == '\t'
}

pub fn clean(s: &str) -> Cow<'_, str> {
    if s.chars().all(allowed) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.chars().filter(|&c| allowed(c)).collect())
    }
}

/// A writer that passes text through `clean`.
pub struct Sanitized<W: Write>(pub W);

impl<W: Write> Write for Sanitized<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Formatted output arrives as whole `str` fragments. Anything that is
        // not valid UTF-8 is filtered byte by byte instead.
        match std::str::from_utf8(buf) {
            Ok(s) => self.0.write_all(clean(s).as_bytes())?,
            Err(_) => {
                let kept: Vec<u8> = buf
                    .iter()
                    .copied()
                    .filter(|&b| b >= 0x20 && b != 0x7f || b == b'\n' || b == b'\t')
                    .collect();
                self.0.write_all(&kept)?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ordinary_text() {
        assert!(matches!(
            clean("Ƀitcoin · 1,000 BTC\n\tok"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn strips_escape_sequences_and_control_characters() {
        assert_eq!(clean("\u{1b}[31mred\u{1b}[0m"), "[31mred[0m");
        assert_eq!(clean("a\u{7}b\rc\u{7f}d\u{9b}e"), "abcde");
    }

    #[test]
    fn filters_through_the_writer() {
        let mut out = Sanitized(Vec::new());
        let label = "\u{1b}]0;title\u{7}pool";
        writeln!(out, "label {label}").unwrap();
        assert_eq!(String::from_utf8(out.0).unwrap(), "label ]0;titlepool\n");
    }
}
