// linear IR but with parameters and return values of functions made explicit

use std::fmt;

use crate::ast::LIdent;
use crate::ir_linear;
use crate::ir_linear::ValueWidth;
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
        // caller
        Register::R10 => "ler7",
        Register::R11 => "ler8",
        // callee
        Register::Rbx => "lee9",
        Register::Rbp => "lee10",
        Register::R12 => "lee11",
        Register::R13 => "lee12",
        Register::R14 => "lee13",
        Register::R15 => "lee14",
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
        width: ValueWidth,
    },
    BinOp {
        dest: LOperand,
        lhs: LOperand,
        op: LPseudoOp,
        rhs: LOperand,
        width: ValueWidth,
    },
    Load {
        dest: LOperand,
        address: LOperand,
        width: ValueWidth,
    },
    Store {
        address: LOperand,
        src: LOperand,
        width: ValueWidth,
    },
    Address {
        dest: LOperand,
        base: LOperand,
        index: Option<LOperand>,
        scale: u8,
        displacement: i32,
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
        width: ValueWidth,
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

fn trans_function(function: &ir_linear::Function) -> Function {
    let location = function
        .body
        .0
        .first()
        .map(|instr| instr.location.clone())
        .unwrap_or_else(|| function.name.location.clone());
    let mut body = Vec::new();

    for (i, &param) in function.params.iter().enumerate() {
        let (dest, src) = (
            loc(Operand::Temp(param), location.clone()),
            loc(
                Operand::FunctionSlot(FunctionSlot::Arg(i)),
                location.clone(),
            ),
        );
        body.push(loc(
            Instr::Move {
                dest,
                src,
                width: function.param_widths[i],
            },
            location.clone(),
        ));
    }

    for instr in &function.body.0 {
        let src_loc = instr.location.clone();
        match &instr.data {
            ir_linear::Instr::Move { dest, src, width } => body.push(loc(
                Instr::Move {
                    dest: trans_oper(dest),
                    src: trans_oper(src),
                    width: *width,
                },
                src_loc,
            )),
            ir_linear::Instr::BinOp {
                dest,
                lhs,
                op,
                rhs,
                width,
            } => body.push(loc(
                Instr::BinOp {
                    dest: trans_oper(dest),
                    lhs: trans_oper(lhs),
                    op: op.clone(),
                    rhs: trans_oper(rhs),
                    width: *width,
                },
                src_loc,
            )),
            ir_linear::Instr::Load {
                dest,
                address,
                width,
            } => body.push(loc(
                Instr::Load {
                    dest: trans_oper(dest),
                    address: trans_oper(address),
                    width: *width,
                },
                src_loc,
            )),
            ir_linear::Instr::Store {
                address,
                src,
                width,
            } => body.push(loc(
                Instr::Store {
                    address: trans_oper(address),
                    src: trans_oper(src),
                    width: *width,
                },
                src_loc,
            )),
            ir_linear::Instr::Address {
                dest,
                base,
                index,
                scale,
                displacement,
            } => body.push(loc(
                Instr::Address {
                    dest: trans_oper(dest),
                    base: trans_oper(base),
                    index: index.as_ref().map(trans_oper),
                    scale: *scale,
                    displacement: *displacement,
                },
                src_loc,
            )),
            ir_linear::Instr::Call {
                dest,
                dest_width,
                callee,
                args,
                arg_widths,
            } => {
                // r <- call f(a,b,c)
                //
                // t0 <- arg0
                // t1 <- arg1
                // t2 <- arg2
                // call f
                // t3 <- res0
                for (i, arg) in args.iter().enumerate() {
                    let (dest, src) = (
                        loc(Operand::FunctionSlot(FunctionSlot::Arg(i)), src_loc.clone()),
                        trans_oper(arg),
                    );
                    body.push(loc(
                        Instr::Move {
                            dest,
                            src,
                            width: arg_widths[i],
                        },
                        src_loc.clone(),
                    ));
                }

                body.push(loc(
                    Instr::Call {
                        callee: callee.clone(),
                        arg_count: args.len(),
                    },
                    src_loc.clone(),
                ));

                if let Some(dest) = dest {
                    let (dest, src) = (
                        loc(Operand::Temp(*dest), src_loc.clone()),
                        loc(
                            Operand::FunctionSlot(FunctionSlot::ReturnValue),
                            src_loc.clone(),
                        ),
                    );
                    body.push(loc(
                        Instr::Move {
                            dest,
                            src,
                            width: dest_width.expect("call result has a width"),
                        },
                        src_loc,
                    ));
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
                    let (dest, src) = (
                        loc(
                            Operand::FunctionSlot(FunctionSlot::ReturnValue),
                            src_loc.clone(),
                        ),
                        trans_oper(value),
                    );
                    body.push(loc(
                        Instr::Move {
                            dest,
                            src,
                            width: function.ret_width,
                        },
                        src_loc.clone(),
                    ));
                }
                body.push(loc(Instr::Return, src_loc));
            }
            ir_linear::Instr::Abort => body.push(loc(Instr::Abort, src_loc)),
            ir_linear::Instr::Jump(label) => body.push(loc(Instr::Jump(*label), src_loc)),
            ir_linear::Instr::CJump {
                lhs,
                op,
                rhs,
                true_target,
                false_target,
                width,
            } => body.push(loc(
                Instr::CJump {
                    lhs: trans_oper(lhs),
                    op: op.clone(),
                    rhs: trans_oper(rhs),
                    true_target: *true_target,
                    false_target: *false_target,
                    width: *width,
                },
                src_loc,
            )),
            ir_linear::Instr::Label(label) => body.push(loc(Instr::Label(*label), src_loc)),
        }
    }

    Function {
        name: function.name.clone(),
        params: function.params.clone(),
        body: Program(body),
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
            Self::Move { dest, src, width } => {
                let suffix = if *width == ValueWidth::Qword { "q" } else { "" };
                write!(f, "{dest} ←{suffix} {src}")
            }
            Self::BinOp {
                dest,
                lhs,
                op,
                rhs,
                width,
            } => {
                let suffix = if *width == ValueWidth::Qword { "q" } else { "" };
                write!(f, "{dest} ←{suffix} {lhs} {op} {rhs}")
            }
            Self::Load {
                dest,
                address,
                width,
            } => write!(f, "{dest} ← load({width:?}) {address}"),
            Self::Store {
                address,
                src,
                width,
            } => write!(f, "store({width:?}) {address} ← {src}"),
            Self::Address {
                dest,
                base,
                index,
                scale,
                displacement,
            } => {
                write!(f, "{dest} ← address({base}")?;
                if let Some(index) = index {
                    write!(f, " + {index} * {scale}")?;
                }
                if *displacement != 0 {
                    write!(f, " + {displacement}")?;
                }
                write!(f, ")")
            }
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
                width,
            } => {
                let suffix = if *width == ValueWidth::Qword { "q" } else { "" };
                write!(
                    f,
                    "if{suffix} {lhs} {op} {rhs} jump {true_target} else jump {false_target}"
                )
            }
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
