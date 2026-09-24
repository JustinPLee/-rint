use crate::context::Context;
use crate::cst::{
    AsnOp, BinOp, Control, Decl, Expr, LExpr, LStmt, LValue, PostOp, Program, Simp, Stmt, Typ, UnOp,
};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::location::{Located, Location};
use crate::token::{LToken, Token};

// parser -> cst -> elaboration -> ast -> ...

pub struct Parser<'ctx> {
    tokens: Vec<LToken>,
    ctx: &'ctx mut Context,
}

#[derive(PartialEq, Clone, Debug)]
pub enum ParseErrorKind {
    UnexpectedToken { expected: Token, found: Token },
    UnexpectedEof { expected: Token },
    ExpectedIdent { found: Token },
    ExpectedIdentEof,
    ExpectedType { found: Token },
    InvalidExpression,
    InvalidStatement,
}

#[derive(PartialEq, Clone, Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub description: String,
    pub location: Location,
}

impl From<ParseError> for Diagnostic {
    fn from(err: ParseError) -> Self {
        Diagnostic {
            description: err.description,
            kind: DiagnosticKind::Error,
            location: err.location,
        }
    }
}

type ParseResult<T> = Result<T, ParseError>;

impl<'ctx> Parser<'ctx> {
    pub fn new(ctx: &'ctx mut Context, tokens: &[LToken]) -> Self {
        let tokens = tokens.iter().cloned().rev().collect();
        Self { tokens, ctx }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.last().map(|t| &t.0)
    }

    fn peek_next(&self) -> Option<&Token> {
        self.tokens.iter().rev().nth(1).map(|t| &t.0)
    }

    fn next(&mut self) -> Option<LToken> {
        self.tokens.pop()
    }

    fn current(&self) -> &LToken {
        self.tokens.last().expect("EOF token always present")
    }

    fn current_location(&self) -> &Location {
        &self.current().1
    }

    /// Consume `token` or return an error
    fn try_consume(&mut self, token: &Token) -> ParseResult<LToken> {
        match self.peek() {
            Some(got) if got == token => Ok(self.next().expect("token was just peeked")),
            Some(Token::Eof) => {
                let loc = self.current_location().clone();
                Err(self.make_error(
                    ParseErrorKind::UnexpectedEof {
                        expected: token.clone(),
                    },
                    None,
                    loc,
                ))
            }
            Some(found) => {
                let found = found.clone();
                let loc = self.current_location().clone();

                Err(self.make_error(
                    ParseErrorKind::UnexpectedToken {
                        expected: token.clone(),
                        found,
                    },
                    None,
                    loc,
                ))
            }
            None => unreachable!("EOF token always present"),
        }
    }

    fn emit_error(&mut self, err: ParseError) {
        self.ctx.emit_diag(err.into());
    }

    fn make_error(
        &self,
        kind: ParseErrorKind,
        description: Option<String>,
        location: Location,
    ) -> ParseError {
        let base_description = match &kind {
            ParseErrorKind::UnexpectedToken { expected, found } => format!(
                "expected `{}`, but found `{}`",
                expected.show(),
                found.show(),
            ),
            ParseErrorKind::UnexpectedEof { expected } => {
                format!("expected `{}`, but found EOF", expected.show(),)
            }
            ParseErrorKind::ExpectedIdent { found } => {
                format!("expected identifier, but found `{}`", found.show(),)
            }
            ParseErrorKind::ExpectedType { found } => {
                format!("expected type, but found `{}`", found.show(),)
            }
            ParseErrorKind::ExpectedIdentEof => "expected identifier, but found EOF".into(),
            ParseErrorKind::InvalidExpression => "invalid expression".into(),
            ParseErrorKind::InvalidStatement => "invalid statement".into(),
        };
        let concatted_description = if let Some(description) = description {
            format!("{} -- expected {}", base_description, description)
        } else {
            base_description
        };
        ParseError {
            kind,
            description: concatted_description,
            location,
        }
    }

