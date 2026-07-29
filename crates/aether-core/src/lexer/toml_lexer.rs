use super::common::{skip_quoted, skip_whitespace};
use super::{LexemeSpan, Lexer, TokenKind};

/// TOML 词法分析器
pub struct TomlLexer;

impl TomlLexer {
    pub fn new() -> Self {
        Self
    }

    fn lex_next(&self, bytes: &[u8], pos: usize) -> (LexemeSpan, usize) {
        if pos >= bytes.len() {
            return (LexemeSpan::new(pos, 0, TokenKind::EOF), pos);
        }

        let ch = bytes[pos];

        match ch {
            b' ' | b'\t' | b'\r' => {
                let end = skip_whitespace(bytes, pos);
                (LexemeSpan::new(pos, end - pos, TokenKind::Whitespace), end)
            }
            b'\n' => (LexemeSpan::new(pos, 1, TokenKind::Newline), pos + 1),
            b'#' => {
                let end = skip_line_comment(bytes, pos);
                (LexemeSpan::new(pos, end - pos, TokenKind::LineComment), end)
            }
            b'[' => {
                // 检测表头 [[table]] 或 [table]
                let mut i = pos + 1;
                let is_array_table = i < bytes.len() && bytes[i] == b'[';
                if is_array_table {
                    i += 1;
                }
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if is_array_table && i < bytes.len() && i + 1 < bytes.len() && bytes[i + 1] == b']'
                {
                    i += 2;
                } else if i < bytes.len() {
                    i += 1;
                }
                (LexemeSpan::new(pos, i - pos, TokenKind::TomlTable), i)
            }
            b'"' => {
                let end = skip_quoted(bytes, pos, b'"');
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::StringLiteral),
                    end,
                )
            }
            b'\'' => {
                let end = skip_literal_string(bytes, pos);
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::StringLiteral),
                    end,
                )
            }
            b'0'..=b'9' => {
                let end = skip_number_or_date(bytes, pos);
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::NumberLiteral),
                    end,
                )
            }
            b'+' | b'-' => {
                // H-11: `+`/`-` 仅在后跟数字时才作为数字起始，
                // 否则作为标点符号（如数组中的 `-` 表项、或二元运算符）。
                if pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit() {
                    let end = skip_number_or_date(bytes, pos);
                    (
                        LexemeSpan::new(pos, end - pos, TokenKind::NumberLiteral),
                        end,
                    )
                } else {
                    (LexemeSpan::new(pos, 1, TokenKind::Punctuation), pos + 1)
                }
            }
            b't' | b'f' => {
                let end = skip_bool(bytes, pos);
                let text = std::str::from_utf8(&bytes[pos..end]).unwrap_or("");
                let kind = if text == "true" || text == "false" {
                    TokenKind::Keyword
                } else {
                    TokenKind::Identifier
                };
                (LexemeSpan::new(pos, end - pos, kind), end)
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let end = skip_identifier(bytes, pos);
                // CORE-L01: TOML 键统一使用 Identifier 替代 JsonKey
                (LexemeSpan::new(pos, end - pos, TokenKind::Identifier), end)
            }
            b'=' | b'.' | b',' => (LexemeSpan::new(pos, 1, TokenKind::Punctuation), pos + 1),
            b'{' | b'}' => (LexemeSpan::new(pos, 1, TokenKind::Punctuation), pos + 1),
            _ => {
                let len = crate::lexer::utf8_char_len(bytes[pos]);
                (LexemeSpan::new(pos, len, TokenKind::Unknown), pos + len)
            }
        }
    }
}

impl Lexer for TomlLexer {
    fn lex_full(&self, text: &str) -> Vec<LexemeSpan> {
        let mut tokens = Vec::with_capacity(text.len() / 4 + 1);
        let bytes = text.as_bytes();
        let mut pos = 0;

        while pos < bytes.len() {
            let (token, new_pos) = self.lex_next(bytes, pos);
            tokens.push(token);
            pos = new_pos;
        }

        tokens
    }
}

impl Default for TomlLexer {
    fn default() -> Self {
        Self::new()
    }
}

fn skip_line_comment(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos + 1;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_literal_string(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos + 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn skip_number_or_date(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < bytes.len()
        && (bytes[i].is_ascii_digit()
            || bytes[i] == b'-'
            || bytes[i] == b':'
            || bytes[i] == b'T'
            || bytes[i] == b'Z'
            || bytes[i] == b'+'
            || bytes[i] == b'.'
            || bytes[i] == b'e'
            || bytes[i] == b'E')
    {
        i += 1;
    }
    i
}

fn skip_bool(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    i
}

fn skip_identifier(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
    {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_table() {
        let lexer = TomlLexer::new();
        let tokens = lexer.lex_full("[package]\nname = \"test\"");
        let table_count = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::TomlTable)
            .count();
        assert_eq!(table_count, 1);
    }

    #[test]
    fn test_toml_keys() {
        let lexer = TomlLexer::new();
        let tokens = lexer.lex_full("name = \"test\"\nversion = \"1.0\"");
        // CORE-L01: TOML 键使用 Identifier 替代 JsonKey
        let id_count = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Identifier)
            .count();
        assert_eq!(id_count, 2);
    }

    #[test]
    fn test_toml_comments() {
        let lexer = TomlLexer::new();
        let tokens = lexer.lex_full("# This is a comment\nkey = \"value\"");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::LineComment));
    }

    #[test]
    fn test_toml_empty() {
        assert!(TomlLexer::new().lex_full("").is_empty());
    }

    #[test]
    fn test_toml_array_table() {
        let tokens = TomlLexer::new().lex_full("[[array]]\nkey = 1");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::TomlTable));
    }

    #[test]
    fn test_toml_strings() {
        let tokens = TomlLexer::new().lex_full(r#""double" 'single'"#);
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.kind == TokenKind::StringLiteral)
                .count(),
            2
        );
    }

    #[test]
    fn test_toml_numbers_and_bools() {
        let tokens =
            TomlLexer::new().lex_full("key = -123.45\nflag = true\ndate = 1979-05-27T07:32:00Z");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::NumberLiteral));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword));
    }

    #[test]
    fn test_toml_punctuation() {
        let tokens = TomlLexer::new().lex_full("{ a = 1, b = 2 }");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Punctuation));
    }

    #[test]
    fn test_toml_unknown() {
        let tokens = TomlLexer::new().lex_full("@");
        assert_eq!(tokens[0].kind, TokenKind::Unknown);
    }

    #[test]
    fn test_toml_unclosed_table() {
        let tokens = TomlLexer::new().lex_full("[table");
        assert_eq!(tokens[0].kind, TokenKind::TomlTable);
    }
}
