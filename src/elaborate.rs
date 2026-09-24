use crate::ast::{BinOp, Expr, LExpr, LStmt, Program, Stmt, Typ};
use crate::cst::{
    AsnOp as CAsnOp, BinOp as CBinOp, Control, Decl as CDecl, Expr as CExpr, LExpr as CLExpr,
    LStmt as CLStmt, LValue as CLValue, PostOp as CPostOp, Program as CProgram, Simp as CSimp,
    Stmt as CStmt, Typ as CTyp, UnOp as CUnOp,
};
use crate::location::{Located, Location};

// straightforward translation
fn elaborate_expr(expr: CLExpr) -> LExpr {
    let loc = expr.1.clone();
    match expr.0 {
        CExpr::Ident(name) => Located(Expr::Ident(name), loc),
        CExpr::Int(i) => Located(Expr::Int(i), loc),
        CExpr::True => Located(Expr::True, loc),
        CExpr::False => Located(Expr::False, loc),
        CExpr::UnOp { op, oper } => elaborate_unop(op, *oper),
        CExpr::BinOp { op, lhs, rhs } => elaborate_binop(op, *lhs, *rhs),
        CExpr::TernOp {
            cond,
            true_expr,
            false_expr,
        } => Located(
            Expr::TernOp {
                cond: Box::new(elaborate_expr(*cond)),
                true_expr: Box::new(elaborate_expr(*true_expr)),
                false_expr: Box::new(elaborate_expr(*false_expr)),
            },
            loc,
        ),
    }
}

fn bin(op: BinOp, lhs: LExpr, rhs: LExpr) -> LExpr {
    let loc = lhs.clone().1.merge(&rhs.1);
    Located(
        Expr::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        loc,
    )
}

fn elaborate_binop(op: CBinOp, lhs: CLExpr, rhs: CLExpr) -> LExpr {
    let lhs = elaborate_expr(lhs);
    let rhs = elaborate_expr(rhs);
    // translate && and || to ternaries
    match op {
        CBinOp::LogicAnd => {
            let loc = lhs.1.clone().merge(&rhs.1);
            Located(
                Expr::TernOp {
                    cond: Box::new(lhs),
                    true_expr: Box::new(rhs),
                    false_expr: Box::new(Located(Expr::False, loc.clone())),
                },
                loc,
            )
        }
        CBinOp::LogicOr => {
            let loc = lhs.1.clone().merge(&rhs.1);
            Located(
                Expr::TernOp {
                    cond: Box::new(lhs),
                    true_expr: Box::new(Located(Expr::True, loc.clone())),
                    false_expr: Box::new(rhs),
                },
                loc,
            )
        }
        CBinOp::Plus => bin(BinOp::Plus, lhs, rhs),
        CBinOp::Minus => bin(BinOp::Minus, lhs, rhs),
        CBinOp::Times => bin(BinOp::Times, lhs, rhs),
        CBinOp::Div => bin(BinOp::Div, lhs, rhs),
        CBinOp::Mod => bin(BinOp::Mod, lhs, rhs),
        CBinOp::BitAnd => bin(BinOp::BitAnd, lhs, rhs),
        CBinOp::BitOr => bin(BinOp::BitOr, lhs, rhs),
        CBinOp::BitXor => bin(BinOp::BitXor, lhs, rhs),
        CBinOp::LShift => bin(BinOp::LShift, lhs, rhs),
        CBinOp::RShift => bin(BinOp::RShift, lhs, rhs),
        CBinOp::Greater => bin(BinOp::Greater, lhs, rhs),
        CBinOp::GreaterEq => bin(BinOp::GreaterEq, lhs, rhs),
        CBinOp::Less => bin(BinOp::Less, lhs, rhs),
        CBinOp::LessEq => bin(BinOp::LessEq, lhs, rhs),
        CBinOp::EqualEq => bin(BinOp::EqualEq, lhs, rhs),
        CBinOp::NotEq => bin(BinOp::NotEq, lhs, rhs),
    }
}

fn elaborate_unop(op: CUnOp, oper: CLExpr) -> LExpr {
    let oper = elaborate_expr(oper);
    let loc = oper.1.clone();
    match op {
        // -x -> 0 - x
        CUnOp::Negate => {
            let zero = Located(Expr::Int(0), loc.clone());
            bin(BinOp::Minus, zero, oper)
        }
        // !x -> x == false
        CUnOp::Exclam => {
            let false_expr = Located(Expr::False, loc.clone());
            bin(BinOp::EqualEq, oper, false_expr)
        }
    }
}

// declare node encapsulates other statements
// declare(τ, name, ....)
fn elaborate_stmts(stmts: &[CLStmt]) -> Vec<LStmt> {
    let Some((head, rest)) = stmts.split_first() else {
        return Vec::new();
    };

    match &head.0 {
        CStmt::Simp(CSimp::Decl(decl)) => {
            vec![elaborate_declare(decl.clone(), head.1.clone(), rest)]
        }
        _ => {
            let mut result = vec![elaborate_stmt(head)];
            result.extend(elaborate_stmts(rest));
            result
        }
    }
}

fn flatten_stmt(stmts: Vec<LStmt>) -> LStmt {
    match (stmts.first(), stmts.last()) {
        (Some(first), Some(last)) => {
            Located(Stmt::Seq(stmts.clone()), first.1.clone().merge(&last.1))
        }
        _ => Located(Stmt::Seq(Vec::new()), Default::default()),
    }
}

