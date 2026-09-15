use crate::ir_ast_3ac::{BinOp, Expr, Program as AProgram, Stmt};
use crate::registers::{Register, Width};
use crate::temps::{Label, Temp, TempGen};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Operand {
    Temp(Temp),
    Imm(i32),
    Reg(Register),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PseudoOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    // LShift,
    // RShift,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    EqualEq,
    NotEq,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instr {
    Move {
        dest: Operand,
        src: Operand,
    },
    BinOp {
        op: PseudoOp,
        dest: Operand,
        lhs: Operand,
        rhs: Operand,
    },
    Jump(Label),
    CJump {
        lhs: Operand,
        op: PseudoOp,
        rhs: Operand,
        target: Label,
    },
    Ret(Operand),
    Label(Label),
    Directive(String),
    Comment(String),
}

pub struct Program(pub Vec<Instr>);

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "program:");
        for instr in &self.0 {
            writeln!(f, "{}", instr);
        }
        Ok(())
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Temp(temp) => write!(f, "{temp}"),
            Operand::Imm(value) => write!(f, "{value}"),
            Operand::Reg(register) => write!(f, "{}", register.name(Width::Qword)),
        }
    }
}

impl fmt::Display for PseudoOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            PseudoOp::Add => "add",
            PseudoOp::Sub => "sub",
            PseudoOp::Mul => "mul",
            PseudoOp::Div => "div",
            PseudoOp::Mod => "mod",
            PseudoOp::And => "and",
            PseudoOp::Or => "or",
            PseudoOp::BitAnd => "bitand",
            PseudoOp::BitOr => "bitor",
            PseudoOp::BitXor => "bitxor",
            // PseudoOp::LShift => todo!(),
            // PseudoOp::RShift => todo!(),
            PseudoOp::Less => "<",
            PseudoOp::LessEq => "<=",
            PseudoOp::Greater => ">",
            PseudoOp::GreaterEq => ">=",
            PseudoOp::EqualEq => "==",
            PseudoOp::NotEq => "!=",
        };
        write!(f, "{}", op)
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instr::Move { dest, src } => {
                write!(f, "mov {dest}, {src}")
            }
            Instr::BinOp { op, dest, lhs, rhs } => {
                write!(f, "{op} {dest}, {lhs}, {rhs}")
            }
            Instr::Jump(label) => {
                write!(f, "goto {label}")
            }
            Instr::CJump {
                lhs,
                op,
                rhs,
                target,
            } => {
                write!(f, "if {lhs} {op} {rhs} goto {target}")
            }
            Instr::Ret(exp) => {
                write!(f, "return {exp}")
            }
            Instr::Label(label) => {
                write!(f, "{label}:")
            }
            Instr::Directive(directive) => {
                write!(f, "{directive}")
            }
            Instr::Comment(comment) => {
                write!(f, "# {comment}")
            }
        }
    }
}

fn munch_op(op: &BinOp) -> PseudoOp {
    match op {
        BinOp::Plus => PseudoOp::Add,
        BinOp::Minus => PseudoOp::Sub,
        BinOp::Times => PseudoOp::Mul,
        BinOp::Div => PseudoOp::Div,
        BinOp::Mod => PseudoOp::Mod,

        BinOp::LogicAnd => PseudoOp::And,
        BinOp::LogicOr => PseudoOp::Or,

        BinOp::BitAnd => PseudoOp::BitAnd,
        BinOp::BitOr => PseudoOp::BitOr,
        BinOp::BitXor => PseudoOp::BitXor,
        BinOp::LShift => todo!(),
        BinOp::RShift => todo!(),
        BinOp::Greater => PseudoOp::Greater,
        BinOp::GreaterEq => PseudoOp::GreaterEq,
        BinOp::Less => PseudoOp::Less,
        BinOp::LessEq => PseudoOp::LessEq,
        BinOp::EqualEq => PseudoOp::EqualEq,
        BinOp::NotEq => PseudoOp::NotEq,
    }
}

// straightforward translation
fn munch_expr(dest: Operand, expr: &Expr, temps: &mut TempGen) -> Vec<Instr> {
    match expr {
        Expr::Int(value) => {
            vec![Instr::Move {
                dest,
                src: Operand::Imm(*value),
            }]
        }
        Expr::Temp(temp) => {
            vec![Instr::Move {
                dest,
                src: Operand::Temp(*temp),
            }]
        }
        Expr::BinOp { lhs, op, rhs } => munch_binop(dest, op.clone(), lhs, rhs, temps),
    }
}

// ex:
// t0 = 2 + 3
// |
// v
// t0 = 2
// t1 = 3
// t0 = t0 + t1
fn munch_binop(
    dest: Operand,
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
    temps: &mut TempGen,
) -> Vec<Instr> {
    let lhs_temp = temps.fresh();
    let rhs_temp = temps.fresh();

    let mut result = Vec::new();
    result.extend(munch_expr(Operand::Temp(lhs_temp), lhs, temps));
    result.extend(munch_expr(Operand::Temp(rhs_temp), rhs, temps));
    result.push(Instr::BinOp {
        op: munch_op(&op),
        dest,
        lhs: Operand::Temp(lhs_temp),
        rhs: Operand::Temp(rhs_temp),
    });

    result
}

// straightforward
pub fn munch_stmt(stmt: &Stmt, temps: &mut TempGen) -> Vec<Instr> {
    match stmt {
        Stmt::Move { dest, src } => munch_expr(Operand::Temp(*dest), src, temps),
        Stmt::Return(expr) => {
            // rax is special
            let return_temp = Temp::new(-1);
            munch_expr(Operand::Temp(return_temp), expr, temps)
        }
        Stmt::Jump(label) => vec![Instr::Jump(*label)],
        Stmt::CJump {
            lhs,
            op,
            rhs,
            target,
        } => {
            let lhs_temp = Operand::Temp(temps.fresh());
            let rhs_temp = Operand::Temp(temps.fresh());

            let mut result = Vec::new();
            result.extend(munch_expr(lhs_temp, lhs, temps));
            result.extend(munch_expr(rhs_temp, rhs, temps));

            result.push(Instr::CJump {
                lhs: lhs_temp,
                op: munch_op(op),
                rhs: rhs_temp,
                target: *target,
            });

            result
        }
        Stmt::Label(label) => vec![Instr::Label(*label)],
    }
}

pub fn codegen(program: &AProgram, temps: &mut TempGen) -> Program {
    Program(
        program
            .0
            .iter()
            .map(|s| munch_stmt(s, temps))
            .flatten()
            .collect(),
    )
}