    fn parse_program(&mut self) -> ParseResult<Program> {
        self.try_consume(&Token::Int)?;
        self.try_consume(&Token::Ident("main".to_string()))?;
        self.try_consume(&Token::LParen)?;
        self.try_consume(&Token::RParen)?;

        self.parse_block().map(Program)
    }

    fn parse_block(&mut self) -> ParseResult<Vec<LStmt>> {
        self.try_consume(&Token::LBrace)?;
        let stmts = self.parse_stmts()?;
        self.try_consume(&Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_stmts(&mut self) -> ParseResult<Vec<LStmt>> {
        let mut stmts = Vec::new();
        while !matches!(self.peek(), Some(Token::RBrace) | Some(Token::Eof) | None) {
            stmts.push(self.parse_stmt()?);
        }

        Ok(stmts)
    }

    fn parse_typ(&mut self) -> ParseResult<Typ> {
        match self.peek() {
            Some(Token::Int) => {
                self.try_consume(&Token::Int)?;
                Ok(Typ::Int)
            }
            Some(Token::Bool) => {
                self.try_consume(&Token::Bool)?;
                Ok(Typ::Bool)
            }
            None | _ => panic!("error todo"),
        }
    }

    fn parse_stmt(&mut self) -> ParseResult<LStmt> {
        let start = self.current_location().clone();
        match self.peek() {
            Some(Token::If) | Some(Token::While) | Some(Token::For) | Some(Token::Return) => {
                let stmt = self.parse_control()?;
                Ok(Located(Stmt::Control(stmt), start))
            }
            Some(Token::LBrace) => {
                let stmts = self.parse_block()?;
                let end = self.current_location().clone();
                Ok(Located(Stmt::Block(stmts), start.merge(&end)))
            }
            _ => {
                let end = self.current_location().clone();
                let loc = start.merge(&end);
                let res = Ok(Located(Stmt::Simp(self.parse_simp()?), loc));
                self.try_consume(&Token::Semicolon)?;
                res
            }
        }
    }

    fn parse_control(&mut self) -> ParseResult<Control> {
        match self.peek() {
            Some(Token::If) => {
                self.try_consume(&Token::If)?;
                self.try_consume(&Token::LParen)?;
                let cond = self.parse_expr()?;
                self.try_consume(&Token::RParen)?;
                let true_stmt = Box::new(self.parse_stmt()?);
                if self.try_consume(&Token::Else).is_ok() {
                    let false_stmt = Box::new(self.parse_stmt()?);
                    return Ok(Control::If {
                        cond,
                        true_stmt,
                        false_stmt,
                    });
                }
                panic!("if statements must have else");
            }
            Some(Token::While) => {
                self.try_consume(&Token::While)?;
                self.try_consume(&Token::LParen)?;
                let cond = self.parse_expr()?;
                self.try_consume(&Token::RParen)?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Control::While { cond, body })
            }
            Some(Token::For) => {
                self.try_consume(&Token::For)?;
                self.try_consume(&Token::LParen)?;
                let init = self.parse_simp().ok();
                self.try_consume(&Token::Semicolon)?;
                let cond = self.parse_expr()?;
                self.try_consume(&Token::Semicolon)?;
                let step = self.parse_simp().ok();
                self.try_consume(&Token::RParen)?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Control::For {
                    init,
                    cond,
                    step,
                    body,
                })
            }
            Some(Token::Return) => {
                self.try_consume(&Token::Return)?;
                let expr = self.parse_expr()?;
                self.try_consume(&Token::Semicolon)?;
                Ok(Control::Return(expr))
            }
            _ => panic!("error todo"),
        }
    }

    fn parse_simp(&mut self) -> ParseResult<Simp> {
        match self.peek() {
            Some(Token::Int | Token::Bool) => {
                let decl = self.parse_decl()?;
                Ok(Simp::Decl(decl))
            }
            Some(Token::Ident(_)) => match self.peek_next() {
                Some(Token::DoubleMinus) | Some(Token::DoublePlus) => self.parse_postfix(),
                _ => {
                    let name = self.parse_lvalue()?;
                    let asnop = self.parse_asnop()?;
                    let value = self.parse_expr()?;
                    Ok(Simp::Assign { name, asnop, value })
                }
            },
            _ => Ok(Simp::StmtExpr(self.parse_expr()?)),
        }
    }

