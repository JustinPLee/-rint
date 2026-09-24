// linear IR but with parameters and return values of functions made explicit

use std::fmt;

use crate::ast::{LIdent, LTyp};
use crate::ir_linear;
pub use crate::ir_linear::{LPseudoOp, PseudoOp};
use crate::location::{Located, loc};
use crate::utils::{Label, Temp};
use crate::x86::{ARG_REGS, RETURN_REG, Register}; // messy abstraction layers but its ok for now

#[derive(Clone, Copy, Debug, Hash, PartialOrd, Ord, Eq, PartialEq)]
pub enum FunctionSlot {
    Arg(usize),
    ReturnValue,
}

impl FunctionSlot {
    pub fn register(self) -> Option<Register> {
        match self {
            Self::Arg(index) => ARG_REGS.get(index).copied(),
            Self::ReturnValue => Some(RETURN_REG),
        }
    }
}

pub fn abstract_name(register: Register) -> &'static str {
    match register {
        Register::Rax => "res0",
        Register::Rdi => "arg1",
        Register::Rsi => "arg2",
        Register::Rdx => "arg3",
        Register::Rcx => "arg4",
        Register::R8 => "arg5",
        Register::R9 => "arg6",
        Register::R10 => "ler7",
        Register::R11 => "ler8",
        Register::Rbx => "cal9",
        Register::Rbp => "cal10",
        Register::R12 => "cal11",
        Register::R13 => "cal12",
        Register::R14 => "cal13",
        Register::R15 => "cal14",
        Register::Rsp => "rsp",
    }
}

#[derive(Clone, Debug, Hash, PartialOrd, Ord, Eq, PartialEq)]
pub enum Operand {
    Imm(i32),
    Temp(Temp),
    Register(Register),
    StackArgIndex(usize),
    FunctionSlot(FunctionSlot),
}

