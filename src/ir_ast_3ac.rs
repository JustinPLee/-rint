use crate::ast::{
    BinOp as AstBinOp, Expr as AstExpr, LExpr, LStmt, Program as AstProgram, Stmt as AstStmt,
};
use crate::location::unloc;
use crate::temps::{Label, LabelGen, Temp, TempGen};
use std::collections::HashMap;
use std::fmt;

// reexport since they are the same
pub use crate::ast::BinOp;
// Translate AST into straightline 3ac with moves

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i32),
    Temp(Temp),
    BinOp {
        lhs: Box<Expr>,
        op: AstBinOp,
        rhs: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Move {
        dest: Temp,
        src: Expr,
    },
    Return(Expr),
    Jump(Label),
    CJump {
        lhs: Expr,
        op: AstBinOp,
        rhs: Expr,
        target: Label,
    },
    Label(Label),
}

pub struct Program(pub Vec<Stmt>);

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "program:")?;
        for stmt in &self.0 {
            writeln!(f, "{}", stmt)?;
        }
        Ok(())
    }
}

impl fmt::Display for Temp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "t{}", self.0)
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Move { dest, src } => {
                write!(f, "{dest} ← {src}")
            }
            Stmt::Return(exp) => {
                write!(f, "return {exp}")
            }
            Stmt::Jump(label) => {
                write!(f, "jump {label}")
            }
            Stmt::CJump {
                lhs,
                op,
                rhs,
                target,
            } => {
                write!(f, "if {lhs} {op} {rhs} jump {target}")
            }
            Stmt::Label(label) => {
                write!(f, "{label}:")
            }
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Int(x) => write!(f, "{x}"),
            Expr::Temp(t) => write!(f, "{t}"),
            Expr::BinOp { lhs, op, rhs } => {
                write!(f, "({lhs} {op} {rhs})")
            }
        }
    }
}

fn trans_expr(
    env: &HashMap<String, Temp>,
    expr: &LExpr,
    tempgen: &mut TempGen,
    labelgen: &mut LabelGen,
    stmts: &mut Vec<Stmt>,
) -> Expr {
    match unloc(expr) {
        // convert string variable names to temporaries
        AstExpr::Ident(name) => {
            let temp = env.get(&name).expect("variable is in env");
            Expr::Temp(*temp)
        }
        AstExpr::Int(value) => Expr::Int(value),
        AstExpr::True => Expr::Int(1),
        AstExpr::False => Expr::Int(0),
        AstExpr::BinOp { op, lhs, rhs } => Expr::BinOp {
            lhs: Box::new(trans_expr(env, &*lhs, tempgen, labelgen, stmts)),
            op,
            rhs: Box::new(trans_expr(env, &*rhs, tempgen, labelgen, stmts)),
        },
        // cjump condition true
        // ... false ...

        // jump end

        // label true:
        // ... true ...

        // label end
        //
        AstExpr::TernOp {
            cond,
            true_expr,
            false_expr,
        } => {
            let cond_raw = trans_expr(env, &*cond, tempgen, labelgen, stmts);
            let true_value = trans_expr(env, &*true_expr, tempgen, labelgen, stmts);
            let false_value = trans_expr(env, &*false_expr, tempgen, labelgen, stmts);
            let temp = tempgen.fresh();

            let label_true = labelgen.fresh();
            let label_end = labelgen.fresh();

            let (lhs, op, rhs) = to_binop_cond(cond_raw);

            stmts.push(Stmt::CJump {
                lhs,
                op,
                rhs,
                target: label_true,
            });
            stmts.push(Stmt::Move {
                dest: temp,
                src: false_value,
            });
            stmts.push(Stmt::Jump(label_end));
            stmts.push(Stmt::Label(label_true));
            stmts.push(Stmt::Move {
                dest: temp,
                src: true_value,
            });
            stmts.push(Stmt::Label(label_end));

            Expr::Temp(temp)
        }
    }
}

fn trans_stmts(
    env: &mut HashMap<String, Temp>,
    tempgen: &mut TempGen,
    labelgen: &mut LabelGen,
    ast: &[LStmt],
) -> Vec<Stmt> {
    let mut result = Vec::new();
    for stmt in ast {
        result.extend(trans_stmt(env, tempgen, labelgen, stmt));
    }
    result
}

fn trans_stmt(
    env: &mut HashMap<String, Temp>,
    tempgen: &mut TempGen,
    labelgen: &mut LabelGen,
    stmt: &LStmt,
) -> Vec<Stmt> {
    let mut result = Vec::new();
    match unloc(stmt) {
        AstStmt::Decl {
            typ: _typ,
            name,
            rest,
        } => {
            let temp = tempgen.fresh();
            env.insert(name.clone(), temp);
            result.extend(trans_stmts(env, tempgen, labelgen, &rest));
            env.remove(&name);
        }

        AstStmt::Assign { name, value } => {
            // assume already declared
            let dest = *env.get(&name).expect("variable is in env");
            let src = trans_expr(env, &value, tempgen, labelgen, &mut result);
            result.push(Stmt::Move { dest, src });
        }
        AstStmt::Return(expr) => {
            let expr = trans_expr(env, &expr, tempgen, labelgen, &mut result);
            result.push(Stmt::Return(expr));
            // don't process code after returns
        }
        AstStmt::If {
            cond,
            true_stmt,
            false_stmt,
        } => {
            let cond_raw = trans_expr(env, &cond, tempgen, labelgen, &mut result);
            let label_true = labelgen.fresh();
            let label_end = labelgen.fresh();
            let (lhs, op, rhs) = to_binop_cond(cond_raw);
            result.push(Stmt::CJump {
                lhs,
                op,
                rhs,
                target: label_true,
            });
            result.extend(trans_stmt(env, tempgen, labelgen, &*false_stmt));
            result.push(Stmt::Jump(label_end));
            result.push(Stmt::Label(label_true));
            result.extend(trans_stmt(env, tempgen, labelgen, &*true_stmt));
            result.push(Stmt::Label(label_end));
        }

        AstStmt::While { cond, body } => {
            let label_start = labelgen.fresh();
            let label_stop = labelgen.fresh();

            let cond_raw = trans_expr(env, &cond, tempgen, labelgen, &mut result);

            let body_result = trans_stmt(env, tempgen, labelgen, &*body);
            let (lhs, op, rhs) = to_binop_cond(cond_raw);

            result.push(Stmt::Jump(label_stop));
            result.push(Stmt::Label(label_start));
            result.extend(body_result);
            result.push(Stmt::Label(label_stop));
            result.push(Stmt::CJump {
                lhs,
                op,
                rhs,
                target: label_start,
            });
        }
        AstStmt::StmtExpr(expr) => {
            // pure function so does nothing
            // but this will chagne in the future
            let _ = trans_expr(env, &expr, tempgen, labelgen, &mut result);
        }
        AstStmt::Seq(stmts) => {
            result.extend(trans_stmts(env, tempgen, labelgen, &stmts));
        }
    }
    result
}

fn to_binop_cond(expr: Expr) -> (Expr, AstBinOp, Expr) {
    match expr {
        Expr::BinOp { lhs, op, rhs } => (*lhs, op, *rhs),
        expr => (expr, AstBinOp::EqualEq, Expr::Int(1)),
    }
}

pub fn translate(ast: &AstProgram, temps: &mut TempGen) -> Program {
    let mut env = HashMap::new();
    let mut labelgen = LabelGen::new();
    Program(trans_stmts(&mut env, temps, &mut labelgen, &ast.0))
}