fn elaborate_stmt(stmt: &CLStmt) -> LStmt {
    match &stmt.0 {
        CStmt::Block(inner) => flatten_stmt(elaborate_stmts(inner)),
        CStmt::Control(ctl) => elaborate_control(ctl.clone(), stmt.1.clone()),
        CStmt::Simp(simp) => elaborate_simp(simp.clone(), stmt.1.clone()),
    }
}

fn elaborate_declare(decl: CDecl, src_span: Location, rest: &[CLStmt]) -> LStmt {
    let rest_stmts = elaborate_stmts(rest);
    let stmt = match decl {
        CDecl::Decl { typ, name } => Stmt::Decl {
            typ: elaborate_typs(typ),
            name,
            rest: rest_stmts,
        },
        CDecl::Init { typ, name, value } => {
            let assign = Located(
                Stmt::Assign {
                    name: name.clone(),
                    value: elaborate_expr(value),
                },
                src_span.clone(),
            );
            let mut rest = vec![assign];
            rest.extend(rest_stmts);
            Stmt::Decl {
                typ: elaborate_typs(typ),
                name,
                rest,
            }
        }
    };

    Located(stmt, src_span)
}

fn elaborate_typs(typ: CTyp) -> Typ {
    match typ {
        CTyp::Int => Typ::Int,
        CTyp::Bool => Typ::Bool,
    }
}

fn elaborate_simp(simp: CSimp, src_span: Location) -> LStmt {
    let stmt = match simp {
        CSimp::Assign { name, asnop, value } => return elaborate_assign(name, asnop, value),
        CSimp::Post { name, postop } => return elaborate_post(name, postop),
        CSimp::StmtExpr(e) => Stmt::StmtExpr(elaborate_expr(e)),
        CSimp::Decl(_) => panic!(""),
    };
    Located(stmt, src_span)
}

fn elaborate_assign(name: CLValue, asnop: CAsnOp, value: CLExpr) -> LStmt {
    let value = elaborate_expr(value);
    let ident = Located(Expr::Ident(name.0.clone()), value.1.clone());
    let rhs = match asnop {
        CAsnOp::Eq => value,
        CAsnOp::PlusEq => bin(BinOp::Plus, ident.clone(), value),
        CAsnOp::MinusEq => bin(BinOp::Minus, ident.clone(), value),
        CAsnOp::TimesEq => bin(BinOp::Times, ident.clone(), value),
        CAsnOp::DivEq => bin(BinOp::Div, ident.clone(), value),
        CAsnOp::ModEq => bin(BinOp::Mod, ident.clone(), value),
        CAsnOp::AndEq => bin(BinOp::BitAnd, ident.clone(), value),
        CAsnOp::XorEq => bin(BinOp::BitXor, ident.clone(), value),
        CAsnOp::OrEq => bin(BinOp::BitOr, ident.clone(), value),
        CAsnOp::LShiftEq => bin(BinOp::LShift, ident.clone(), value),
        CAsnOp::RShiftEq => bin(BinOp::RShift, ident.clone(), value),
    };
    let loc = rhs.1.clone();
    Located(
        Stmt::Assign {
            name: name.0,
            value: rhs,
        },
        loc,
    )
}

fn elaborate_post(name: CLValue, postop: CPostOp) -> LStmt {
    let op = match postop {
        CPostOp::DoublePlus => BinOp::Plus,
        CPostOp::DoubleMinus => BinOp::Minus,
    };
    let ident = Located(Expr::Ident(name.0.clone()), Default::default());
    let one = Located(Expr::Int(1), Default::default());
    let value = bin(op, ident, one);
    let loc = value.1.clone();
    Located(
        Stmt::Assign {
            name: name.0,
            value,
        },
        loc,
    )
}

fn elaborate_control(ctl: Control, src_span: Location) -> LStmt {
    let stmt = match ctl {
        Control::If {
            cond,
            true_stmt,
            false_stmt,
        } => Stmt::If {
            cond: elaborate_expr(cond),
            true_stmt: Box::new(elaborate_stmt(&true_stmt)),
            false_stmt: Box::new(elaborate_stmt(&*false_stmt)),
        },
        Control::While { cond, body } => Stmt::While {
            cond: elaborate_expr(cond),
            body: Box::new(elaborate_stmt(&body)),
        },
        Control::Return(e) => Stmt::Return(elaborate_expr(e)),
        Control::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(CSimp::Decl(_)) = &step {
                panic!("cannot declare in step");
            }
            let mut while_body = vec![*body];
            if let Some(step) = step {
                while_body.push(Located(CStmt::Simp(step), src_span.clone()));
            }
            let while_body = Located(CStmt::Block(while_body), src_span.clone());
            let while_stmt = Located(
                CStmt::Control(Control::While {
                    cond,
                    body: Box::new(while_body),
                }),
                src_span.clone(),
            );

            let mut program = Vec::new();
            if let Some(init) = init {
                program.push(Located(CStmt::Simp(init), src_span.clone()));
            }
            program.push(while_stmt);

            return flatten_stmt(elaborate_stmts(&program));
        }
    };
    Located(stmt, src_span)
}

pub fn elaborate(program: CProgram) -> Program {
    Program(elaborate_stmts(&program.0))
}
