use crate::{
    context::Context,
    diagnostic::{Diagnostic, DiagnosticKind},
    location::{Located, Location, Span, loc},
    token::Token,
    token::sym_to_keyword,
};

#[derive(PartialEq, Clone, Copy, Debug)]
struct Cursor {
    pub pos: usize,
    pub line: u32,
    pub col: u32,
}

pub struct Lexer<'ctx, 'a> {
    input: &'a [u8],
    start_cursor: Cursor,
    // points to next unread char
    end_cursor: Cursor,
    ctx: &'ctx mut Context,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum LexErrorKind {
    Eof,
    UnexpectedCharacter(char),
    UnterminatedMultiLineComment,
    IntOverflow,
}

#[derive(PartialEq, Clone, Debug)]
struct LexError {
    pub kind: LexErrorKind,
    pub description: String,
    pub location: Location,
}

impl From<LexError> for Diagnostic {
    fn from(err: LexError) -> Self {
        Diagnostic {
            description: err.description,
            kind: DiagnosticKind::Error,
            location: err.location,
        }
    }
}

impl<'ctx, 'a> Lexer<'ctx, 'a> {
    pub fn new(ctx: &'ctx mut Context, input: &'a [u8]) -> Self {
        Self {
            input,
            start_cursor: Cursor {
                pos: 0,
                line: 1,
                col: 1,
            },
            end_cursor: Cursor {
                pos: 0,
                line: 1,
                col: 1,
            },
            ctx,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input
            .get(self.end_cursor.pos as usize)
            .map(|&b| b as char)
    }

    fn peek_next(&self) -> Option<char> {
        self.input
            .get((self.end_cursor.pos + 1) as usize)
            .map(|&b| b as char)
    }

    fn peek_next_next(&self) -> Option<char> {
        self.input
            .get((self.end_cursor.pos + 2) as usize)
            .map(|&b| b as char)
    }

    /// Moves next `n` steps.
    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            let _c = self.next();
        }
    }

    /// Advances the lexer one character. returns `None` or the skipped character.
    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        if is_newline(c) {
            self.end_cursor.line += 1;
            self.end_cursor.col = 1;
        } else {
            self.end_cursor.col += 1;
        }
        self.end_cursor.pos += 1;

        Some(c)
    }

    fn skip(&mut self, n_chars: usize) {
        for _ in 0..n_chars {
            self.advance(1);
        }
        self.collapse_cursors();
    }

    fn collapse_cursors(&mut self) {
        assert!(self.start_cursor.pos <= self.end_cursor.pos);
        self.start_cursor = self.end_cursor;
    }

    fn lex_token(&mut self) -> Result<Located<Token>, LexError> {
        self.skip_whitespace();
        self.collapse_cursors();

        match self.peek() {
            Some(c) if is_ident_start(c) => Ok(self.lex_ident()),
            Some(c) if c.is_ascii_digit() => self.lex_number(),
            Some(_) => self.lex_symbol(),
            None => Err(self.process_error(LexErrorKind::Eof, None)),
        }
    }

    fn lex_ident(&mut self) -> Located<Token> {
        assert!(matches!(self.peek(), Some(c) if is_ident_start(c)));

        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                self.advance(1);
            } else {
                break;
            }
        }

        let utf8 = &self.input[self.start_cursor.pos..self.end_cursor.pos];
        let s = str::from_utf8(utf8).expect("valid utf8").to_string();

        if let Some(keyword) = sym_to_keyword(&s) {
            self.loc(keyword)
        } else {
            self.loc(Token::Ident(s))
        }
    }

    fn lex_number(&mut self) -> Result<Located<Token>, LexError> {
        assert!(matches!(self.peek(), Some(c) if c.is_ascii_digit()));

        // hex: 0[xX]...
        if self.peek() == Some('0') && matches!(self.peek_next(), Some('x' | 'X')) {
            // consume 0x prefix
            self.advance(2);

            let non_prefix_start = self.end_cursor.pos;

            while let Some(c) = self.peek()
                && c.is_ascii_hexdigit()
            {
                self.advance(1);
            }

            let next = self.peek().unwrap_or(' ');
            if is_ident_start(next) || self.end_cursor.pos == non_prefix_start {
                let kind = LexErrorKind::UnexpectedCharacter(self.peek().expect("char"));
                // skip 0x or _ after hex
                let n_chars = if is_ident_start(next) { 1 } else { 2 };
                self.advance(n_chars);
                return Err(self.process_error(kind, Some("hexadecimal number".to_string())));
            }

            let utf8 = &self.input[non_prefix_start..self.end_cursor.pos];
            let s = str::from_utf8(utf8).expect("valid utf8");
            if let Ok(val) = i32::from_str_radix(&s, 16) {
                return Ok(self.loc(Token::Num(val)));
            } else {
                return Err(self.process_error(
                    LexErrorKind::IntOverflow,
                    Some("hexadecimal number".to_string()),
                ));
            }
        }

        // decimal
        while let Some(c) = self.peek()
            && c.is_ascii_digit()
        {
            self.advance(1);
        }

        let next = self.peek().unwrap_or(' ');
        if is_ident_start(next) {
            let kind = LexErrorKind::UnexpectedCharacter(self.peek().unwrap());
            self.advance(1);
            return Err(self.process_error(kind, Some("decimal number".to_string())));
        }

        let utf8 = &self.input[self.start_cursor.pos..self.end_cursor.pos];
        let s = str::from_utf8(utf8).expect("valid utf8");
        if let Ok(val) = i32::from_str_radix(&s, 10) {
            return Ok(self.loc(Token::Num(val)));
        } else {
            return Err(self.process_error(
                LexErrorKind::IntOverflow,
                Some("decimal number".to_string()),
            ));
        }
    }

    fn lex_symbol(&mut self) -> Result<Located<Token>, LexError> {
        let Some(c) = self.peek() else {
            self.advance(1);
            return Err(self.process_error(LexErrorKind::Eof, Some("symbol".to_string())));
        };

        match c {
            '+' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::PlusEq))
                } else if self.peek_next() == Some('+') {
                    self.advance(2);
                    Ok(self.loc(Token::DoublePlus))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Plus))
                }
            }
            '-' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::MinusEq))
                } else if self.peek_next() == Some('-') {
                    self.advance(2);
                    Ok(self.loc(Token::DoubleMinus))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Minus))
                }
            }
            '*' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::TimesEq))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Times))
                }
            }
            '/' => {
                if self.peek_next() == Some('/') {
                    self.skip_line_comment();
                    self.lex_token()
                } else if self.peek_next() == Some('*') {
                    self.skip_block_comment()?;
                    self.lex_token()
                } else if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::DivEq))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Div))
                }
            }
            '%' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::ModEq))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Mod))
                }
            }
            '=' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::EqualEq))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Eq))
                }
            }

            '(' => {
                self.advance(1);
                Ok(self.loc(Token::LParen))
            }
            ')' => {
                self.advance(1);
                Ok(self.loc(Token::RParen))
            }
            '{' => {
                self.advance(1);
                Ok(self.loc(Token::LBrace))
            }
            '}' => {
                self.advance(1);
                Ok(self.loc(Token::RBrace))
            }
            ';' => {
                self.advance(1);
                Ok(self.loc(Token::Semicolon))
            }
            ':' => {
                self.advance(1);
                Ok(self.loc(Token::Colon))
            }
            '?' => {
                self.advance(1);
                Ok(self.loc(Token::Question))
            }
            '!' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::NotEq))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Exclam))
                }
            }
            '&' => {
                if self.peek_next() == Some('&') {
                    self.advance(2);
                    Ok(self.loc(Token::LogicAnd))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::BitAnd))
                }
            }
            '|' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::OrEq))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::BitOr))
                }
            }
            '^' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::XorEq))
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::BitXor))
                }
            }
            '<' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::LessEq))
                } else if self.peek_next() == Some('<') {
                    if self.peek_next_next() == Some('=') {
                        self.advance(3);
                        return Ok(self.loc(Token::LShiftEq));
                    } else {
                        self.advance(2);
                        return Ok(self.loc(Token::LShift));
                    }
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Less))
                }
            }
            '>' => {
                if self.peek_next() == Some('=') {
                    self.advance(2);
                    Ok(self.loc(Token::GreaterEq))
                } else if self.peek_next() == Some('>') {
                    if self.peek_next_next() == Some('=') {
                        self.advance(3);
                        return Ok(self.loc(Token::RShiftEq));
                    } else {
                        self.advance(2);
                        return Ok(self.loc(Token::RShift));
                    }
                } else {
                    self.advance(1);
                    Ok(self.loc(Token::Greater))
                }
            }
            _ => {
                self.advance(1);
                Err(self.process_error(LexErrorKind::UnexpectedCharacter(c), None))
            }
        }
    }

    fn process_error(&mut self, kind: LexErrorKind, description: Option<String>) -> LexError {
        let base_description = match kind {
            LexErrorKind::UnexpectedCharacter(ch) => {
                format!("unexpected character `{ch}`")
            }
            LexErrorKind::Eof => "EOF".to_string(),
            LexErrorKind::UnterminatedMultiLineComment => {
                "unterminated multi-line comment".to_string()
            }
            LexErrorKind::IntOverflow => "integer overflow".to_string(),
        };

        let concatted_description = if let Some(description) = description {
            format!("{} -- expected {}", base_description, description)
        } else {
            base_description
        };
        LexError {
            kind,
            description: concatted_description,
            location: self.get_location(),
        }
    }

    fn skip_error(&mut self) {
        while let Some(c) = self.peek() {
            if is_whitespace(c) || matches!(c, ';' | '}' | ')') {
                break;
            }
            self.skip(1);
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if is_newline(c) {
                break;
            }
            self.skip(1);
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        self.advance(2); // skip /*

        while let Some(c) = self.peek() {
            if c == '*' && self.peek_next() == Some('/') {
                self.skip(2);
                return Ok(());
            }
            self.skip(1);
        }

        Err(self.process_error(LexErrorKind::UnterminatedMultiLineComment, None))
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if !is_whitespace(c) {
                break;
            }
            self.skip(1);
        }
    }

    pub fn lex(&mut self) -> Vec<Located<Token>> {
        let mut tokens = Vec::new();

        loop {
            match self.lex_token() {
                Ok(tok) => tokens.push(tok),
                Err(err) => {
                    if err.kind == LexErrorKind::Eof {
                        tokens.push(self.lex_eof());
                        break;
                    }

                    self.emit_diag(err.into());
                    self.skip_error();
                }
            }
        }
        tokens
    }

    fn emit_diag(&mut self, error: Diagnostic) {
        self.ctx.emit_diag(error);
    }

    fn get_location(&mut self) -> Location {
        Location::new(
            Span::new(self.start_cursor.pos as u32, self.end_cursor.pos as u32),
            self.start_cursor.line,
            self.start_cursor.col,
            "file1".to_string(), // temporary
        )
    }

    fn lex_eof(&mut self) -> Located<Token> {
        self.loc(Token::Eof)
    }

    fn loc(&mut self, token: Token) -> Located<Token> {
        loc(token, self.get_location())
    }
}

fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\n' | '\r' | '\t')
}

fn is_newline(c: char) -> bool {
    matches!(c, '\n' | '\r')
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    fn new_lexer<'ctx, 'a>(ctx: &'ctx mut Context, s: &'a str) -> Lexer<'ctx, 'a> {
        Lexer::new(ctx, s.as_bytes())
    }

    #[test]
    fn test_next() {
        let mut ctx = Context::default();

        let mut lx = new_lexer(&mut ctx, "");
        assert_eq!(lx.next(), None);

        let mut lx = new_lexer(&mut ctx, " \n \r");
        lx.next(); // ' '
        assert_eq!(lx.end_cursor.col, 2);
        assert_eq!(lx.end_cursor.line, 1);

        lx.next(); // '\n'
        assert_eq!(lx.end_cursor.col, 1);
        assert_eq!(lx.end_cursor.line, 2);

        lx.next(); // ' '
        assert_eq!(lx.end_cursor.col, 2);
        assert_eq!(lx.end_cursor.line, 2);

        lx.next(); // '\r'
        assert_eq!(lx.end_cursor.col, 1);
        assert_eq!(lx.end_cursor.line, 3);
    }

    fn raw(mtoken: Result<Located<Token>, LexError>) -> Result<Token, LexErrorKind> {
        mtoken
            .map(|mtok| mtok.0.clone())
            .map_err(|lex_err| lex_err.kind)
    }

    #[test]
    fn test_eof() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "");
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_lex_token() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "int main() 0x44 a1_ \n11=--- /*\nabc\n*/ //2+2");
        assert_eq!(raw(lx.lex_token()), Ok(Token::Int));
        assert_eq!(raw(lx.lex_token()), Ok(Token::Ident("main".to_string())));
        assert_eq!(raw(lx.lex_token()), Ok(Token::LParen));
        assert_eq!(raw(lx.lex_token()), Ok(Token::RParen));
        assert_eq!(raw(lx.lex_token()), Ok(Token::Num(68)));
        assert_eq!(raw(lx.lex_token()), Ok(Token::Ident("a1_".to_string())));
        assert_eq!(raw(lx.lex_token()), Ok(Token::Num(11)));
        assert_eq!(raw(lx.lex_token()), Ok(Token::Eq));
        assert_eq!(raw(lx.lex_token()), Ok(Token::DoubleMinus));
        assert_eq!(raw(lx.lex_token()), Ok(Token::Minus));
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_lex_ident() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "");
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_lex_number() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "");
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_lex_symbol() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "");
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_skip_line_comment() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "break //   break");
        assert_eq!(raw(lx.lex_token()), Ok(Token::Break));
        lx.skip_line_comment();
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_skip_block_comment() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "break /*   break*/ /* b");
        assert_eq!(raw(lx.lex_token()), Ok(Token::Break));
        assert_eq!(lx.skip_block_comment(), Ok(()));

        // check unterminated block comment
        assert_eq!(
            lx.skip_block_comment().map_err(|lx_err| lx_err.kind),
            Err(LexErrorKind::UnterminatedMultiLineComment)
        );
    }

    #[test]
    fn test_skip_whitespace() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, " \n\r\t\ta\r");
        lx.skip_whitespace();
        assert_eq!(lx.next(), Some('a'));
    }

    #[test]
    fn test_bad_char() {
        let mut ctx = Context::default();
        let mut lx = new_lexer(&mut ctx, "aaa@ aa");
        assert_eq!(raw(lx.lex_token()), Ok(Token::Ident("aaa".to_string())));
        assert!(lx.lex_token().is_err());
    }
}
