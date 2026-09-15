// parser -> cst -> elaborate -> ast

use crate::location::{Located, unloc};
use std::fmt;

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
    LShiftEq,
    RShiftEq,
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
        op: UnOp,
        oper: Box<LExpr>,
    },
    BinOp {
        op: BinOp,
        lhs: Box<LExpr>,
        rhs: Box<LExpr>,
    },
    TernOp {
        cond: Box<LExpr>,
        true_expr: Box<LExpr>,
        false_expr: Box<LExpr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    If {
        cond: LExpr,
        true_stmt: Box<LStmt>,
        false_stmt: Box<LStmt>,
    },
    While {
        cond: LExpr,
        body: Box<LStmt>,
    },
    For {
        init: Option<Simp>,
        cond: LExpr,
        step: Option<Simp>,
        body: Box<LStmt>,
    },
    Return(LExpr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LValue(pub String);

pub type LExpr = Located<Expr>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typ {
    Int,
    Bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decl {
    Decl {
        typ: Typ,
        name: String,
    },
    Init {
        typ: Typ,
        name: String,
        value: LExpr,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Block(Vec<LStmt>),
    Control(Control),
    Simp(Simp),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Simp {
    Assign {
        name: LValue,
        asnop: AsnOp,
        value: LExpr,
    },
    Post {
        name: LValue,
        postop: PostOp,
    },
    StmtExpr(LExpr),
    Decl(Decl),
}

pub type LStmt = Located<Stmt>;

#[derive(Clone, Debug, PartialEq)]
pub struct Program(pub Vec<LStmt>);

impl fmt::Display for PostOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            PostOp::DoublePlus => "++",
            PostOp::DoubleMinus => "--",
        };
        write!(f, "{op}")
    }
}

impl fmt::Display for UnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            UnOp::Exclam => "!",
            UnOp::Negate => "-",
        };
        write!(f, "{op}")
    }
}

impl fmt::Display for AsnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        write!(f, "{op}")
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        write!(f, "{op}")
    }
}

impl fmt::Display for LValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Typ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Typ::Int => write!(f, "int"),
            Typ::Bool => write!(f, "bool"),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Ident(name) => write!(f, "{name}"),
            Expr::Int(value) => write!(f, "{value}"),
            Expr::True => write!(f, "true"),
            Expr::False => write!(f, "false"),

            Expr::UnOp { op, oper } => {
                write!(f, "({op} {})", oper.0)
            }

            Expr::BinOp { op, lhs, rhs } => {
                write!(f, "({op} {} {})", lhs.0, rhs.0)
            }

            Expr::TernOp {
                cond,
                true_expr,
                false_expr,
            } => {
                write!(f, "(?: {} {} {})", cond.0, true_expr.0, false_expr.0)
            }
        }
    }
}

impl fmt::Display for Decl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Decl::Decl { typ, name } => {
                write!(f, "(declare {name} {typ})")
            }

            Decl::Init { typ, name, value } => {
                write!(f, "(declare {name} {typ} {})", value.0)
            }
        }
    }
}

impl fmt::Display for Simp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Simp::Assign { name, asnop, value } => {
                if matches!(asnop, &AsnOp::Eq) {
                    write!(f, "(assign {name} {})", value.0)
                } else {
                    write!(f, "({asnop} {name} {})", value.0)
                }
            }

            Simp::Post { name, postop } => {
                write!(f, "({postop} {name})")
            }

            Simp::StmtExpr(expr) => {
                write!(f, "{}", expr.0)
            }

            Simp::Decl(decl) => {
                write!(f, "{decl}")
            }
        }
    }
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Control::If {
                cond,
                true_stmt,
                false_stmt,
            } => {
                write!(
                    f,
                    "(if {}\nthen {}\nelse {})",
                    cond.0, true_stmt.0, false_stmt.0
                )
            }

            Control::While { cond, body } => {
                write!(f, "(while {}\n{})", cond.0, body.0)
            }

            Control::For {
                init,
                cond,
                step,
                body,
            } => {
                write!(f, "(for ")?;

                match init {
                    Some(init) => write!(f, "{init}")?,
                    None => write!(f, "")?,
                }

                write!(f, "{}", cond.0)?;

                match step {
                    Some(step) => write!(f, "{step}")?,
                    None => write!(f, "nil")?,
                }

                write!(f, "\n{})", body.0)
            }

            Control::Return(expr) => {
                write!(f, "(return {})", expr.0)
            }
        }
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Block(stmts) => {
                writeln!(f, "(block")?;

                for (i, stmt) in stmts.iter().enumerate() {
                    if i + 1 == stmts.len() {
                        write!(f, "{}", stmt.0)?;
                    } else {
                        writeln!(f, "{}", stmt.0)?;
                    }
                }

                write!(f, ")")
            }

            Stmt::Control(control) => {
                write!(f, "{control}")
            }

            Stmt::Simp(simp) => {
                write!(f, "{simp}")
            }
        }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "(program")?;

        for lstmt in &self.0 {
            writeln!(f, "{}", lstmt.0)?;
        }

        write!(f, ")")
    }
}
