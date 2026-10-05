// parse ast -> ast

use crate::ast::{
    BinOp, Block, Expr, GlobalDecl, LBinOp, LBlock, LExpr, LGlobalDecl, LLValue, LProgram, LStmt,
    LStructField, LTyp, LValue, Param, Program, Stmt, StructField, Typ,
};
use crate::ast_parse::{
    AsnOp as CAsnOp, BinOp as CBinOp, Control, Decl as CDecl, Expr as CExpr,
    GlobalDecl as CGlobalDecl, LAsnOp as CLAsnOp, LBinOp as CLBinOp, LBlock as CLBlock, LControl,
    LDecl as CLDecl, LExpr as CLExpr, LGlobalDecl as CLGlobalDecl, LIdent as CLIdent,
    LLValue as CLLValue, LParam as CLParam, LPostOp, LProgram as CProgram, LRetTyp, LSimp,
    LStmt as CLStmt, LStructField as CLStructField, LTyp as CLTyp, LUnOp as CLUnOp,
    LValue as CLValue, PostOp as CPostOp, RetTyp as CRetTyp, Simp as CSimp, Stmt as CStmt,
    Typ as CTyp, UnOp as CUnOp,
};
use crate::location::{Located, Location, loc};

// straightforward translation
fn elaborate_expr(expr: CLExpr) -> LExpr {
    let location = expr.location;

    let data = match expr.data {
        CExpr::Ident(name) => Expr::Ident(name),
        CExpr::Int(i) => Expr::Int(i),
        CExpr::True => Expr::True,
        CExpr::False => Expr::False,
        CExpr::Null => Expr::Null,
        CExpr::UnOp { op, oper } => elaborate_unop(op, *oper, location.clone()).data,
        CExpr::BinOp { op, lhs, rhs } => elaborate_binop(op, *lhs, *rhs, location.clone()).data,
        CExpr::TernOp {
            cond,
            true_expr,
            false_expr,
        } => Expr::TernOp {
            cond: Box::new(elaborate_expr(*cond)),
            true_expr: Box::new(elaborate_expr(*true_expr)),
            false_expr: Box::new(elaborate_expr(*false_expr)),
        },
        CExpr::FunCall { name, args } => Expr::FunCall {
            name: loc(name.data, name.location),
            args: args.into_iter().map(elaborate_expr).map(Box::new).collect(),
        },
        CExpr::Field { ident, field } => Expr::Field {
            ident: Box::new(elaborate_expr(*ident)),
            field: loc(field.data, field.location),
        },
        // a->b becomes (*a).b
        CExpr::Arrow { base, field } => {
            let deref_location = base.location.clone();
            Expr::Field {
                ident: Box::new(loc(
                    Expr::Deref(Box::new(elaborate_expr(*base))),
                    deref_location,
                )),
                field: loc(field.data, field.location),
            }
        }
        CExpr::Deref(expr) => Expr::Deref(Box::new(elaborate_expr(*expr))),
        CExpr::Index { base, index } => Expr::Index {
            base: Box::new(elaborate_expr(*base)),
            index: Box::new(elaborate_expr(*index)),
        },
        CExpr::Alloc(typ) => Expr::Alloc(elaborate_typ(typ)),
        CExpr::AllocArray { typ, size } => Expr::AllocArray {
            typ: elaborate_typ(typ),
            size: Box::new(elaborate_expr(*size)),
        },
    };

    loc(data, location)
}

fn bin(op: LBinOp, lhs: LExpr, rhs: LExpr, location: Location) -> LExpr {
    loc(
        Expr::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        location,
    )
}

