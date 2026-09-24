use crate::location::Located;
use crate::utils::{Pretty, write_indent};
use std::fmt::Write;

pub use crate::ast_parse::BinOp;

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Typ {
    Int,
    Bool,
    Void,
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Param {
    pub typ: LTyp,
    pub name: LIdent,
}

#[derive(Clone, Debug)]
pub enum GlobalDecl {
    Typedef {
        typ: LTyp,
        alias: LIdent,
    },
    FunDef {
        ret_typ: LTyp,
        name: LIdent,
        params: Vec<LParam>,
        body: Option<LBlock>, // if none, then this is a function declaration
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Ident(String),
    Int(i32),
    True,
    False,
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
        args: Vec<Box<LExpr>>,
    },
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Assign {
        name: LIdent,
        value: LExpr,
    },
    If {
        cond: LExpr,
        true_block: LBlock,
        false_block: Option<LBlock>,
    },
    While {
        cond: LExpr,
        body: LBlock,
    },
    // while cannot be fully lowered in for loops due to continue statements
    // a while loop's continue jumps to the condition label
    // but a for loops continue has to execute the step statement first before jumping
    // TODO: try to find a cleaner solution?
    For {
        init: Option<Box<LStmt>>,
        cond: LExpr,
        step: Option<Box<LStmt>>,
        body: LBlock,
    },
    Return(Option<LExpr>),
    // variables are statically scoped at their declaration
    Decl {
        typ: LTyp,
        name: LIdent,
        block: LBlock,
    },
    Expr(LExpr),
    Assert(LExpr),
    Block(LBlock),
    Break,
    Continue,
}

#[derive(Clone, Debug)]
pub struct Block(pub Vec<LStmt>);

#[derive(Clone, Debug)]
pub struct Program(pub Vec<LGlobalDecl>);

pub type LStmt = Located<Stmt>;
pub type LIdent = Located<String>;
pub type LTyp = Located<Typ>;
pub type LBinOp = Located<BinOp>;
pub type LExpr = Located<Expr>;
pub type LBlock = Located<Block>;
pub type LParam = Located<Param>;
pub type LGlobalDecl = Located<GlobalDecl>;
pub type LProgram = Located<Program>;

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

impl Pretty for Typ {
    fn pretty(&self, _indent: usize) -> String {
        match self {
            Typ::Int => "int".to_string(),
            Typ::Bool => "bool".to_string(),
            Typ::Void => "void".to_string(),
            Typ::Named(typ) => typ.to_string(),
        }
    }
}
impl Pretty for Stmt {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Stmt::Assign { name, value } => {
                format!("(assign {} {})", name, value.data.pretty(indent))
            }
            Stmt::If {
                cond,
                true_block,
                false_block,
            } => {
                let mut s = String::new();
                writeln!(s, "(if {}", cond.data.pretty(indent)).unwrap();
                write_indent(&mut s, indent + 1).unwrap();
                write!(s, "then {}", true_block.pretty(indent + 1)).unwrap();
                if let Some(false_block) = false_block {
                    s.push('\n');
                    write_indent(&mut s, indent + 1).unwrap();
                    write!(s, "else {}", false_block.pretty(indent + 1)).unwrap();
                }
                s.push_str(")");
                s
            }
            Stmt::While { cond, body } => {
                let mut s = String::new();
                writeln!(s, "(while {}", cond.data.pretty(indent)).unwrap();
                write_indent(&mut s, indent + 1).unwrap();
                write!(s, "{}", body.pretty(indent + 2)).unwrap();
                s
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                let init = init
                    .as_ref()
                    .map(|stmt| stmt.data.pretty(indent))
                    .unwrap_or_default();
                let step = step
                    .as_ref()
                    .map(|stmt| stmt.data.pretty(indent))
                    .unwrap_or_default();
                let mut s = String::new();
                writeln!(s, "(for ({init}; {}; {step})", cond.data.pretty(indent)).unwrap();
                write_indent(&mut s, indent + 1).unwrap();
                write!(s, "{}", body.pretty(indent + 2)).unwrap();
                s
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    format!("(return {})", expr.data.pretty(indent))
                } else {
                    "(return)".to_string()
                }
            }
            Stmt::Decl { typ, name, block } => {
                let mut s = String::new();
                writeln!(s, "(declare {} {}", name, typ.pretty(indent)).unwrap();
                write_indent(&mut s, indent + 1).unwrap();
                write!(s, "{})", block.pretty(indent + 1)).unwrap();
                s
            }
            Stmt::Expr(expr) => expr.data.pretty(indent),
            Stmt::Block(block) => block.pretty(indent),
            Stmt::Break => "(break)".to_string(),
            Stmt::Continue => "(continue)".to_string(),
            Stmt::Assert(expr) => format!("(assert {})", expr.data.pretty(indent)),
        }
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
                    if i > 0 {
                        s.push_str(", ");
                    }
                    write!(s, "{}", param.pretty(indent)).unwrap();
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
    fn pretty(&self, _indent: usize) -> String {
        format!("{} {}", self.typ.pretty(0), self.name)
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

impl Pretty for Expr {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Expr::Ident(name) => name.to_string(),
            Expr::Int(value) => value.to_string(),
            Expr::True => "true".to_string(),
            Expr::False => "false".to_string(),
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

crate::impl_display_from_pretty!(Block, Typ, Stmt, GlobalDecl, Param, Program, Expr);