impl Operand {
    pub fn has_name(&self) -> bool {
        matches!(
            self,
            Self::Temp(_) | Self::Register(_) | Self::FunctionSlot(_)
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Instr {
    Move {
        dest: LOperand,
        src: LOperand,
    },
    BinOp {
        dest: LOperand,
        lhs: LOperand,
        op: LPseudoOp,
        rhs: LOperand,
    },
    Return,
    Call {
        callee: LIdent,
        arg_count: usize,
    },
    Push(Register),
    Pop(Register),
    Abort,
    Jump(Label),
    CJump {
        lhs: LOperand,
        op: LPseudoOp,
        rhs: LOperand,
        true_target: Label,
        false_target: Label,
    },
    Label(Label),
}

pub type LOperand = Located<Operand>;
pub type LInstr = Located<Instr>;

#[derive(Debug)]
pub struct Program(pub Vec<LInstr>);

#[derive(Debug)]
pub struct Function {
    pub name: LIdent,
    pub params: Vec<Temp>,
    pub ret_type: LTyp,
    pub body: Program,
}

#[derive(Debug)]
pub struct Module {
    pub functions: Vec<Function>,
}

fn trans_oper(oper: &ir_linear::LOperand) -> LOperand {
    loc(
        match oper.data {
            ir_linear::Operand::Imm(n) => Operand::Imm(n),
            ir_linear::Operand::Temp(t) => Operand::Temp(t),
        },
        oper.location.clone(),
    )
}

// Linear IR already makes body-internal fallthrough explicit. Parameter copies
// are prepended here, so add the entry edge when the body starts with a label.
fn add_entry_jump_after_params(mut instrs: Vec<LInstr>, param_count: usize) -> Vec<LInstr> {
    if param_count == 0 {
        return instrs;
    }

    let entry_label = instrs.get(param_count).and_then(|instr| match &instr.data {
        Instr::Label(label) => Some((*label, instr.location.clone())),
        _ => None,
    });

    if let Some((label, location)) = entry_label {
        instrs.insert(param_count, loc(Instr::Jump(label), location));
    }

    instrs
}

fn trans_function(function: &ir_linear::Function) -> Function {
    let location = function
        .body
        .0
        .first()
        .map(|instr| instr.location.clone())
        .unwrap_or_else(|| function.name.location.clone());
    let mut body = Vec::new();
    let mut emit = |data, location| body.push(loc(data, location));

    for (i, &param) in function.params.iter().enumerate() {
        emit(
            Instr::Move {
                dest: loc(Operand::Temp(param), location.clone()),
                src: loc(
                    Operand::FunctionSlot(FunctionSlot::Arg(i)),
                    location.clone(),
                ),
            },
            location.clone(),
        );
    }

    for instr in &function.body.0 {
        let src_loc = instr.location.clone();
        match &instr.data {
            ir_linear::Instr::Move { dest, src } => emit(
                Instr::Move {
                    dest: trans_oper(dest),
                    src: trans_oper(src),
                },
                src_loc,
            ),
            ir_linear::Instr::BinOp { dest, lhs, op, rhs } => emit(
                Instr::BinOp {
                    dest: trans_oper(dest),
                    lhs: trans_oper(lhs),
                    op: op.clone(),
                    rhs: trans_oper(rhs),
                },
                src_loc,
            ),
            ir_linear::Instr::Call { dest, callee, args } => {
                // r <- call f(a,b,c)
                //
                // t0 <- arg0
                // t1 <- arg1
                // t2 <- arg2
                // call f
                // t3 <- res0
                for (i, arg) in args.iter().enumerate() {
                    emit(
                        Instr::Move {
                            dest: loc(Operand::FunctionSlot(FunctionSlot::Arg(i)), src_loc.clone()),
                            src: trans_oper(arg),
                        },
                        src_loc.clone(),
                    );
                }

                emit(
                    Instr::Call {
                        callee: callee.clone(),
                        arg_count: args.len(),
                    },
                    src_loc.clone(),
                );

                if let Some(dest) = dest {
                    emit(
                        Instr::Move {
                            dest: loc(Operand::Temp(*dest), src_loc.clone()),
                            src: loc(
                                Operand::FunctionSlot(FunctionSlot::ReturnValue),
                                src_loc.clone(),
                            ),
                        },
                        src_loc,
                    );
                }
            }
            ir_linear::Instr::Return(value) => {
                // res0 <- t0
                // ret
                //
                // or
                //
                // ret
                if let Some(value) = value {
                    emit(
                        Instr::Move {
                            dest: loc(
                                Operand::FunctionSlot(FunctionSlot::ReturnValue),
                                src_loc.clone(),
                            ),
                            src: trans_oper(value),
                        },
                        src_loc.clone(),
                    );
                }
                emit(Instr::Return, src_loc);
            }
            ir_linear::Instr::Abort => emit(Instr::Abort, src_loc),
            ir_linear::Instr::Jump(label) => emit(Instr::Jump(*label), src_loc),
            ir_linear::Instr::CJump {
                lhs,
                op,
                rhs,
                true_target,
                false_target,
            } => emit(
                Instr::CJump {
                    lhs: trans_oper(lhs),
                    op: op.clone(),
                    rhs: trans_oper(rhs),
                    true_target: *true_target,
                    false_target: *false_target,
                },
                src_loc,
            ),
            ir_linear::Instr::Label(label) => emit(Instr::Label(*label), src_loc),
        }
    }

    Function {
        name: function.name.clone(),
        params: function.params.clone(),
        ret_type: function.ret_type.clone(),
        body: Program(add_entry_jump_after_params(body, function.params.len())),
    }
}

pub fn translate(module: &ir_linear::Module) -> Module {
    Module {
        functions: module.functions.iter().map(trans_function).collect(),
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Imm(n) => write!(f, "{n}"),
            Self::Temp(t) => write!(f, "{t}"),
            Self::Register(r) => f.write_str(abstract_name(*r)),
            Self::StackArgIndex(i) => write!(f, "stack_arg{i}"),
            Self::FunctionSlot(a) => write!(f, "{a}"),
        }
    }
}

impl fmt::Display for FunctionSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arg(index) => write!(f, "arg{}", index + 1),
            Self::ReturnValue => write!(f, "res0"),
        }
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Move { dest, src } => write!(f, "{dest} ← {src}"),
            Self::BinOp { dest, lhs, op, rhs } => write!(f, "{dest} ← {lhs} {op} {rhs}"),
            Self::Return => f.write_str("return"),
            Self::Call { callee, .. } => write!(f, "call {callee}"),
            Self::Push(r) => write!(f, "push {}", abstract_name(*r)),
            Self::Pop(r) => write!(f, "pop {}", abstract_name(*r)),
            Self::Abort => f.write_str("abort"),
            Self::Jump(label) => write!(f, "jump {label}"),
            Self::CJump {
                lhs,
                op,
                rhs,
                true_target,
                false_target,
            } => write!(
                f,
                "if {lhs} {op} {rhs} jump {true_target} else jump {false_target}"
            ),
            Self::Label(label) => write!(f, "{label}:"),
        }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "program:")?;
        for instr in &self.0 {
            writeln!(f, "    {}", instr.data)?;
        }
        Ok(())
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fn {}(", self.name)?;
        for (index, param) in self.params.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        writeln!(f, "):")?;
        for instr in &self.body.0 {
            if matches!(&instr.data, Instr::Label(_)) {
                writeln!(f, "{}", instr.data)?;
            } else {
                writeln!(f, "    {}", instr.data)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, function) in self.functions.iter().enumerate() {
            write!(f, "{}", function)?;
            if index + 1 < self.functions.len() {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}