fn elaborate_binop(op: CLBinOp, lhs: CLExpr, rhs: CLExpr, location: Location) -> LExpr {
    let lhs = elaborate_expr(lhs);
    let rhs = elaborate_expr(rhs);
    let op_location = op.location.clone();

    let op = match op.data {
        // translate && and || to ternaries
        // a && b becomes a ? b : false
        CBinOp::LogicAnd => {
            return loc(
                Expr::TernOp {
                    cond: Box::new(lhs),
                    true_expr: Box::new(rhs),
                    false_expr: Box::new(loc(Expr::False, location.clone())),
                },
                location,
            );
        }
        // a || b becomes a ? true : b
        CBinOp::LogicOr => {
            return loc(
                Expr::TernOp {
                    cond: Box::new(lhs),
                    true_expr: Box::new(loc(Expr::True, location.clone())),
                    false_expr: Box::new(rhs),
                },
                location,
            );
        }
        CBinOp::Plus => BinOp::Plus,
        CBinOp::Minus => BinOp::Minus,
        CBinOp::Times => BinOp::Times,
        CBinOp::Div => BinOp::Div,
        CBinOp::Mod => BinOp::Mod,
        CBinOp::BitAnd => BinOp::BitAnd,
        CBinOp::BitOr => BinOp::BitOr,
        CBinOp::BitXor => BinOp::BitXor,
        CBinOp::LShift => BinOp::LShift,
        CBinOp::RShift => BinOp::RShift,
        CBinOp::Greater => BinOp::Greater,
        CBinOp::GreaterEq => BinOp::GreaterEq,
        CBinOp::Less => BinOp::Less,
        CBinOp::LessEq => BinOp::LessEq,
        CBinOp::EqualEq => BinOp::EqualEq,
        CBinOp::NotEq => BinOp::NotEq,
    };

    bin(loc(op, op_location), lhs, rhs, location)
}

fn elaborate_unop(op: CLUnOp, oper: CLExpr, location: Location) -> LExpr {
    let oper = elaborate_expr(oper);
    let op_location = op.location.clone();

    match op.data {
        // -x becomes 0 - x
        CUnOp::Negate => {
            let zero = loc(Expr::Int(0), location.clone());

            bin(loc(BinOp::Minus, op_location), zero, oper, location)
        }
        // !x becomes x == false
        CUnOp::Exclam => {
            let false_expr = loc(Expr::False, location.clone());

            bin(loc(BinOp::EqualEq, op_location), oper, false_expr, location)
        }
        // ~x becomes x ^ -1
        CUnOp::BitNot => {
            let minus_one = loc(Expr::Int(-1), location.clone());

            bin(loc(BinOp::BitXor, op_location), oper, minus_one, location)
        }
    }
}

fn elaborate_block(block: CLBlock) -> LBlock {
    loc(Block(elaborate_stmts(&block.data.0)), block.location)
}

fn elaborate_stmt(stmt: CLStmt) -> Vec<LStmt> {
    let src_loc = stmt.location.clone();

    match stmt.data {
        CStmt::Block(block) => vec![loc(Stmt::Block(elaborate_block(block)), src_loc)],
        CStmt::Control(ctl) => elaborate_control(ctl),
        CStmt::Simp(simp) if matches!(&simp.data, CSimp::Decl(_)) => {
            // declare node must have knowledge of its siblings
            panic!("declaration should be handled by elaborate_stmts")
        }
        CStmt::Simp(simp) => vec![elaborate_simp(simp)],
    }
}

fn elaborate_stmts(stmts: &[CLStmt]) -> Vec<LStmt> {
    let mut result = Vec::new();

    for (i, stmt) in stmts.iter().enumerate() {
        if let CStmt::Simp(simp) = &stmt.data
            && let CSimp::Decl(decl) = &simp.data
        {
            result.push(elaborate_decl(
                decl.clone(),
                &stmts[i + 1..],
                stmt.location.clone(),
            ));
            break; // elaborate_decl will consume the remaining statements
        }

        result.extend(elaborate_stmt(stmt.clone()));
    }

    result
}

