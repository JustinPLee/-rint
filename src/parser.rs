// list of located tokens -> parse ast

use crate::ast_parse::{
    AsnOp, BinOp, Block, Control, Decl, Expr, GlobalDecl, LAsnOp, LBlock, LControl, LDecl, LExpr,
    LGlobalDecl, LIdent, LLValue, LParam, LProgram, LRetTyp, LSimp, LStmt, LStructField, LTyp,
    LValue, Param, PostOp, Program, RetTyp, Simp, Stmt, StructField, Typ, UnOp,
};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::location::{Located, Location, loc};
use crate::token::{LToken, Token};
use std::collections::HashSet;

pub struct Parser {
    tokens: Vec<LToken>,
    errors: Vec<ParseError>,
    last_location: Location, // uesd for location tracking
    // hack for context aware parsing, avoids backtracking by tracking which idents have been parsed
    // as a type
    // disambiguates situations such as x * y: is it a multiplication expression
    // or a declaration of variable y with type pointer to x
    // primitive types such as int and bool are reserved types, so this is ok
    // however, typedefs can create type aliases so types can be an arbitrary identifier
    type_aliases: HashSet<String>,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum ParseErrorKind {
    UnexpectedEof,
    UnexpectedToken,
    InvalidIdent,
    InvalidType,
    InvalidExpression,
    InvalidOperator,
}

#[derive(PartialEq, Clone, Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub description: String,
    pub location: Location,
}

type ParseResult<T> = Result<T, ParseError>;

enum Infix {
    Binary(BinOp),
    Ternary,
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

impl Parser {
    pub fn new(tokens: &[LToken]) -> Self {
        let tokens = tokens.iter().cloned().rev().collect();
        Self {
            tokens,
            errors: Vec::new(),
            last_location: Location::default(),
            type_aliases: HashSet::new(),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.last().map(|lt| &lt.data)
    }

    fn peek2(&self) -> Option<&Token> {
        self.tokens.iter().rev().nth(1).map(|lt| &lt.data)
    }

    fn peek3(&self) -> Option<&Token> {
        self.tokens.iter().rev().nth(2).map(|lt| &lt.data)
    }

    fn next(&mut self) -> Option<LToken> {
        let last_token = self.tokens.pop()?;
        self.last_location = last_token.location.clone();
        Some(last_token)
    }

    fn start_to_last(&self, start: Location) -> Location {
        start.merge(&self.last_location)
    }

    fn current_token(&self) -> &LToken {
        self.tokens.last().expect("EOF token always present")
    }

    fn current_location(&self) -> &Location {
        &self.current_token().location
    }

    /// Consume `token` or return an error
    fn try_consume(&mut self, token: &Token) -> ParseResult<LToken> {
        match self.peek() {
            Some(got) if got == token => Ok(self.next().expect("consumed")),
            Some(Token::Eof) => {
                let loc = self.current_location().clone();
                Err(self.process_error(ParseErrorKind::UnexpectedEof, None, loc))
            }
            Some(_got) => {
                let loc = self.current_location().clone();
                Err(self.process_error(ParseErrorKind::UnexpectedToken, None, loc))
            }
            None => unreachable!("EOF token always present"),
        }
    }

    pub fn errors(&self) -> Option<&[ParseError]> {
        if self.errors.is_empty() {
            None
        } else {
            Some(&self.errors)
        }
    }

    fn process_error(
        &mut self,
        kind: ParseErrorKind,
        description: Option<String>,
        location: Location,
    ) -> ParseError {
        let base_description = match &kind {
            ParseErrorKind::UnexpectedEof => "unexpected EOF".to_string(),
            ParseErrorKind::InvalidIdent => "invalid identifier".to_string(),
            ParseErrorKind::InvalidType => "invalid type".to_string(),
            ParseErrorKind::UnexpectedToken => "unexpected token".to_string(),
            ParseErrorKind::InvalidExpression => "invalid expression".to_string(),
            ParseErrorKind::InvalidOperator => "invalid operator".to_string(),
        };
        let concatted_description = if let Some(description) = description {
            format!(
                "{} -> {:?} -- expected {}",
                base_description,
                self.current_token().data,
                description
            )
        } else {
            base_description
        };
        let err = ParseError {
            kind,
            description: concatted_description,
            location,
        };
        self.errors.push(err.clone());
        self.synchronize();
        err
    }

    fn parse_program(&mut self) -> ParseResult<LProgram> {
        let start = self.current_location().clone();
        let mut gdecls = Vec::new();
        while matches!(
            self.peek(),
            Some(
                Token::Typedef
                    | Token::Int
                    | Token::Bool
                    | Token::Void
                    | Token::Struct
                    | Token::Ident(_)
            )
        ) {
            let decl = match self.peek() {
                Some(Token::Typedef) => self.parse_typedef()?,
                Some(Token::Struct)
                    if matches!(self.peek3(), Some(Token::LBrace | Token::Semicolon)) =>
                {
                    self.parse_structdef()?
                }
                _ => self.parse_fun_def()?,
            };
            gdecls.push(decl);
        }
        let location = if gdecls.is_empty() {
            start
        } else {
            self.start_to_last(start)
        };
        Ok(loc(Program(gdecls), location))
    }

    fn parse_structdef(&mut self) -> ParseResult<LGlobalDecl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::Struct)?;
        let name = self.parse_ident()?;
        let body = match self.peek() {
            Some(Token::Semicolon) => {
                self.next();
                GlobalDecl::StructDef { name, body: None }
            }
            Some(Token::LBrace) => {
                let body = self.parse_structfields()?;
                GlobalDecl::StructDef {
                    name,
                    body: Some(body),
                }
            }
            _ => {
                let location = self.current_location().clone();
                return Err(self.process_error(ParseErrorKind::UnexpectedToken, None, location));
            }
        };
        Ok(loc(body, self.start_to_last(start)))
    }

