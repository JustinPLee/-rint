// a (almost direct) translation from grammar to code
// parse -> ast_parse -> elaboration -> ast

use crate::location::Located;
use crate::utils::{Pretty, write_indent};
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq)]
pub enum PostOp {
    DoublePlus,
    DoubleMinus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UnOp {
    Exclam,
    Negate,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AsnOp {
    Eq,
    PlusEq,
    MinusEq,
    TimesEq,
    DivEq,
    ModEq,
    AndEq,
    XorEq,
    OrEq,
    LShiftEq, // not implemented
    RShiftEq, // not implemented
}

#[derive(Clone, Debug, PartialEq)]
pub enum BinOp {
    Plus,
    Minus,
    Times,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    LShift,
    RShift,
    Greater,
    GreaterEq,
    Less,
    LessEq,
    EqualEq,
    NotEq,
    LogicAnd,
    LogicOr,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Ident(String),
    Int(i32),
    True,
    False,
    UnOp {
        op: LUnOp,
        oper: Box<LExpr>,
    },
    BinOp {
        op: LBinOp,
        lhs: Box<LExpr>,
        rhs: Box<LExpr>,
    },
    TernOp {
        cond: Box<LExpr>,
        true_expr: Box<LExpr>,
        false_expr: Box<LExpr>,
    },
    FunCall {
        name: LIdent,
        args: Vec<LExpr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    If {
        cond: LExpr,
        true_block: LBlock,
        false_block: Option<LBlock>,
    },
    While {
        cond: LExpr,
        body: LBlock,
    },
    For {
        init: Option<LSimp>,
        cond: LExpr,
        step: Option<LSimp>,
        body: LBlock,
    },
    Return(Option<LExpr>),
    Assert(LExpr),
    Break,
    Continue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typ {
    Int,
    Bool,
    Named(String), // user defined types or aliases
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetTyp {
    Typ(LTyp),
    Void,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decl {
    Decl {
        typ: LTyp,
        name: LIdent,
    },
    Init {
        typ: LTyp,
        name: LIdent,
        value: LExpr,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Block(LBlock),
    Control(LControl),
    Simp(LSimp),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Simp {
    Assign {
        name: LValue,
        asnop: LAsnOp,
        value: LExpr,
    },
    Post {
        name: LValue,
        postop: LPostOp,
    },
    Expr(LExpr),
    Decl(LDecl),
}

#[derive(Clone, Debug, PartialEq)]
pub enum GlobalDecl {
    Typedef {
        typ: LTyp,
        alias: LIdent,
    },
    FunDef {
        ret_typ: LRetTyp,
        name: LIdent,
        params: Vec<LParam>,
        body: Option<LBlock>, // if none, then FunDecl
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub typ: LTyp,
    pub name: LIdent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block(pub Vec<LStmt>);

#[derive(Clone, Debug, PartialEq)]
pub struct Program(pub Vec<LGlobalDecl>);

pub type LIdent = Located<String>;
pub type LPostOp = Located<PostOp>;
pub type LUnOp = Located<UnOp>;
pub type LAsnOp = Located<AsnOp>;
pub type LBinOp = Located<BinOp>;
pub type LExpr = Located<Expr>;
pub type LTyp = Located<Typ>;
pub type LDecl = Located<Decl>;
pub type LSimp = Located<Simp>;
pub type LControl = Located<Control>;
pub type LStmt = Located<Stmt>;
pub type LBlock = Located<Block>;
pub type LParam = Located<Param>;
pub type LRetTyp = Located<RetTyp>;
pub type LGlobalDecl = Located<GlobalDecl>;
pub type LProgram = Located<Program>;

pub type LValue = LIdent; // will change in the future

impl Pretty for String {
    fn pretty(&self, _indent: usize) -> String {
        self.clone()
    }
}

impl<T: Pretty> Pretty for Located<T> {
    fn pretty(&self, indent: usize) -> String {
        self.data.pretty(indent)
    }
}

impl Pretty for PostOp {
    fn pretty(&self, _indent: usize) -> String {
        let op = match self {
            PostOp::DoublePlus => "++",
            PostOp::DoubleMinus => "--",
        };
        op.to_string()
    }
}

impl Pretty for UnOp {
    fn pretty(&self, _indent: usize) -> String {
        let op = match self {
            UnOp::Exclam => "!",
            UnOp::Negate => "-",
        };
        op.to_string()
    }
}

impl Pretty for AsnOp {
    fn pretty(&self, _indent: usize) -> String {
        let op = match self {
            AsnOp::Eq => "=",
            AsnOp::PlusEq => "+=",
            AsnOp::MinusEq => "-=",
            AsnOp::TimesEq => "*=",
            AsnOp::DivEq => "/=",
            AsnOp::ModEq => "%=",
            AsnOp::AndEq => "&=",
            AsnOp::XorEq => "^=",
            AsnOp::OrEq => "|=",
            AsnOp::LShiftEq => "<<=",
            AsnOp::RShiftEq => ">>=",
        };
        op.to_string()
    }
}

impl Pretty for BinOp {
    fn pretty(&self, _indent: usize) -> String {
        let op = match self {
            BinOp::Plus => "+",
            BinOp::Minus => "-",
            BinOp::Times => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::LShift => "<<",
            BinOp::RShift => ">>",
            BinOp::Greater => ">",
            BinOp::GreaterEq => ">=",
            BinOp::Less => "<",
            BinOp::LessEq => "<=",
            BinOp::EqualEq => "==",
            BinOp::NotEq => "!=",
            BinOp::LogicAnd => "&&",
            BinOp::LogicOr => "||",
        };
        op.to_string()
    }
}

impl Pretty for Typ {
    fn pretty(&self, _indent: usize) -> String {
        match self {
            Typ::Int => "int".to_string(),
            Typ::Bool => "bool".to_string(),
            Typ::Named(t) => t.to_string(),
        }
    }
}

impl Pretty for Expr {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Expr::Ident(name) => name.to_string(),
            Expr::Int(value) => value.to_string(),
            Expr::True => "true".to_string(),
            Expr::False => "false".to_string(),

            Expr::UnOp { op, oper } => {
                let mut s = String::new();
                write!(s, "({} {})", op, oper.data.pretty(indent)).unwrap();
                s
            }
            Expr::BinOp { op, lhs, rhs } => {
                let mut s = String::new();
                write!(
                    s,
                    "({} {} {})",
                    op,
                    lhs.data.pretty(indent),
                    rhs.data.pretty(indent)
                )
                .unwrap();
                s
            }
            Expr::TernOp {
                cond,
                true_expr,
                false_expr,
            } => {
                let mut s = String::new();
                write!(
                    s,
                    "({} ? {} : {})",
                    cond.data.pretty(indent),
                    true_expr.data.pretty(indent),
                    false_expr.data.pretty(indent)
                )
                .unwrap();
                s
            }

            Expr::FunCall { name, args } => {
                let mut s = String::new();
                write!(s, "(call {}", name).unwrap();
                for arg in args {
                    write!(s, " {}", arg.data.pretty(indent)).unwrap();
                }
                s.push_str(")");
                s
            }
        }
    }
}

impl Pretty for Decl {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Decl::Decl { typ, name } => {
                format!("(declare {name} {})", typ.pretty(indent))
            }
            Decl::Init { typ, name, value } => {
                let mut s = String::new();
                write!(
                    s,
                    "(declare {} {} {})",
                    name,
                    typ.pretty(indent),
                    value.data.pretty(indent)
                )
                .unwrap();
                s
            }
        }
    }
}

impl Pretty for Simp {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Simp::Assign { name, asnop, value } => {
                let mut s = String::new();
                if matches!(&asnop.data, AsnOp::Eq) {
                    write!(s, "(assign {name} ").unwrap();
                } else {
                    write!(s, "({asnop} {name} ").unwrap();
                }
                write!(s, "{})", value.data.pretty(indent)).unwrap();
                s
            }
            Simp::Post { name, postop } => {
                format!("({postop} {name})")
            }
            Simp::Expr(expr) => expr.data.pretty(indent),
            Simp::Decl(decl) => decl.pretty(indent),
        }
    }
}

impl Pretty for Control {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Control::If {
                cond,
                true_block,
                false_block,
            } => {
                let mut s = String::new();
                writeln!(s, "(if {}", cond.data.pretty(indent)).unwrap();
                write_indent(&mut s, indent + 1).unwrap();
                write!(s, "(then {})", true_block.pretty(indent + 1)).unwrap();
                if let Some(false_block) = false_block {
                    s.push('\n');
                    write_indent(&mut s, indent + 1).unwrap();
                    write!(s, "(else {})", false_block.pretty(indent + 1)).unwrap();
                }
                s.push_str(")");
                s
            }
            Control::While { cond, body } => {
                let mut s = String::new();
                writeln!(s, "(while {}", cond.data.pretty(indent)).unwrap();
                write_indent(&mut s, indent + 1).unwrap();
                write!(s, "{}", body.pretty(indent + 2)).unwrap();
                s
            }
            Control::For {
                init,
                cond,
                step,
                body,
            } => {
                let mut s = String::new();
                s.push_str("(for ");
                if let Some(init) = init {
                    write!(s, "{}", init.pretty(indent)).unwrap();
                }
                write!(s, " {} ", cond.data.pretty(indent)).unwrap();
                if let Some(step) = step {
                    write!(s, "{}", step.pretty(indent)).unwrap();
                } else {
                    s.push_str("nil");
                }
                s.push('\n');
                write_indent(&mut s, indent + 1).unwrap();
                write!(s, "{}", body.pretty(indent + 2)).unwrap();
                s
            }
            Control::Return(expr) => {
                let mut s = String::new();
                s.push_str("(return ");

                if let Some(expr) = expr {
                    write!(s, "{}", expr.data.pretty(indent)).unwrap();
                }

                s.push_str(")");
                s
            }
            Control::Break => "break".to_string(),
            Control::Continue => "continue".to_string(),
            Control::Assert(expr) => {
                format!("assert {}", expr.data.pretty(indent))
            }
        }
    }
}

impl Pretty for Stmt {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Stmt::Block(block) => block.pretty(indent),
            Stmt::Control(control) => control.pretty(indent),
            Stmt::Simp(simp) => simp.pretty(indent),
        }
    }
}