    fn parse_decl(&mut self) -> ParseResult<Decl> {
        let typ = self.parse_typ()?;
        let name = self.parse_ident()?;
        if self.try_consume(&Token::Eq).is_ok() {
            let value = self.parse_expr()?;
            Ok(Decl::Init { typ, name, value })
        } else {
            Ok(Decl::Decl { typ, name })
        }
    }

    fn parse_postfix(&mut self) -> ParseResult<Simp> {
        let lvalue = self.parse_lvalue()?;
        let postop = match self.next() {
            Some(Located(Token::DoubleMinus, _)) => PostOp::DoubleMinus,
            Some(Located(Token::DoublePlus, _)) => PostOp::DoublePlus, // Some(Token::DoublePlus) => {},
            _ => panic!("error todo"),
        };
        Ok(Simp::Post {
            name: lvalue,
            postop,
        })
    }

    fn parse_ident(&mut self) -> ParseResult<String> {
        match self.next() {
            Some(Located(Token::Ident(s), _)) => Ok(s),
            Some(Located(tok, loc)) => {
                Err(self.make_error(ParseErrorKind::ExpectedIdent { found: tok }, None, loc))
            }
            None => Err(self.make_error(
                ParseErrorKind::ExpectedIdentEof,
                None,
                self.current_location().clone(),
            )),
        }
    }

    fn parse_lvalue(&mut self) -> ParseResult<LValue> {
        match self.next() {
            Some(Located(Token::Ident(name), _)) => Ok(LValue(name)),
            _ => panic!("error todo"),
        }
    }

    fn parse_asnop(&mut self) -> ParseResult<AsnOp> {
        let tok = match self.next().map_or(None, |t| Some(t.0)) {
            Some(Token::Eq) => AsnOp::Eq,
            Some(Token::PlusEq) => AsnOp::PlusEq,
            Some(Token::MinusEq) => AsnOp::MinusEq,
            Some(Token::TimesEq) => AsnOp::TimesEq,
            Some(Token::DivEq) => AsnOp::DivEq,
            Some(Token::ModEq) => AsnOp::ModEq,
            Some(Token::AndEq) => AsnOp::AndEq,
            Some(Token::XorEq) => AsnOp::XorEq,
            Some(Token::OrEq) => AsnOp::OrEq,
            Some(Token::LShiftEq) => AsnOp::LShiftEq,
            Some(Token::RShiftEq) => AsnOp::RShiftEq,
            _ => panic!("error todo"),
        };
        Ok(tok)
    }

    // pratt parsing
    fn parse_expr(&mut self) -> ParseResult<LExpr> {
        self.parse_expr_impl(0)
    }