    fn parse_structfields(&mut self) -> ParseResult<Vec<LStructField>> {
        self.try_consume(&Token::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), Some(Token::RBrace | Token::Eof) | None) {
            fields.push(self.parse_structfield()?);
        }
        self.try_consume(&Token::RBrace)?;
        self.try_consume(&Token::Semicolon)?;

        Ok(fields)
    }

    fn parse_structfield(&mut self) -> ParseResult<LStructField> {
        let start = self.current_location().clone();
        let typ = self.parse_typ()?;
        let name = self.parse_ident()?;
        self.try_consume(&Token::Semicolon)?;
        Ok(loc(StructField { typ, name }, self.start_to_last(start)))
    }

    fn parse_typedef(&mut self) -> ParseResult<LGlobalDecl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::Typedef)?;
        let typ = self.parse_typ()?;
        let alias = self.parse_ident()?;
        self.try_consume(&Token::Semicolon)?;
        self.type_aliases.insert(alias.data.clone());
        Ok(loc(
            GlobalDecl::Typedef { typ, alias },
            self.start_to_last(start),
        ))
    }

    fn parse_fun_def(&mut self) -> ParseResult<LGlobalDecl> {
        let start = self.current_location().clone();

        let ret_typ = self.parse_ret_typ()?;
        let name = self.parse_ident()?;
        let params = self.parse_params()?;
        let body = match self.peek() {
            Some(Token::Semicolon) => {
                self.next();
                GlobalDecl::FunDef {
                    ret_typ,
                    name,
                    params,
                    body: None,
                }
            }
            Some(Token::LBrace) => {
                let body = self.parse_block()?;
                GlobalDecl::FunDef {
                    ret_typ,
                    name,
                    params,
                    body: Some(body),
                }
            }
            _ => {
                let location = self.current_location().clone();
                return Err(self.process_error(
                    ParseErrorKind::UnexpectedToken,
                    Some("function declaration or definition".to_string()),
                    location,
                ));
            }
        };
        Ok(loc(body, self.start_to_last(start)))
    }

    fn parse_ret_typ(&mut self) -> ParseResult<LRetTyp> {
        match self.peek() {
            Some(Token::Void) => {
                let token = self.try_consume(&Token::Void)?;
                Ok(loc(RetTyp::Void, token.location))
            }
            _ => {
                let typ = self.parse_typ()?;
                let location = typ.location.clone();
                Ok(loc(RetTyp::Typ(typ), location))
            }
        }
    }

    fn parse_params(&mut self) -> ParseResult<Vec<LParam>> {
        if matches!(
            (self.peek(), self.peek2()),
            (Some(Token::LParen), Some(Token::RParen))
        ) {
            self.next(); // (
            self.next(); // )
            return Ok(Vec::new());
        }

        self.try_consume(&Token::LParen)?;

        let mut params = vec![self.parse_param()?];
        while matches!(self.peek(), Some(Token::Comma)) {
            self.try_consume(&Token::Comma)?;
            params.push(self.parse_param()?);
        }

        self.try_consume(&Token::RParen)?;
        Ok(params)
    }

    fn parse_assert(&mut self) -> ParseResult<LControl> {
        let start = self.current_location().clone();

        self.try_consume(&Token::Assert)?;

        self.try_consume(&Token::LParen)?;
        let expr = self.parse_expr()?;
        self.try_consume(&Token::RParen)?;

        self.try_consume(&Token::Semicolon)?;
        Ok(loc(Control::Assert(expr), self.start_to_last(start)))
    }

    fn parse_param(&mut self) -> ParseResult<LParam> {
        let start = self.current_location().clone();
        let typ = self.parse_typ()?;
        let name = self.parse_ident()?;
        Ok(loc(Param { typ, name }, self.start_to_last(start)))
    }

    fn parse_block(&mut self) -> ParseResult<LBlock> {
        let start = self.current_location().clone();
        self.try_consume(&Token::LBrace)?;
        let stmts = self.parse_stmts()?;
        self.try_consume(&Token::RBrace)?;
        Ok(loc(Block(stmts), self.start_to_last(start)))
    }

    fn parse_stmts(&mut self) -> ParseResult<Vec<LStmt>> {
        let mut stmts = Vec::new();

        while !matches!(self.peek(), Some(Token::RBrace) | Some(Token::Eof) | None) {
            if let Ok(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            }
        }

        Ok(stmts)
    }

    fn parse_typ(&mut self) -> ParseResult<LTyp> {
        let mut typ = match self.peek() {
            Some(Token::Int) => {
                let token = self.try_consume(&Token::Int)?;
                loc(Typ::Int, token.location)
            }
            Some(Token::Bool) => {
                let token = self.try_consume(&Token::Bool)?;
                loc(Typ::Bool, token.location)
            }
            Some(Token::Struct) => {
                let start = self.try_consume(&Token::Struct)?.location;
                let name = self.parse_ident()?;
                let location = start.merge(&name.location);
                loc(Typ::Struct(name), location)
            }
            Some(Token::Ident(_)) => {
                let token = self.parse_ident()?;
                let location = token.location.clone();
                loc(Typ::Alias(token), location)
            }
            None => {
                return Err(self.process_error(
                    ParseErrorKind::UnexpectedEof,
                    None,
                    self.current_location().clone(),
                ));
            }
            _ => {
                return Err(self.process_error(
                    ParseErrorKind::InvalidType,
                    None,
                    self.current_location().clone(),
                ));
            }
        };

        loop {
            match self.peek() {
                Some(Token::Star) => {
                    let star = self.next().unwrap();
                    let location = typ.location.clone().merge(&star.location);
                    typ = loc(Typ::Pointer(Box::new(typ)), location);
                }
                Some(Token::LBracket) => {
                    self.next();
                    let end = self.try_consume(&Token::RBracket)?.location;
                    let location = typ.location.clone().merge(&end);
                    typ = loc(Typ::Array(Box::new(typ)), location);
                }
                _ => break,
            }
        }

        Ok(typ)
    }

    fn parse_stmt(&mut self) -> ParseResult<LStmt> {
        let start = self.current_location().clone();

        match self.peek() {
            Some(Token::If) => {
                let ctrl = self.parse_if()?;
                Ok(loc(Stmt::Control(ctrl), self.start_to_last(start)))
            }
            Some(Token::While) => {
                let ctrl = self.parse_while()?;
                Ok(loc(Stmt::Control(ctrl), self.start_to_last(start)))
            }
            Some(Token::For) => {
                let ctrl = self.parse_for()?;
                Ok(loc(Stmt::Control(ctrl), self.start_to_last(start)))
            }
            Some(Token::Return) => {
                let ctrl = self.parse_return()?;
                Ok(loc(Stmt::Control(ctrl), self.start_to_last(start)))
            }
            Some(Token::Break) => {
                let ctrl = self.parse_break()?;
                Ok(loc(Stmt::Control(ctrl), self.start_to_last(start)))
            }
            Some(Token::Continue) => {
                let ctrl = self.parse_continue()?;
                Ok(loc(Stmt::Control(ctrl), self.start_to_last(start)))
            }
            Some(Token::Assert) => {
                let ctrl = self.parse_assert()?;
                Ok(loc(Stmt::Control(ctrl), self.start_to_last(start)))
            }
            Some(Token::LBrace) => {
                let stmts = self.parse_block()?;
                Ok(loc(Stmt::Block(stmts), self.start_to_last(start)))
            }
            _ => {
                let simp = self.parse_simp()?;
                self.try_consume(&Token::Semicolon)?;
                Ok(loc(Stmt::Simp(simp), self.start_to_last(start)))
            }
        }
    }

    fn parse_if(&mut self) -> ParseResult<LControl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::If)?;

        self.try_consume(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.try_consume(&Token::RParen)?;

        let true_block = self.parse_block()?;

        if self.peek() == Some(&Token::Else) {
            self.next();
            let false_block = self.parse_block()?;
            Ok(loc(
                Control::If {
                    cond,
                    true_block,
                    false_block: Some(false_block),
                },
                self.start_to_last(start),
            ))
        } else {
            Ok(loc(
                Control::If {
                    cond,
                    true_block,
                    false_block: None,
                },
                self.start_to_last(start),
            ))
        }
    }

    fn parse_while(&mut self) -> ParseResult<LControl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::While)?;

        self.try_consume(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.try_consume(&Token::RParen)?;

        let body = self.parse_block()?; // different than spec
        Ok(loc(
            Control::While { cond, body },
            self.start_to_last(start),
        ))
    }

    fn parse_for(&mut self) -> ParseResult<LControl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::For)?;

        self.try_consume(&Token::LParen)?;
        let init = self.parse_for_init()?;
        self.try_consume(&Token::Semicolon)?;

        let cond = self.parse_expr()?;
        self.try_consume(&Token::Semicolon)?;

        let step = self.parse_for_step()?;
        self.try_consume(&Token::RParen)?;

        let body = self.parse_block()?; // different than spec
        Ok(loc(
            Control::For {
                init,
                cond,
                step,
                body,
            },
            self.start_to_last(start),
        ))
    }

    // optional
    fn parse_for_init(&mut self) -> ParseResult<Option<LSimp>> {
        if matches!(self.peek(), Some(Token::Semicolon)) {
            return Ok(None);
        }

        Ok(Some(self.parse_simp()?))
    }

    // optional
    fn parse_for_step(&mut self) -> ParseResult<Option<LSimp>> {
        if matches!(self.peek(), Some(Token::RParen)) {
            return Ok(None);
        }

        Ok(Some(self.parse_simp()?))
    }

    fn parse_return(&mut self) -> ParseResult<LControl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::Return)?;
        if matches!(self.peek(), Some(Token::Semicolon)) {
            self.try_consume(&Token::Semicolon)?;
            Ok(loc(Control::Return(None), self.start_to_last(start)))
        } else {
            let expr = self.parse_expr()?;
            self.try_consume(&Token::Semicolon)?;
            Ok(loc(Control::Return(Some(expr)), self.start_to_last(start)))
        }
    }

    fn parse_break(&mut self) -> ParseResult<LControl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::Break)?;
        self.try_consume(&Token::Semicolon)?;
        Ok(loc(Control::Break, self.start_to_last(start)))
    }

    fn parse_continue(&mut self) -> ParseResult<LControl> {
        let start = self.current_location().clone();
        self.try_consume(&Token::Continue)?;
        self.try_consume(&Token::Semicolon)?;
        Ok(loc(Control::Continue, self.start_to_last(start)))
    }

    fn parse_simp(&mut self) -> ParseResult<LSimp> {
        let start = self.current_location().clone();
        match self.peek() {
            Some(Token::Int | Token::Bool | Token::Struct) => {
                let decl = self.parse_decl()?;
                Ok(loc(Simp::Decl(decl), self.start_to_last(start)))
            }
            Some(Token::Ident(name)) => match self.peek2() {
                // check if the first identifier was actually a type from a typedef
                // if so, then <ident> <ident> is the start of a variable declaration
                Some(Token::Ident(_)) if self.type_aliases.contains(name) => {
                    let decl = self.parse_decl()?;
                    Ok(loc(Simp::Decl(decl), self.start_to_last(start)))
                }
                Some(Token::Star) if self.type_aliases.contains(name) => {
                    let decl = self.parse_decl()?;
                    Ok(loc(Simp::Decl(decl), self.start_to_last(start)))
                }
                _ => self.parse_expr_or_assign(),
            },
            Some(Token::LParen) => self.parse_expr_or_assign(),
            _ => Ok(loc(
                Simp::Expr(self.parse_expr()?),
                self.start_to_last(start),
            )),
        }
    }

    fn parse_decl(&mut self) -> ParseResult<LDecl> {
        let start = self.current_location().clone();
        let typ = self.parse_typ()?;
        let name = self.parse_ident()?;
        if let Some(&Token::Eq) = self.peek() {
            self.next(); // skip '='
            let value = self.parse_expr()?;
            Ok(loc(
                Decl::Init { typ, name, value },
                self.start_to_last(start),
            ))
        } else {
            Ok(loc(Decl::Decl { typ, name }, self.start_to_last(start)))
        }
    }

    fn parse_postfix(&mut self, lvalue: LLValue) -> ParseResult<LSimp> {
        let postop = match self.next() {
            Some(Located {
                data: Token::DoubleMinus,
                location,
            }) => loc(PostOp::DoubleMinus, location),
            Some(Located {
                data: Token::DoublePlus,
                location,
            }) => loc(PostOp::DoublePlus, location),
            _ => {
                return Err(self.process_error(
                    ParseErrorKind::UnexpectedEof,
                    None,
                    self.current_location().clone(),
                ));
            }
        };
        let location = lvalue.location.clone().merge(&postop.location);
        Ok(loc(
            Simp::Post {
                name: lvalue,
                postop,
            },
            location,
        ))
    }

    fn parse_ident(&mut self) -> ParseResult<LIdent> {
        match self.next() {
            Some(Located {
                data: Token::Ident(s),
                location,
            }) => Ok(loc(s, location)),
            Some(Located { location, .. }) => {
                Err(self.process_error(ParseErrorKind::InvalidIdent, None, location))
            }
            None => Err(self.process_error(
                ParseErrorKind::UnexpectedEof,
                None,
                self.current_location().clone(),
            )),
        }
    }

    fn parse_call(&mut self, name: LIdent) -> ParseResult<LExpr> {
        self.try_consume(&Token::LParen)?;

        let mut args = Vec::new();
        if self.peek() != Some(&Token::RParen) {
            loop {
                args.push(self.parse_expr()?);

                if self.peek() != Some(&Token::Comma) {
                    break;
                }

                self.next(); // comma
            }
        }

        let end = self.try_consume(&Token::RParen)?.location;

        let location = name.location.clone().merge(&end);
        Ok(loc(Expr::FunCall { name, args }, location))
    }

    fn parse_expr_or_assign(&mut self) -> ParseResult<LSimp> {
        let expr = self.parse_expr()?;

        match self.peek() {
            Some(Token::DoubleMinus | Token::DoublePlus) => {
                let lvalue = self.expr_to_lvalue(expr)?;
                self.parse_postfix(lvalue)
            }
            Some(
                Token::Eq
                | Token::PlusEq
                | Token::MinusEq
                | Token::StarEq
                | Token::DivEq
                | Token::ModEq
                | Token::AndEq
                | Token::XorEq
                | Token::OrEq
                | Token::LShiftEq
                | Token::RShiftEq,
            ) => {
                let name = self.expr_to_lvalue(expr)?;
                let asnop = self.parse_asnop()?;
                let value = self.parse_expr()?;
                let location = name.location.clone().merge(&value.location);
                Ok(loc(Simp::Assign { name, asnop, value }, location))
            }
            _ => {
                let location = expr.location.clone();
                Ok(loc(Simp::Expr(expr), location))
            }
        }
    }

    fn expr_to_lvalue(&mut self, expr: LExpr) -> ParseResult<LLValue> {
        let location = expr.location.clone();
        let data = match expr.data {
            Expr::Ident(name) => LValue::Ident(name),
            Expr::Field { ident, field } => LValue::Field {
                base: Box::new(self.expr_to_lvalue(*ident)?),
                field,
            },
            Expr::Arrow { base, field } => LValue::Arrow { base, field },
            Expr::Deref(value) => LValue::Deref(value),
            Expr::Index { base, index } => LValue::Index {
                base: Box::new(self.expr_to_lvalue(*base)?),
                index,
            },
            _ => {
                return Err(self.process_error(ParseErrorKind::InvalidExpression, None, location));
            }
        };
        Ok(loc(data, location))
    }

    fn parse_asnop(&mut self) -> ParseResult<LAsnOp> {
        let token = self.next().unwrap();
        let op = match token.data {
            Token::Eq => AsnOp::Eq,
            Token::PlusEq => AsnOp::PlusEq,
            Token::MinusEq => AsnOp::MinusEq,
            Token::StarEq => AsnOp::TimesEq,
            Token::DivEq => AsnOp::DivEq,
            Token::ModEq => AsnOp::ModEq,
            Token::AndEq => AsnOp::AndEq,
            Token::XorEq => AsnOp::XorEq,
            Token::OrEq => AsnOp::OrEq,
            Token::LShiftEq => AsnOp::LShiftEq,
            Token::RShiftEq => AsnOp::RShiftEq,
            _ => {
                return Err(self.process_error(
                    ParseErrorKind::UnexpectedToken,
                    None,
                    token.location,
                ));
            }
        };

        Ok(loc(op, token.location))
    }

    // pratt parsing
    fn parse_expr(&mut self) -> ParseResult<LExpr> {
        self.parse_expr_impl(0)
    }

    fn parse_expr_impl(&mut self, min_bp: u8) -> ParseResult<LExpr> {
        // parse prefix
        let mut lhs = self.parse_prefix()?;

        while let Some(tok) = self.peek() {
            // parse postfix
            if let Some(bp) = postfix_bp(tok) {
                if bp < min_bp {
                    break;
                }

                let op = self.next().unwrap().data;
                lhs = match op {
                    Token::LBracket => {
                        let index = self.parse_expr()?;
                        let end = self.try_consume(&Token::RBracket)?.location;
                        let location = lhs.location.clone().merge(&end);
                        loc(
                            Expr::Index {
                                base: Box::new(lhs),
                                index: Box::new(index),
                            },
                            location,
                        )
                    }
                    Token::Dot => {
                        let field = self.parse_ident()?;
                        let location = lhs.location.clone().merge(&field.location);
                        loc(
                            Expr::Field {
                                ident: Box::new(lhs),
                                field,
                            },
                            location,
                        )
                    }
                    Token::Arrow => {
                        let field = self.parse_ident()?;
                        let location = lhs.location.clone().merge(&field.location);
                        loc(
                            Expr::Arrow {
                                base: Box::new(lhs),
                                field,
                            },
                            location,
                        )
                    }
                    _ => panic!(),
                };
                continue;
            }

            // parse infix
            if let Some((kind, left_bp, right_bp)) = Infix::bp(tok) {
                if left_bp < min_bp {
                    break;
                }

                let op_location = self.next().unwrap().location;
                lhs = match kind {
                    Infix::Binary(op) => {
                        let rhs = self.parse_expr_impl(right_bp)?;
                        let location = lhs.location.clone().merge(&rhs.location);

                        loc(
                            Expr::BinOp {
                                op: loc(op, op_location),
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

                        let location = lhs.location.clone().merge(&false_expr.location);
                        loc(
                            Expr::TernOp {
                                cond: Box::new(lhs),
                                true_expr: Box::new(true_expr),
                                false_expr: Box::new(false_expr),
                            },
                            location,
                        )
                    }
                };
            } else {
                break;
            }
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> ParseResult<LExpr> {
        if matches!(self.peek(), Some(Token::Eof)) {
            let location = self.current_location().clone();
            return Err(self.process_error(
                ParseErrorKind::InvalidExpression,
                Some("expression".into()),
                location,
            ));
        }

        let Located {
            data: tok,
            location: start,
        } = self.next().ok_or_else(|| {
            self.process_error(
                ParseErrorKind::InvalidExpression,
                None,
                self.current_location().clone(),
            )
        })?;

        match tok {
            Token::True => Ok(loc(Expr::True, start)),
            Token::False => Ok(loc(Expr::False, start)),
            Token::Null => Ok(loc(Expr::Null, start)),
            Token::Num(n) => Ok(loc(Expr::Int(n), start)),
            Token::Alloc => {
                self.try_consume(&Token::LParen)?;
                let typ = self.parse_typ()?;
                self.try_consume(&Token::RParen)?;
                Ok(loc(Expr::Alloc(typ), self.start_to_last(start)))
            }
            Token::AllocArray => {
                self.try_consume(&Token::LParen)?;
                let typ = self.parse_typ()?;
                self.try_consume(&Token::Comma)?;
                let size = self.parse_expr()?;
                self.try_consume(&Token::RParen)?;
                Ok(loc(
                    Expr::AllocArray {
                        typ,
                        size: Box::new(size),
                    },
                    self.start_to_last(start),
                ))
            }
            Token::Ident(name) if self.peek() == Some(&Token::LParen) => {
                self.parse_call(loc(name, start))
            }
            Token::Ident(name) => Ok(loc(Expr::Ident(loc(name, start.clone())), start)),
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.try_consume(&Token::RParen)?;

                Ok(loc(expr.data, self.start_to_last(start)))
            }
            Token::Minus => {
                let right_bp = prefix_bp(&Token::Minus).unwrap();
                let operand = self.parse_expr_impl(right_bp)?;
                let location = self.start_to_last(start.clone());

                Ok(loc(
                    Expr::UnOp {
                        op: loc(UnOp::Negate, start),
                        oper: Box::new(operand),
                    },
                    location,
                ))
            }
            Token::Exclam => {
                let right_bp = prefix_bp(&Token::Exclam).unwrap();
                let operand = self.parse_expr_impl(right_bp)?;
                let location = self.start_to_last(start.clone());

                Ok(loc(
                    Expr::UnOp {
                        op: loc(UnOp::Exclam, start),
                        oper: Box::new(operand),
                    },
                    location,
                ))
            }
            Token::Star => {
                let right_bp = prefix_bp(&Token::Star).unwrap();
                let operand = self.parse_expr_impl(right_bp)?;
                let location = self.start_to_last(start);

                Ok(loc(Expr::Deref(Box::new(operand)), location))
            }

            _tok => Err(self.process_error(ParseErrorKind::InvalidExpression, None, start)),
        }
    }

    fn synchronize(&mut self) {
        loop {
            match self.peek() {
                None | Some(Token::Eof) => return,
                Some(Token::Semicolon) | Some(Token::RBrace) => {
                    self.next();
                    return;
                }
                _ => {
                    self.next();
                }
            }
        }
    }

    pub fn parse(&mut self) -> LProgram {
        self.parse_program()
            .unwrap_or_else(|_| loc(Program(vec![]), self.current_location().clone()))
    }
}

fn prefix_bp(tok: &Token) -> Option<u8> {
    let res = match tok {
        Token::Minus | Token::Exclam | Token::Star => 25,
        _ => return None,
    };

    Some(res)
}

fn postfix_bp(tok: &Token) -> Option<u8> {
    match tok {
        Token::Dot | Token::Arrow | Token::LBracket => Some(26),
        _ => None,
    }
}

impl Infix {
    /*
        () [] -> .
        ! - ++ -- ~ *     (right)
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
    // higher binding powers have higher precedence
    // left_bp < right_bp means left associativity
    // right_bp < left_bp means right associativity
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

            Token::Star => (Self::Binary(BinOp::Times), 23, 24),
            Token::Div => (Self::Binary(BinOp::Div), 23, 24),
            Token::Mod => (Self::Binary(BinOp::Mod), 23, 24),

            _ => return None,
        };

        Some((kind, lbp, rbp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn new_parser(tokens: &[LToken]) -> Parser {
        Parser::new(tokens)
    }

    fn lex_tokens(bytes: &[u8]) -> Vec<LToken> {
        Lexer::new(bytes).lex()
    }

    fn assert_parse_error(source: &str, expected: ParseErrorKind) {
        let tokens = lex_tokens(source.as_bytes());
        let mut parser = new_parser(&tokens);
        parser.parse();

        let Some(errors) = parser.errors() else {
            panic!();
        };
        assert_eq!(errors.first().cloned().unwrap().kind, expected);
    }

    fn parse_numeric_expr(source: &str) -> i32 {
        let tokens = lex_tokens(source.as_bytes());
        let mut parser = new_parser(&tokens);
        let expr = parser.parse_expr().expect("number");

        assert!(matches!(parser.errors(), None));
        interpret_expr(&expr.data)
    }

    // mainly for testing pratt parsing and operator precedence
    fn interpret_expr(expr: &Expr) -> i32 {
        match expr {
            Expr::Ident(_) => todo!(),
            Expr::Int(i) => *i,
            Expr::True => 1,
            Expr::False => 0,
            Expr::BinOp { op, lhs, rhs } => {
                let lhs = interpret_expr(&lhs.data);
                let rhs = interpret_expr(&rhs.data);

                match op.data {
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
                let oper = interpret_expr(&oper.data);

                match op.data {
                    UnOp::Negate => -1 * oper,
                    UnOp::Exclam => oper ^ 1,
                    UnOp::BitNot => !oper,
                }
            }
            Expr::TernOp {
                cond,
                true_expr,
                false_expr,
            } => {
                let cond = interpret_expr(&cond.data);

                if cond == 1 {
                    interpret_expr(&true_expr.data)
                } else {
                    interpret_expr(&false_expr.data)
                }
            }
            _ => panic!("non-numeric expression in numeric parser test"),
        }
    }

    #[test]
    fn test_operators() {
        let cases = [
            ("subtraction", "10 - 3 - 2", 5),
            ("division", "24 / 4 / 3", 2),
            ("multiplication binds before addition", "1 + 2 * 3", 7),
            ("bitwise precedence", "1 | 2 ^ 3 & 1", 3),
            ("comparison binds before equality", "1 < 2 == true", 1),
            ("bit: and binds before or", "1 | 1 & 0", 1),
            ("unary binds before multiplication", "-2 * 3", -6),
            ("ternary", "true ? 2 + 3 : 4 * 5", 5),
            (
                "ternary is right associative",
                "false ? 1 : true ? 2 : 3",
                2,
            ),
            ("mul precedence", "1+1*2", 3),
            ("minus precedence", "-1+1*2", 1),
            ("minus and mul", "-9*-9", 81),
            ("div and minus", "9/-9", -1),
            ("mod and div precendence", "4%2+3/3", 1),
            ("parenthesis precendence left", "(1+2)*3", 9),
            ("parenthesis precendence right", "10/(1*2)", 5),
            ("parenthesis duplication expr", "(((1+2)))*3", 9),
            ("parenthesis duplication atoms", "((1)/(2))", 0),
        ];

        for (description, source, expected) in cases {
            assert_eq!(
                parse_numeric_expr(source),
                expected,
                "{description}: {source}"
            );
        }
    }

    #[test]
    fn unexpected_eof() {
        let source = r#"
            int main() {
                return 0;
            "#;

        assert_parse_error(source, ParseErrorKind::UnexpectedEof);
    }

    #[test]
    fn unexpected_token() {
        let source = r#"
            int main() {
                return 0
            }
            "#;

        assert_parse_error(source, ParseErrorKind::UnexpectedToken);
    }

    #[test]
    fn bad_fun_decl() {
        let source = r#"
            int main() 0;
            "#;

        assert_parse_error(source, ParseErrorKind::UnexpectedToken);
    }

    #[test]
    fn invalid_identifier() {
        let source = r#"
            int main(int) {
                return 0;
            }
            "#;

        assert_parse_error(source, ParseErrorKind::InvalidIdent);
    }

    #[test]
    fn invalid_type() {
        let source = r#"
            typedef true Alias;
            "#;

        assert_parse_error(source, ParseErrorKind::InvalidType);
    }

    #[test]
    fn invalid_expression() {
        let source = r#"
            int main() {
                return + 1;
            }
            "#;

        assert_parse_error(source, ParseErrorKind::InvalidExpression);
    }

    #[test]
    fn invalid_statement_error_kind() {
        let source = r#"
            int main() {
                int y = 0;
                y = y++;
            }
            "#;

        assert_parse_error(source, ParseErrorKind::UnexpectedToken);
    }

    #[test]
    fn test_program_0() {
        let source = r#"
            bool xyz(Number y1);
            typedef int Number;
            bool xyz(Number y2) {
                return y2 > 2;
            }
            int main(Number x, bool flag) {
                Number result = flag && x > 0 ? x : 0;
                if (result != 0 && xyz(3)) {
                    return result;
                } else {
                    return 1;
                }
            }"#;
        let tokens = lex_tokens(source.as_bytes());
        let mut parser = new_parser(&tokens);
        let program = parser.parse();
        assert!(matches!(parser.errors(), None));
        insta::assert_snapshot!(format!("{}", program));
    }
}