impl Pretty for Block {
    fn pretty(&self, indent: usize) -> String {
        let mut s = String::new();
        s.push_str("(block");
        for stmt in &self.0 {
            s.push('\n');
            write_indent(&mut s, indent + 1).unwrap();
            write!(s, "{}", stmt.data.pretty(indent + 1)).unwrap();
        }
        s.push('\n');
        write_indent(&mut s, indent).unwrap();
        s.push_str(")");
        s
    }
}

impl Pretty for GlobalDecl {
    fn pretty(&self, indent: usize) -> String {
        match self {
            GlobalDecl::Typedef { typ, alias } => {
                format!("(typedef {} {})", typ.pretty(indent), alias)
            }
            GlobalDecl::FunDef {
                ret_typ,
                name,
                params,
                body,
            } => {
                let mut s = String::new();
                write!(s, "(fun_def {} {} [", ret_typ.pretty(indent), name).unwrap();
                for (i, param) in params.iter().enumerate() {
                    write!(s, "{}", param.pretty(indent)).unwrap();
                    if i != params.len() - 1 {
                        s.push_str(", ");
                    }
                }
                s.push_str("]");
                if let Some(body) = body {
                    s.push('\n');
                    write_indent(&mut s, indent + 1).unwrap();
                    write!(s, "{}", body.pretty(indent + 1)).unwrap();
                }
                s.push_str(")");
                s
            }
        }
    }
}

impl Pretty for Param {
    fn pretty(&self, indent: usize) -> String {
        format!("{} {}", self.typ.pretty(indent), self.name)
    }
}

impl Pretty for RetTyp {
    fn pretty(&self, indent: usize) -> String {
        match self {
            RetTyp::Typ(typ) => typ.pretty(indent),
            RetTyp::Void => "void".to_string(),
        }
    }
}

impl Pretty for Program {
    fn pretty(&self, indent: usize) -> String {
        let mut s = String::new();
        s.push_str("(program");
        for gdecl in &self.0 {
            s.push('\n');
            write_indent(&mut s, indent + 1).unwrap();
            write!(s, "{}", gdecl.pretty(indent + 1)).unwrap();
        }
        s.push('\n');
        write_indent(&mut s, indent).unwrap();
        s.push_str(")");
        s
    }
}

crate::impl_display_from_pretty!(
    PostOp, UnOp, AsnOp, BinOp, Typ, Expr, Decl, Simp, Control, Stmt, Block, GlobalDecl, Param,
    RetTyp, Program,
);