    fn parse_expr_impl(&mut self, min_bp: u8) -> ParseResult<LExpr> {
        let mut lhs = self.parse_prefix()?;

        loop {
            let Some(tok) = self.peek() else { break };
            let Some((kind, left_bp, right_bp)) = Infix::bp(tok) else {
                break;
            };
            if left_bp < min_bp {
                break;
            }
            self.next().unwrap();

            lhs = match kind {
                Infix::Binary(op) => {
                    let rhs = self.parse_expr_impl(right_bp)?;
                    let location = lhs.1.clone().merge(&rhs.1);
                    Located(
                        Expr::BinOp {
                            op,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                        location,
                    )
                }
                Infix::Ternary => {
                    let true_expr = self.parse_expr_impl(right_bp)?;
                    self.try_consume(&Token::Colon)?;
                    let false_expr = self.parse_expr_impl(right_bp)?;
                    let location = lhs.1.clone().merge(&false_expr.1);
                    Located(
                        Expr::TernOp {
                            cond: Box::new(lhs),
                            true_expr: Box::new(true_expr),
                            false_expr: Box::new(false_expr),
                        },
                        location,
                    )
                }
            };
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> ParseResult<LExpr> {
        let Located(tok, start) = self.next().ok_or_else(|| {
            self.make_error(
                ParseErrorKind::InvalidExpression,
                Some("expression".into()),
                self.current_location().clone(),
            )
        })?;

        match tok {
            Token::True => Ok(Located(Expr::True, start)),
            Token::False => Ok(Located(Expr::False, start)),
            Token::Num(n) => Ok(Located(Expr::Int(n), start)),
            Token::Ident(name) => Ok(Located(Expr::Ident(name), start)),
            Token::LParen => {
                let expr = self.parse_expr()?;
                let Located(_, end) = self.try_consume(&Token::RParen)?;
                Ok(Located(expr.0, start.merge(&end)))
            }
            Token::Minus => {
                let (_, right_bp) = prefix_binding_power(&Token::Minus).unwrap();
                let operand = self.parse_expr_impl(right_bp)?;
                let location = start.merge(&operand.1);
                Ok(Located(
                    Expr::UnOp {
                        op: UnOp::Negate,
                        oper: Box::new(operand),
                    },
                    location,
                ))
            }
            Token::Exclam => {
                let (_, right_bp) = prefix_binding_power(&Token::Exclam).unwrap();
                let operand = self.parse_expr_impl(right_bp)?;
                let location = start.merge(&operand.1);
                Ok(Located(
                    Expr::UnOp {
                        op: UnOp::Exclam,
                        oper: Box::new(operand),
                    },
                    location,
                ))
            }
            tok => Err(self.make_error(
                ParseErrorKind::InvalidExpression,
                Some(format!("expression, found `{}`", tok.show())),
                start,
            )),
        }
    }

    /// "basic constructs" like types, numbers, idents should not synchronize
    /// let higher level constructs like statements synchronize
    // fn synchronize(&mut self) {
    //     loop {
    //         match self.peek_raw() {
    //             // sensible (?) ending spots
    //             None | Some(Token::Eof) | Some(Token::RBrace) => return,
    //
    //             // sensible (?) ending spots
    //             Some(Token::Semicolon) => {
    //                 self.next();
    //                 return;
    //             }
    //
    //             // don't consume sensible (?) starting spots
    //             Some(Token::Int) | Some(Token::Bool) | Some(Token::Return)
    //             | Some(Token::LParen) | Some(Token::LBrace) => return,
    //
    //             // consume "bad" tokens otherwise
    //             _ => {
    //                 self.next();
    //             }
    //         }
    //     }
    // }

    pub fn parse(&mut self) -> ParseResult<Program> {
        self.parse_program()
    }
}

/*
    ()
    ! - ++ --          (right)
    * / %
    + -
    << >>
    < <= > >=
    == !=
    &
    ^
    |
    &&
    ||
    ? :
    = += -= *= /= %=
      &= ^= |= <<= >>= (right)
*/

enum Infix {
    Binary(BinOp),
    Ternary,
}

impl Infix {
    fn bp(tok: &Token) -> Option<(Self, u8, u8)> {
        let (kind, lbp, rbp) = match tok {
            Token::Question => (Self::Ternary, 4, 3),
            Token::LogicOr => (Self::Binary(BinOp::LogicOr), 5, 6),
            Token::LogicAnd => (Self::Binary(BinOp::LogicAnd), 7, 8),
            Token::BitOr => (Self::Binary(BinOp::BitOr), 9, 10),
            Token::BitXor => (Self::Binary(BinOp::BitXor), 11, 12),
            Token::BitAnd => (Self::Binary(BinOp::BitAnd), 13, 14),
            Token::EqualEq => (Self::Binary(BinOp::EqualEq), 15, 16),
            Token::NotEq => (Self::Binary(BinOp::NotEq), 15, 16),
            Token::Less => (Self::Binary(BinOp::Less), 17, 18),
            Token::LessEq => (Self::Binary(BinOp::LessEq), 17, 18),
            Token::Greater => (Self::Binary(BinOp::Greater), 17, 18),
            Token::GreaterEq => (Self::Binary(BinOp::GreaterEq), 17, 18),
            Token::LShift => (Self::Binary(BinOp::LShift), 19, 20),
            Token::RShift => (Self::Binary(BinOp::RShift), 19, 20),
            Token::Plus => (Self::Binary(BinOp::Plus), 21, 22),
            Token::Minus => (Self::Binary(BinOp::Minus), 21, 22),
            Token::Times => (Self::Binary(BinOp::Times), 23, 24),
            Token::Div => (Self::Binary(BinOp::Div), 23, 24),
            Token::Mod => (Self::Binary(BinOp::Mod), 23, 24),
            _ => return None,
        };
        Some((kind, lbp, rbp))
    }
}
fn prefix_binding_power(tok: &Token) -> Option<(u8, u8)> {
    let res = match tok {
        Token::Minus | Token::Exclam => (26, 25),
        _ => return None,
    };

    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::lexer::Lexer;

    fn new_parser<'ctx>(ctx: &'ctx mut Context, tokens: &[LToken]) -> Parser<'ctx> {
        Parser::new(ctx, tokens)
    }

    fn lex_tokens<'ctx>(ctx: &'ctx mut Context, bytes: &[u8]) -> Vec<LToken> {
        Lexer::new(ctx, bytes).lex()
    }

