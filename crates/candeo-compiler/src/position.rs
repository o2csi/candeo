//! Positions as the author reads them.

use crate::Diagnostic;

/// 1-based line and column of a byte offset, as TypeScript's
/// `getLineAndCharacterOfPosition` counts them: lines broken by `\n`, `\r\n`,
/// `\r`, U+2028 and U+2029, columns in UTF-16 code units. Oxc's spans are UTF-8
/// byte offsets: an accent earlier on the line would otherwise shift the
/// column, and Monaco would underline the wrong place.
pub(crate) fn line_column(source: &str, offset: usize) -> (u32, u32) {
    let mut end = offset.min(source.len());
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let mut line = 1u32;
    let mut column = 0u32;
    let mut chars = source[..end].chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                column = 0;
            }
            '\n' | '\u{2028}' | '\u{2029}' => {
                line += 1;
                column = 0;
            }
            c => column += c.len_utf16() as u32,
        }
    }
    (line, column + 1)
}

/// In the order the author reads the file.
pub(crate) fn sorted(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by_key(|d| (d.line, d.column));
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_count_utf16_units_like_typescript() {
        // 'é' is two bytes in UTF-8, one unit in UTF-16; '😀' four and two.
        assert_eq!(line_column("é = 1", 2), (1, 2));
        assert_eq!(line_column("😀x", 4), (1, 3));
    }

    #[test]
    fn every_line_break_typescript_knows_starts_a_line() {
        assert_eq!(line_column("a\r\nb", 3), (2, 1));
        assert_eq!(line_column("a\rb", 2), (2, 1));
        assert_eq!(line_column("a\u{2028}b", 4), (2, 1));
    }

    #[test]
    fn an_offset_inside_a_character_or_past_the_end_stays_in_the_source() {
        assert_eq!(line_column("é", 1), (1, 1));
        assert_eq!(line_column("ab", 99), (1, 3));
    }
}