fn elaborate_decl(decl: CLDecl, rest: &[CLStmt], location: Location) -> LStmt {
    let rest = elaborate_stmts(rest);

    let (typ, name, block) = match decl.data {
        CDecl::Decl { typ, name } => (
            elaborate_typ(typ),
            loc(name.data, name.location),
            Block(rest),
        ),
        CDecl::Init { typ, name, value } => {
            let assign = loc(
                Stmt::Assign {
                    name: loc(LValue::Ident(name.clone()), name.location.clone()),
                    value: elaborate_expr(value),
                },
                location.clone(),
            );

            let mut block = vec![assign];
            block.extend(rest);
            (
                elaborate_typ(typ),
                loc(name.data, name.location),
                Block(block),
            )
        }
    };

    loc(
        Stmt::Decl {
            typ,
            name,
            block: loc(block, location.clone()),
        },
        location,
    )
}

fn elaborate_typ(typ: CLTyp) -> LTyp {
    let data = match typ.data {
        CTyp::Int => Typ::Int,
        CTyp::Bool => Typ::Bool,
        CTyp::Alias(t) => Typ::Alias(t),
        CTyp::Struct(t) => Typ::Struct(t),
        CTyp::Pointer(typ) => Typ::Pointer(Box::new(elaborate_typ(*typ))),
        CTyp::Array(typ) => Typ::Array(Box::new(elaborate_typ(*typ))),
    };

    loc(data, typ.location)
}

fn elaborate_ret_typ(ret_typ: LRetTyp) -> LTyp {
    match ret_typ.data {
        CRetTyp::Typ(typ) => elaborate_typ(typ),
        CRetTyp::Void => loc(Typ::Void, ret_typ.location),
    }
}

fn elaborate_simp(simp: LSimp) -> LStmt {
    let src_loc = simp.location;

    match simp.data {
        CSimp::Assign { name, asnop, value } => elaborate_assign(name, asnop, value, src_loc),
        CSimp::Post { name, postop } => elaborate_post(name, postop, src_loc),
        CSimp::Expr(expr) => loc(Stmt::Expr(elaborate_expr(expr)), src_loc),
        CSimp::Decl(_) => {
            // declaration needs information of sibling nodes, which this does not have
            panic!("declaration should be handled by elaborate_stmts")
        }
    }
}

fn elaborate_lvalue(value: CLLValue) -> LLValue {
    let location = value.location;
    let data = match value.data {
        CLValue::Ident(name) => LValue::Ident(name),
        CLValue::Field { base, field } => LValue::Field {
            base: Box::new(elaborate_lvalue(*base)),
            field: loc(field.data, field.location),
        },
        CLValue::Arrow { base, field } => {
            let deref_location = base.location.clone();
            LValue::Field {
                base: Box::new(loc(
                    LValue::Deref(Box::new(elaborate_expr(*base))),
                    deref_location,
                )),
                field: loc(field.data, field.location),
            }
        }
        CLValue::Deref(value) => LValue::Deref(Box::new(elaborate_expr(*value))),
        CLValue::Index { base, index } => LValue::Index {
            base: Box::new(elaborate_lvalue(*base)),
            index: Box::new(elaborate_expr(*index)),
        },
    };
    loc(data, location)
}

fn elaborate_assign(name: CLLValue, asnop: CLAsnOp, value: CLExpr, src_loc: Location) -> LStmt {
    let name = elaborate_lvalue(name);
    let value = elaborate_expr(value);

    if !matches!(&name.data, LValue::Ident(_)) && asnop.data != CAsnOp::Eq {
        return loc(
            Stmt::CompoundAssign {
                name,
                op: loc(asnop.data, asnop.location),
                value,
            },
            src_loc,
        );
    }

    let lhs = loc(Expr::LValue(name.clone()), name.location.clone());
    let op_location = asnop.location.clone();

    // ident += y -> ident = x + y
    let rhs = match asn_to_binop(asnop.data) {
        Some(op) => bin(loc(op, op_location), lhs, value, src_loc.clone()),
        None => value,
    };

    loc(Stmt::Assign { name, value: rhs }, src_loc)
}

