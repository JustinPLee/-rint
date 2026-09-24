use std::collections::HashMap;

use crate::{
    ast::{BinOp, Expr, LExpr, LStmt, Program, Stmt, Typ},
    location::unloc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitStatus {
    Decl,
    Init,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VarInfo {
    typ: Typ,
    status: InitStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Analysis {
    typ: Typ,
    returns: bool,
}

type Env = HashMap<String, VarInfo>;

// TODO: add more error types + diagnostics + better error messages
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisError {
    TypeCheck,
    UseBeforeDecl,
    UseBeforeInit,
    NoReturn,
}

pub fn typecheck(program: &Program) -> Result<(), AnalysisError> {
    let mut env = Env::new();
    let mut returns = false;
    for stmt in &program.0 {
        if tc_stmt(stmt, &mut env)?.returns {
            returns = true;
        }
    }
    if returns {
        Ok(())
    } else {
        Err(AnalysisError::NoReturn)
    }
}

fn tc_stmt(stmt: &LStmt, env: &mut Env) -> Result<Analysis, AnalysisError> {
    match &stmt.0 {
        // handle scopes later
        Stmt::Decl { typ, name, rest } => {
            let mut inner_env = env.clone();
            inner_env.insert(
                name.clone(),
                VarInfo {
                    status: InitStatus::Decl,
                    typ: typ.clone(),
                },
            );

            let mut returns = false;
            for stmt in rest {
                returns |= tc_stmt(stmt, &mut inner_env)?.returns;
            }

            Ok(Analysis {
                typ: typ.clone(),
                returns,
            })
        }

        Stmt::Assign { name, value } => {
            let value_type = tc_expr(&value, env)?.typ;
            let var = env.get(name).ok_or_else(|| AnalysisError::UseBeforeDecl)?;
            if var.typ != value_type {
                return Err(AnalysisError::TypeCheck);
            }

            let var = env.entry(name.clone()).or_insert(VarInfo {
                typ: value_type,
                status: InitStatus::Init,
            });
            var.status = InitStatus::Init;
            Ok(Analysis {
                typ: var.typ,
                returns: false,
            })
        }

        Stmt::Return(expr) => {
            let ret_type = tc_expr(&expr, env)?.typ;

            // only main function for now
            // main returns int
            if ret_type != Typ::Int {
                return Err(AnalysisError::TypeCheck);
            }
            Ok(Analysis {
                typ: ret_type,
                returns: true,
            })
        }
        Stmt::If {
            cond,
            true_stmt,
            false_stmt,
        } => {
            let cond_typ = tc_expr(&cond, env)?.typ;
            let Analysis {
                typ: true_typ,
                returns: true_rets,
            } = tc_stmt(&*true_stmt, env)?;
            let Analysis {
                typ: false_typ,
                returns: false_rets,
            } = tc_stmt(&*false_stmt, env)?;
            require_type(cond_typ, Typ::Bool)?;
            require_type(true_typ, false_typ)?;
            Ok(Analysis {
                typ: true_typ,
                returns: true_rets && false_rets,
            })
        }
        Stmt::While { cond, body } => {
            let cond_typ = tc_expr(&cond, env)?.typ;
            let body_typ = tc_stmt(&*body, env)?.typ;
            require_type(cond_typ, Typ::Bool)?;
            Ok(Analysis {
                typ: body_typ,
                returns: false,
            })
        }
        Stmt::StmtExpr(expr) => tc_expr(&expr, env),
        // NOTE: this is wrong
        Stmt::Seq(stmts) => {
            let mut returns = false;
            for s in stmts {
                returns |= tc_stmt(s, env)?.returns;
            }
            Ok(Analysis {
                typ: Typ::Int,
                returns,
            })
        }
    }
}

fn tc_expr(expr: &LExpr, env: &Env) -> Result<Analysis, AnalysisError> {
    match unloc(expr) {
        Expr::Ident(name) => {
            let var = env.get(&name).ok_or_else(|| AnalysisError::UseBeforeDecl)?;
            if var.status == InitStatus::Decl {
                return Err(AnalysisError::UseBeforeInit);
            }

            Ok(Analysis {
                typ: var.typ,
                returns: false,
            })
        }
        Expr::Int(_) => Ok(Analysis {
            typ: Typ::Int,
            returns: false,
        }),
        Expr::True | Expr::False => Ok(Analysis {
            typ: Typ::Bool,
            returns: false,
        }),
        Expr::BinOp { op, lhs, rhs } => {
            let lhs_type = tc_expr(&*lhs, env)?.typ;
            let rhs_type = tc_expr(&*rhs, env)?.typ;

            match op {
                BinOp::Plus
                | BinOp::Minus
                | BinOp::Times
                | BinOp::Div
                | BinOp::Mod
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::LShift
                | BinOp::RShift => {
                    require_type(lhs_type, Typ::Int)?;
                    require_type(rhs_type, Typ::Int)?;
                    Ok(Analysis {
                        typ: Typ::Int,
                        returns: false,
                    })
                }
                BinOp::Greater | BinOp::GreaterEq | BinOp::Less | BinOp::LessEq => {
                    require_type(lhs_type, Typ::Int)?;
                    require_type(rhs_type, Typ::Int)?;
                    Ok(Analysis {
                        typ: Typ::Bool,
                        returns: false,
                    })
                }
                BinOp::LogicAnd | BinOp::LogicOr => {
                    require_type(lhs_type, Typ::Bool)?;
                    require_type(rhs_type, Typ::Bool)?;
                    Ok(Analysis {
                        typ: Typ::Bool,
                        returns: false,
                    })
                }
                BinOp::EqualEq => {
                    require_type(lhs_type, rhs_type)?;
                    Ok(Analysis {
                        typ: Typ::Bool,
                        returns: false,
                    })
                }
                BinOp::NotEq => {
                    require_type(lhs_type, rhs_type)?;
                    Ok(Analysis {
                        typ: Typ::Bool,
                        returns: false,
                    })
                }
            }
        }
        Expr::TernOp {
            cond,
            true_expr,
            false_expr,
        } => {
            let cond_typ = tc_expr(&*cond, env)?.typ;
            let true_typ = tc_expr(&*true_expr, env)?.typ;
            let false_typ = tc_expr(&*false_expr, env)?.typ;
            require_type(cond_typ, Typ::Bool)?;
            require_type(true_typ, false_typ)?;
            Ok(Analysis {
                typ: true_typ,
                returns: false,
            })
        }
    }
}

fn require_type(actual: Typ, expected: Typ) -> Result<(), AnalysisError> {
    if actual != expected {
        return Err(AnalysisError::TypeCheck);
    }

    Ok(())
}
