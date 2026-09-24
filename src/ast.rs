use crate::location::Located;
use std::fmt;

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
    Assign {
        name: String,
        value: LExpr,
    },
    If {
        cond: LExpr,
        false_stmt: Box<LStmt>,
        true_stmt: Box<LStmt>,
    },
    While {
        cond: LExpr,
        body: Box<LStmt>,
    },
    Return(LExpr),
    // this follows the specification
    // this way, variable scopes are clear
    Decl {
        typ: Typ,
        name: String,
        rest: Vec<LStmt>,
    },
    StmtExpr(LExpr),
    Seq(Vec<LStmt>),
}

pub type LStmt = Located<Stmt>;

pub struct Program(pub Vec<LStmt>);

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "(program ");
        for lstmt in &self.0 {
            writeln!(f, "{}", lstmt.0);
        }
        writeln!(f, ")")
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

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Ident(name) => write!(f, "{name}"),

            Expr::Int(value) => write!(f, "{value}"),

            Expr::True => write!(f, "true"),

            Expr::False => write!(f, "false"),

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

impl fmt::Display for Typ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Typ::Int => write!(f, "int"),
            Typ::Bool => write!(f, "bool"),
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

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Assign { name, value } => {
                write!(f, "(assign {name} {})", value.0)
            }

            Stmt::If {
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

            Stmt::While { cond, body } => {
                write!(f, "(while {}\n{})", cond.0, body.0)
            }

            Stmt::Return(expr) => {
                write!(f, "(return {})", expr.0)
            }

            Stmt::Decl { typ, name, rest } => {
                write!(f, "(declare {name} {typ}")?;

                for stmt in rest {
                    write!(f, "\n{}", stmt.0)?;
                }

                write!(f, ")")
            }

            Stmt::StmtExpr(expr) => {
                write!(f, "{}", expr.0)
            }

            Stmt::Seq(stmts) => {
                write!(f, "(seq")?;

                for stmt in stmts {
                    write!(f, "\n{}", stmt.0)?;
                }

                write!(f, ")")
            }
        }
    }
}