fn asn_to_binop(op: CAsnOp) -> Option<BinOp> {
    match op {
        CAsnOp::Eq => None,
        CAsnOp::PlusEq => Some(BinOp::Plus),
        CAsnOp::MinusEq => Some(BinOp::Minus),
        CAsnOp::TimesEq => Some(BinOp::Times),
        CAsnOp::DivEq => Some(BinOp::Div),
        CAsnOp::ModEq => Some(BinOp::Mod),
        CAsnOp::AndEq => Some(BinOp::BitAnd),
        CAsnOp::XorEq => Some(BinOp::BitXor),
        CAsnOp::OrEq => Some(BinOp::BitOr),
        CAsnOp::LShiftEq => Some(BinOp::LShift),
        CAsnOp::RShiftEq => Some(BinOp::RShift),
    }
}

fn elaborate_post(name: CLLValue, postop: LPostOp, src_loc: Location) -> LStmt {
    let name = elaborate_lvalue(name);

    // similar situation to x += y, but for postfixes like x++ (x = x + 1) or x-- (x = x - 1)
    // we have to specially handle if x is not an identitifer
    if !matches!(&name.data, LValue::Ident(_)) {
        return loc(Stmt::Postfix { name, op: postop }, src_loc);
    }

    let op_location = postop.location;
    // x++ -> x = x + 1;
    let op = match postop.data {
        CPostOp::DoublePlus => BinOp::Plus,
        CPostOp::DoubleMinus => BinOp::Minus,
    };

    let ident = loc(Expr::LValue(name.clone()), name.location.clone());
    let one = loc(Expr::Int(1), src_loc.clone());
    let value = bin(loc(op, op_location), ident, one, src_loc.clone());

    loc(Stmt::Assign { name, value }, src_loc)
}

fn elaborate_for(
    init: Option<LSimp>,
    cond: CLExpr,
    step: Option<LSimp>,
    body: CLBlock,
    src_loc: Location,
) -> Vec<LStmt> {
    // for (init; cond; step) { body }
    let cond = elaborate_expr(cond);
    let step = step.map(|step| Box::new(elaborate_simp(step)));
    let body = elaborate_block(body);
    let make_for = |init| {
        loc(
            Stmt::For {
                init,
                cond: cond.clone(),
                step: step.clone(),
                body: body.clone(),
            },
            src_loc.clone(),
        )
    };

    match init {
        // for (; ...)
        None => vec![make_for(None)],
        // for (int i; ...) OR for (int i = ...; ...)
        Some(Located {
            data: CSimp::Decl(decl),
            ..
        }) => {
            match decl.data {
                // variables declared in a for loop have its own scope
                // declare i outside and place for body inside
                // for (int i = 0; ...) ...
                // decl (i,
                //   for_body
                // )
                //
                // this particular branch should always through an uninitialized variable error
                // during analysis
                // for (int i; ...) {}
                CDecl::Decl { typ, name } => {
                    let for_stmt = make_for(None);
                    vec![loc(
                        Stmt::Decl {
                            typ: elaborate_typ(typ),
                            name: loc(name.data, name.location),
                            block: loc(Block(vec![for_stmt]), decl.location.clone()),
                        },
                        src_loc,
                    )]
                }
                // for (int i = ...; ...)
                CDecl::Init { typ, name, value } => {
                    let assign = loc(
                        Stmt::Assign {
                            name: loc(LValue::Ident(name.clone()), name.location.clone()),
                            value: elaborate_expr(value),
                        },
                        src_loc.clone(),
                    );
                    let for_stmt = make_for(None);
                    vec![loc(
                        Stmt::Decl {
                            typ: elaborate_typ(typ),
                            name: loc(name.data, name.location),
                            block: loc(Block(vec![assign, for_stmt]), decl.location.clone()),
                        },
                        src_loc,
                    )]
                }
            }
        }
        // for (i = ...; ...)
        Some(init) => vec![make_for(Some(Box::new(elaborate_simp(init))))],
    }
}

