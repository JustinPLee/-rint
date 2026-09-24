use crate::{
    ast::BinOp,
    ir_ast_3ac::{Expr, Program, Stmt},
    temps::Temp,
};
use std::collections::HashMap;

// not working

// simplify temps into constants, and constants in binary ops to constants and so on
// rather too powerful in this simple language as of now
fn simplify(exp: &Expr, env: &HashMap<Temp, Expr>) -> Expr {
    match exp {
        Expr::Int(_) => exp.clone(),
        // get a temp's parent alias
        Expr::Temp(t) => env.get(t).cloned().unwrap_or_else(|| exp.clone()),
        Expr::BinOp { lhs, op, rhs } => {
            let lhs = simplify(lhs, env);
            let rhs = simplify(rhs, env);
            fold_constants(op.clone(), lhs, rhs)
        }
    }
}

fn fold_constants(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    if let (Expr::Int(l), Expr::Int(r)) = (&lhs, &rhs) {
        let (l, r) = (*l, *r);
        match op {
            BinOp::Plus => return Expr::Int(l.wrapping_add(r)),
            BinOp::Minus => return Expr::Int(l.wrapping_sub(r)),
            BinOp::Times => return Expr::Int(l.wrapping_mul(r)),
            BinOp::Div if r != 0 => return Expr::Int(l.wrapping_div(r)),
            // rem_euclid v wrapping_rem?
            BinOp::Mod if r != 0 => return Expr::Int(l.rem_euclid(r)),
            _ => {}
        }
    }

    Expr::BinOp {
        lhs: Box::new(lhs),
        op,
        rhs: Box::new(rhs),
    }
}

pub fn copy_const_prop(program: Program) -> Program {
    let mut env: HashMap<Temp, Expr> = HashMap::new();
    let mut result = Vec::with_capacity(program.0.len());
    for stmt in program.0 {
        match stmt {
            Stmt::Move { dest, src } => {
                let src = simplify(&src, &env);
                match &src {
                    Expr::Int(_) | Expr::Temp(_) => {
                        env.insert(dest, src.clone());
                    }
                    _ => {
                        env.remove(&dest);
                    }
                }
                result.push(Stmt::Move { dest, src });
            }
            Stmt::Return(exp) => result.push(Stmt::Return(simplify(&exp, &env))),
            Stmt::Jump(label) => todo!(),
            Stmt::CJump {
                lhs,
                op,
                rhs,
                target,
            } => todo!(),
            Stmt::Label(label) => todo!(),
        }
    }
    Program(result)
}