    // mainly for testing pratt parsing and operator precedence
    fn interpret_expr(expr: &Expr) -> i32 {
        match expr {
            Expr::Ident(_) => todo!(),
            Expr::Int(i) => *i,
            Expr::True => 1,
            Expr::False => 0,
            Expr::BinOp { op, lhs, rhs } => {
                let lhs = interpret_expr(&lhs.0);
                let rhs = interpret_expr(&rhs.0);
                match *op {
                    BinOp::Plus => lhs + rhs,
                    BinOp::Minus => lhs - rhs,
                    BinOp::Times => lhs * rhs,
                    BinOp::Div => lhs / rhs,
                    BinOp::Mod => lhs % rhs,
                    BinOp::BitAnd => lhs & rhs,
                    BinOp::BitOr => lhs | rhs,
                    BinOp::BitXor => lhs ^ rhs,
                    BinOp::LogicAnd => lhs & rhs,
                    BinOp::LogicOr => lhs | rhs,
                    BinOp::LShift => lhs << rhs,
                    BinOp::RShift => lhs >> rhs,
                    BinOp::Greater => (lhs > rhs) as i32,
                    BinOp::GreaterEq => (lhs >= rhs) as i32,
                    BinOp::Less => (lhs < rhs) as i32,
                    BinOp::LessEq => (lhs <= rhs) as i32,
                    BinOp::EqualEq => (lhs == rhs) as i32,
                    BinOp::NotEq => (lhs != rhs) as i32,
                }
            }
            Expr::UnOp { op, oper } => {
                let oper = interpret_expr(&oper.0);
                match op {
                    UnOp::Negate => -1 * oper,
                    UnOp::Exclam => oper ^ 1,
                }
            }
            Expr::TernOp {
                cond,
                true_expr,
                false_expr,
            } => {
                let cond = interpret_expr(&cond.0);
                if cond == 1 {
                    return interpret_expr(&true_expr.0);
                } else {
                    return interpret_expr(&false_expr.0);
                }
            }
        }
    }

    #[test]
    fn test_parse_numeric_expr() {
        let test = |description, s: &str, expected| {
            let bytes = s.as_bytes();
            let mut ctx = Context::new(bytes);
            let tokens = lex_tokens(&mut ctx, bytes);
            let mut p = new_parser(&mut ctx, &tokens);
            let expr = p.parse_expr().unwrap();
            assert_eq!(interpret_expr(&expr.0), expected);

            insta::assert_yaml_snapshot!(description, format!("{}={} {}", s, expected, &expr.0),);
        };

        test("mul precedence", "1+1*2", 3);
        test("minus precedence", "-1+1*2", 1);
        test("minus and mul", "-9*-9", 81);
        test("div and minus", "9/-9", -1);
        test("mod and div precendence", "4%2+3/3", 1);

        test("parenthesis precendence left", "(1+2)*3", 9);
        test("parenthesis precendence right", "10/(1*2)", 5);
        test("parenthesis duplication expr", "(((1+2)))*3", 9);
        test("parenthesis duplication atoms", "((1)/(2))", 0);
    }
}
