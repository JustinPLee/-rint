// string -> list of located tokens

use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::location::{Located, Location, Span, loc};
use crate::token::Token;
use crate::token::string_to_keyword;

#[derive(PartialEq, Clone, Copy, Debug)]
struct Cursor {
    pub pos: usize,
    pub line: u32,
    pub col: u32,
}

pub struct Lexer<'a> {
    input: &'a [u8],
    filename: String,
    start_cursor: Cursor,
    // points to next unread char
    end_cursor: Cursor,
    errors: Vec<LexError>,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum LexErrorKind {
    UnexpectedCharacter(char),
    UnterminatedMultiLineComment,
    IntOverflow,
    Eof,
}

#[derive(PartialEq, Clone, Debug)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub description: String,
    pub location: Location,
}

fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\n' | '\r' | '\t')
}

fn is_newline(c: char) -> bool {
    matches!(c, '\n' | '\r')
}

fn is_start_of_ident(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_body_of_ident(c: char) -> bool {
    is_start_of_ident(c) || c.is_ascii_digit()
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

impl<'a> Lexer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self::with_file(input, "<input>")
    }

    pub fn with_file(input: &'a [u8], filename: &str) -> Self {
        Self {
            input,
            filename: filename.to_string(),
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
            errors: Vec::new(),
        }
    }

    pub fn errors(&self) -> Option<&[LexError]> {
        if self.errors.is_empty() {
            None
        } else {
            Some(&self.errors)
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.end_cursor.pos).map(|&b| b as char)
    }

    fn peek2(&self) -> Option<char> {
        self.input.get(self.end_cursor.pos + 1).map(|&b| b as char)
    }

    fn peek3(&self) -> Option<char> {
        self.input.get(self.end_cursor.pos + 2).map(|&b| b as char)
    }

    /// calls next() n times
    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            let _ = self.next();
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
        self.advance(n_chars);
        self.collapse_cursors();
    }

    /// start tracking a new token
    /// does not process the current token being lexed
    fn collapse_cursors(&mut self) {
        assert!(self.start_cursor.pos <= self.end_cursor.pos);
        self.start_cursor = self.end_cursor;
    }

    fn lex_token(&mut self) -> Result<Located<Token>, LexError> {
        self.skip_whitespace();
        self.collapse_cursors();

        match self.peek() {
            Some(c) if is_start_of_ident(c) => Ok(self.lex_ident()),
            Some(c) if c.is_ascii_digit() => self.lex_number(),
            Some(_) => self.lex_symbol(),
            None => Err(self.make_error(LexErrorKind::Eof, None)),
        }
    }

    fn lex_ident(&mut self) -> Located<Token> {
        while let Some(c) = self.peek()
            && is_body_of_ident(c)
        {
            self.next();
        }

        let utf8 = &self.input[self.start_cursor.pos..self.end_cursor.pos];
        let s = str::from_utf8(utf8).expect("valid utf8").to_string();

        if let Some(keyword) = string_to_keyword(&s) {
            self.add_location(keyword)
        } else {
            self.add_location(Token::Ident(s))
        }
    }

    fn lex_number(&mut self) -> Result<Located<Token>, LexError> {
        // hex: 0[xX]...
        if self.peek() == Some('0') && matches!(self.peek2(), Some('x' | 'X')) {
            // 0[xX]
            self.advance(2);

            let hex_start = self.end_cursor.pos;
            while let Some(c) = self.peek()
                && c.is_ascii_hexdigit()
            {
                self.next();
            }

            // detect dangling hex numbers. ex: 0x or 0xG
            if self.end_cursor.pos == hex_start {
                return Err(self.make_error(
                    LexErrorKind::UnexpectedCharacter(self.peek().unwrap()),
                    Some("incomplete hexadecimal number"),
                ));
            }

            // detect invalid characters following hex numbers. ex: 0x2a
            // use ident as the detecting condition because other symbols could be terminators
            if let Some(next) = self.peek()
                && is_start_of_ident(next)
            {
                return Err(self.make_error(
                    LexErrorKind::UnexpectedCharacter(next),
                    Some("bad hexadecimal number"),
                ));
            }

            let utf8 = &self.input[hex_start..self.end_cursor.pos];
            let s = str::from_utf8(utf8).expect("valid utf8");
            if let Ok(val) = i32::from_str_radix(s, 16) {
                return Ok(self.add_location(Token::Num(val)));
            } else {
                return Err(self.make_error(
                    LexErrorKind::IntOverflow,
                    Some("overflowed hexadecimal number"),
                ));
            }
        }

        // decimal
        while let Some(c) = self.peek()
            && c.is_ascii_digit()
        {
            self.next();
        }

        if let Some(next) = self.peek()
            && is_start_of_ident(next)
        {
            let kind = LexErrorKind::UnexpectedCharacter(next);
            self.next();
            return Err(self.make_error(kind, Some("bad decimal number")));
        }

        let utf8 = &self.input[self.start_cursor.pos..self.end_cursor.pos];
        let s = str::from_utf8(utf8).expect("valid utf8");
        if let Ok(val) = s.parse::<i32>() {
            Ok(self.add_location(Token::Num(val)))
        } else {
            Err(self.make_error(LexErrorKind::IntOverflow, Some("bad decimal number")))
        }
    }

    fn lex_symbol(&mut self) -> Result<Located<Token>, LexError> {
        let Some(c) = self.peek() else {
            return Err(self.make_error(LexErrorKind::Eof, Some("symbol")));
        };

        let token = match c {
            '+' => self.lex_three(Token::Plus, '+', Token::DoublePlus, '=', Token::PlusEq),
            '-' => self.lex_minus(),
            '*' => self.lex_two(Token::Star, '=', Token::StarEq),
            '%' => self.lex_two(Token::Mod, '=', Token::ModEq),
            '=' => self.lex_two(Token::Eq, '=', Token::EqualEq),
            '!' => self.lex_two(Token::Exclam, '=', Token::NotEq),
            '&' => self.lex_three(Token::BitAnd, '&', Token::LogicAnd, '=', Token::AndEq),
            '|' => self.lex_three(Token::BitOr, '|', Token::LogicOr, '=', Token::OrEq),
            '^' => self.lex_two(Token::BitXor, '=', Token::XorEq),
            '<' => self.lex_shift(
                '<',
                Token::Less,
                Token::LessEq,
                Token::LShift,
                Token::LShiftEq,
            ),
            '>' => self.lex_shift(
                '>',
                Token::Greater,
                Token::GreaterEq,
                Token::RShift,
                Token::RShiftEq,
            ),
            '(' => self.lex_one(Token::LParen),
            ')' => self.lex_one(Token::RParen),
            '{' => self.lex_one(Token::LBrace),
            '}' => self.lex_one(Token::RBrace),
            '[' => self.lex_one(Token::LBracket),
            ']' => self.lex_one(Token::RBracket),
            ';' => self.lex_one(Token::Semicolon),
            ':' => self.lex_one(Token::Colon),
            '?' => self.lex_one(Token::Question),
            ',' => self.lex_one(Token::Comma),
            '.' => self.lex_one(Token::Dot),
            '/' => return self.lex_slash(), // special handling for comments
            _ => {
                self.next();
                return Err(self.make_error(LexErrorKind::UnexpectedCharacter(c), None));
            }
        };

        Ok(token)
    }

    fn lex_one(&mut self, token: Token) -> Located<Token> {
        self.next();
        self.add_location(token)
    }

    fn lex_minus(&mut self) -> Located<Token> {
        if self.peek2() == Some('>') {
            self.lex_two(Token::Minus, '>', Token::Arrow)
        } else {
            self.lex_three(Token::Minus, '-', Token::DoubleMinus, '=', Token::MinusEq)
        }
    }

    /// if current char is `one` and next char is `next`, lex `two`
    /// otherwise, lex `one`
    fn lex_two(&mut self, one: Token, next: char, two: Token) -> Located<Token> {
        if self.peek2() == Some(next) {
            self.advance(2);
            self.add_location(two)
        } else {
            self.next();
            self.add_location(one)
        }
    }

    /// if current char is `base` and next char is `one`, lex `one_token`
    /// if current char is `base` and next char is `two`, lex `two_token`
    /// otherwise, lex `base`
    fn lex_three(
        &mut self,
        base: Token,
        one: char,
        one_token: Token,
        two: char,
        two_token: Token,
    ) -> Located<Token> {
        match self.peek2() {
            Some(c) if c == one => {
                self.advance(2);
                self.add_location(one_token)
            }
            Some(c) if c == two => {
                self.advance(2);
                self.add_location(two_token)
            }
            _ => {
                self.next();
                self.add_location(base)
            }
        }
    }

    fn lex_shift(
        &mut self,
        shift_char: char,
        single: Token,
        comparison: Token,
        shift: Token,
        shift_assign: Token,
    ) -> Located<Token> {
        match (self.peek2(), self.peek3()) {
            (Some(c), Some('=')) if c == shift_char => {
                self.advance(3);
                self.add_location(shift_assign)
            }
            (Some('='), _) => {
                self.advance(2);
                self.add_location(comparison)
            }
            (Some(c), _) if c == shift_char => {
                self.advance(2);
                self.add_location(shift)
            }
            _ => {
                self.next();
                self.add_location(single)
            }
        }
    }

    fn lex_slash(&mut self) -> Result<Located<Token>, LexError> {
        match self.peek2() {
            Some('/') => {
                self.skip_line_comment();
                self.lex_token()
            }
            Some('*') => {
                self.skip_block_comment()?;
                self.lex_token()
            }
            Some('=') => {
                self.advance(2);
                Ok(self.add_location(Token::DivEq))
            }
            _ => {
                self.next();
                Ok(self.add_location(Token::Div))
            }
        }
    }

    fn make_error(&mut self, kind: LexErrorKind, description: Option<&str>) -> LexError {
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

        let description = match description {
            Some(description) => format!("{base_description}; {description}"),
            None => base_description,
        };

        LexError {
            kind,
            description,
            location: self.get_location(),
        }
    }

    fn skip_error(&mut self) {
        // skip to the first "terminating" character
        while let Some(c) = self.peek() {
            if is_whitespace(c) || matches!(c, ',' | ';' | '}' | ')') {
                break;
            }
            self.skip(1);
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek()
            && !is_newline(c)
        {
            self.skip(1);
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        self.advance(2); // /*

        while let Some(c) = self.peek() {
            if c == '*' && self.peek2() == Some('/') {
                self.skip(2); // */
                return Ok(());
            }
            self.skip(1);
        }

        Err(self.make_error(LexErrorKind::UnterminatedMultiLineComment, None))
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek()
            && is_whitespace(c)
        {
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

                    self.errors.push(err);
                    self.skip_error();
                }
            }
        }
        tokens
    }

    fn lex_eof(&mut self) -> Located<Token> {
        self.add_location(Token::Eof)
    }
    fn get_location(&mut self) -> Location {
        Location::new(
            Span::new(self.start_cursor.pos as u32, self.end_cursor.pos as u32),
            self.start_cursor.line,
            self.start_cursor.col,
            self.filename.clone(),
        )
    }

    fn add_location(&mut self, token: Token) -> Located<Token> {
        loc(token, self.get_location())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_lexer<'a>(s: &'a str) -> Lexer<'a> {
        Lexer::new(s.as_bytes())
    }

    #[test]
    fn test_next() {
        let mut lx = new_lexer("");
        assert_eq!(lx.next(), None);

        let mut lx = new_lexer(" \n \r");
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

    fn raw(ltoken: Result<Located<Token>, LexError>) -> Result<Token, LexErrorKind> {
        ltoken.map(|ltok| ltok.data.clone()).map_err(|err| err.kind)
    }

    #[test]
    fn test_eof() {
        let mut lx = new_lexer("");
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_lex_token() {
        let mut lx = new_lexer("int main() 0x44 a1_ \n11=--- /*\nabc\n*/ //2+2");
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
    fn test_tokens_and_locations() {
        let source = r#"
            int main(bool flag, int x) {
                if (flag && x >= 0) {
                    return x == 0 ? 1 : x + 0x10;
                } else {
                    return 0;
                }
            }
        "#;
        let mut lexer = new_lexer(source);
        let tokens = lexer.lex();

        assert!(lexer.errors().is_none());
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_skip_line_comment() {
        let mut lx = new_lexer("break //   break");
        assert_eq!(raw(lx.lex_token()), Ok(Token::Break));
        lx.skip_line_comment();
        assert_eq!(raw(lx.lex_token()), Err(LexErrorKind::Eof));
    }

    #[test]
    fn test_skip_block_comment() {
        let mut lx = new_lexer("break /*   break*/ /* b");
        assert_eq!(raw(lx.lex_token()), Ok(Token::Break));
        assert_eq!(lx.skip_block_comment(), Ok(()));

        assert_eq!(
            lx.skip_block_comment().map_err(|lx_err| lx_err.kind),
            Err(LexErrorKind::UnterminatedMultiLineComment)
        );
    }

    #[test]
    fn test_skip_whitespace() {
        let mut lx = new_lexer(" \n\r\t\ta\r");
        lx.skip_whitespace();
        assert_eq!(lx.next(), Some('a'));
    }

    #[test]
    fn test_bad_char() {
        let mut lx = new_lexer("aaa@ aa");
        assert_eq!(raw(lx.lex_token()), Ok(Token::Ident("aaa".to_string())));
        assert_eq!(
            raw(lx.lex_token()),
            Err(LexErrorKind::UnexpectedCharacter('@'))
        );
    }

    #[test]
    fn test_integer_overflow() {
        let mut dec = new_lexer("2147483648");
        assert_eq!(raw(dec.lex_token()), Err(LexErrorKind::IntOverflow));

        let mut hex = new_lexer("0x100000000");
        assert_eq!(raw(hex.lex_token()), Err(LexErrorKind::IntOverflow));
    }

    #[test]
    fn test_bad_hex() {
        for (source, ch) in [("0xG", 'G'), ("0x_", '_'), ("0x1G", 'G'), ("0X1_", '_')] {
            let mut lx = new_lexer(source);
            assert_eq!(
                raw(lx.lex_token()),
                Err(LexErrorKind::UnexpectedCharacter(ch)),
            );
        }
    }
}