fn elaborate_control(ctrl: LControl) -> Vec<LStmt> {
    let src_loc = ctrl.location;

    match ctrl.data {
        Control::If {
            cond,
            true_block,
            false_block,
        } => vec![loc(
            Stmt::If {
                cond: elaborate_expr(cond),
                true_block: elaborate_block(true_block),
                false_block: false_block.map(elaborate_block),
            },
            src_loc,
        )],
        Control::While { cond, body } => vec![loc(
            Stmt::While {
                cond: elaborate_expr(cond),
                body: elaborate_block(body),
            },
            src_loc,
        )],
        Control::For {
            init,
            cond,
            step,
            body,
        } => elaborate_for(init, cond, step, body, src_loc),
        Control::Return(expr) => vec![loc(Stmt::Return(expr.map(elaborate_expr)), src_loc)],
        Control::Break => vec![loc(Stmt::Break, src_loc)],
        Control::Continue => vec![loc(Stmt::Continue, src_loc)],
        Control::Assert(expr) => vec![loc(Stmt::Assert(elaborate_expr(expr)), src_loc)],
    }
}

fn elaborate_typedef(typ: CLTyp, alias: CLIdent) -> GlobalDecl {
    GlobalDecl::Typedef {
        typ: elaborate_typ(typ),
        alias: loc(alias.data, alias.location),
    }
}

fn elaborate_fundef(
    ret_typ: LRetTyp,
    name: CLIdent,
    params: Vec<CLParam>,
    body: Option<CLBlock>,
) -> GlobalDecl {
    let eparams = params
        .into_iter()
        .map(|p| {
            loc(
                Param {
                    typ: elaborate_typ(p.data.typ),
                    name: loc(p.data.name.data, p.data.name.location),
                },
                p.location,
            )
        })
        .collect();

    GlobalDecl::FunDef {
        ret_typ: elaborate_ret_typ(ret_typ),
        name: loc(name.data, name.location),
        params: eparams,
        body: body.map(elaborate_block),
    }
}

fn elaborate_structdef(name: CLIdent, body: Option<Vec<CLStructField>>) -> GlobalDecl {
    let ebody: Option<Vec<LStructField>> = body.map(|b| {
        b.into_iter()
            .map(|f| {
                loc(
                    StructField {
                        typ: elaborate_typ(f.data.typ),
                        name: loc(f.data.name.data, f.data.name.location),
                    },
                    f.location,
                )
            })
            .collect()
    });

    GlobalDecl::StructDef {
        name: loc(name.data, name.location),
        body: ebody,
    }
}

fn elaborate_gdecl(gdecl: CLGlobalDecl) -> LGlobalDecl {
    let location = gdecl.location;
    let data = match gdecl.data {
        CGlobalDecl::Typedef { typ, alias } => elaborate_typedef(typ, alias),
        CGlobalDecl::FunDef {
            ret_typ,
            name,
            params,
            body,
        } => elaborate_fundef(ret_typ, name, params, body),
        CGlobalDecl::StructDef { name, body } => elaborate_structdef(name, body),
    };

    loc(data, location)
}

pub fn elaborate(program: CProgram) -> LProgram {
    loc(
        Program(program.data.0.into_iter().map(elaborate_gdecl).collect()),
        program.location,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::Lexer, parser::Parser};

    fn elaborate_source(source: &str) -> LProgram {
        let tokens = Lexer::new(source.as_bytes()).lex();
        let mut parser = Parser::new(&tokens);
        let program = parser.parse();
        assert!(
            parser.errors().is_none(),
            "parser errors: {:?}",
            parser.errors()
        );
        elaborate(program)
    }

    #[test]
    fn test_ast() {
        let source = r#"
            int main(int x, bool flag) {
                int result = flag && x > 0 ? x : 0;
                if (result != 0) {
                    result += 1;
                } else {
                    result = 1;
                }
                return result;
            }
        "#;

        let ast = elaborate_source(source);
        insta::assert_snapshot!(format!("{ast}"));
    }
}
