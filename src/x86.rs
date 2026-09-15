use crate::{
    codegen::{Instr, Operand, Program, PseudoOp},
    registers::{Allocation, Register, Width},
    temps::Label,
};
use core::fmt;

// at&t syntax
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    pub offset: i32,
    pub base: Register,
}

impl Memory {
    fn asm(self) -> String {
        if self.offset == 0 {
            format!("({})", self.base.name(Width::Qword))
        } else {
            format!("{}({})", self.offset, self.base.name(Width::Qword))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Operand {
    Reg(Register),
    Imm(i32),
    Mem(Memory),
}

impl X86Operand {
    fn asm(self, width: Width) -> String {
        match self {
            Self::Reg(reg) => reg.name(width).to_string(),
            Self::Imm(value) => {
                format!("${value}")
            }
            Self::Mem(mem) => mem.asm(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum X86Instr {
    Mov {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    Add {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    Sub {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    And {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    Or {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    Xor {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    Imul {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    Idiv {
        width: Width,
        src: X86Operand,
    },

    Cdq, // sign extension

    Push(Register),
    Pop(Register),

    // manipulate rsp
    AddStack(i32),
    SubStack(i32),

    Cmp {
        lhs: X86Operand,
        rhs: X86Operand,
        width: Width,
    },
    Jmp(Label),
    Je(Label),
    Jne(Label),
    Jl(Label),
    Jle(Label),
    Jg(Label),
    Jge(Label),
    Label(Label),

    // for comparisons
    Sete {
        dest: Register,
    },
    Setne {
        dest: Register,
    },
    Setl {
        dest: Register,
    },
    Setle {
        dest: Register,
    },
    Setg {
        dest: Register,
    },
    Setge {
        dest: Register,
    },

    // move with zero extend
    Movzx {
        src: Register,
        dest: Register,
    },

    Comment(String),
    Directive(String),

    Ret,
}

impl X86Instr {
    fn base(&self) -> &'static str {
        match self {
            Self::Mov { .. } => "mov",
            Self::Add { .. } => "add",
            Self::Sub { .. } => "sub",
            Self::And { .. } => "and",
            Self::Or { .. } => "or",
            Self::Xor { .. } => "xor",
            Self::Imul { .. } => "imul",
            Self::Idiv { .. } => "idiv",

            Self::Cdq => "cltd",

            Self::Push(_) => "pushq",
            Self::Pop(_) => "popq",

            Self::AddStack(_) => "addq",
            Self::SubStack(_) => "subq",

            Self::Cmp { .. } => "cmp",
            Self::Jmp(_) => "jmp",
            Self::Je(_) => "je",
            Self::Jne(_) => "jne",
            Self::Jl(_) => "jl",
            Self::Jle(_) => "jle",
            Self::Jg(_) => "jg",
            Self::Jge(_) => "jge",

            Self::Label(_) => "",
            Self::Ret => "ret",

            Self::Sete { .. } => "sete",
            Self::Setne { .. } => "setne",
            Self::Setl { .. } => "setl",
            Self::Setle { .. } => "setle",
            Self::Setg { .. } => "setg",
            Self::Setge { .. } => "setge",

            Self::Movzx { src, dest } => unimplemented!(),
            Self::Comment(_) => unimplemented!(),
            Self::Directive(_) => unimplemented!(),
        }
    }
}

impl fmt::Display for X86Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.clone() {
            Self::Mov { width, dest, src }
            | Self::Add { width, dest, src }
            | Self::Sub { width, dest, src }
            | Self::And { width, dest, src }
            | Self::Or { width, dest, src }
            | Self::Xor { width, dest, src }
            | Self::Imul { width, dest, src } => {
                write!(
                    f,
                    "    {}{} {}, {}",
                    self.base(),
                    width.suffix(),
                    src.asm(width),
                    dest.asm(width)
                )
            }
            Self::Idiv { width, src } => {
                write!(
                    f,
                    "    {}{} {}",
                    self.base(),
                    width.suffix(),
                    src.asm(width)
                )
            }
            Self::Cdq => {
                write!(f, "    cltd")
            }
            Self::Push(reg) => {
                write!(f, "    pushq {}", reg.name(Width::Qword))
            }
            Self::Pop(reg) => {
                write!(f, "    popq {}", reg.name(Width::Qword))
            }
            Self::AddStack(amount) => {
                write!(
                    f,
                    "    addq ${amount}, {}",
                    Register::Rsp.name(Width::Qword)
                )
            }
            Self::SubStack(amount) => {
                write!(
                    f,
                    "    subq ${amount}, {}",
                    Register::Rsp.name(Width::Qword)
                )
            }
            Self::Ret => {
                write!(f, "    ret")
            }
            Self::Cmp { lhs, rhs, width } => {
                write!(
                    f,
                    "    cmp{} {}, {}",
                    width.suffix(),
                    lhs.asm(width),
                    rhs.asm(width)
                )
            }
            Self::Jmp(label) => write!(f, "    jmp {label}"),
            Self::Je(label) => write!(f, "    je {label}"),
            Self::Jne(label) => write!(f, "    jne {label}"),
            Self::Jl(label) => write!(f, "    jl {label}"),
            Self::Jle(label) => write!(f, "    jle {label}"),
            Self::Jg(label) => write!(f, "    jg {label}"),
            Self::Jge(label) => write!(f, "    jge {label}"),

            Self::Label(label) => write!(f, "{label}:"),

            Self::Sete { dest }
            | Self::Setne { dest }
            | Self::Setl { dest }
            | Self::Setle { dest }
            | Self::Setg { dest }
            | Self::Setge { dest } => write!(f, "    {} {}", self.base(), dest.name(Width::Byte)),
            Self::Comment(comment) => {
                write!(f, "    # {comment}")
            }
            Self::Directive(directive) => {
                write!(f, "{directive}")
            }
            Self::Movzx { src, dest } => {
                write!(
                    f,
                    "    movzbl {}, {}",
                    src.name(Width::Byte),
                    dest.name(Width::Dword)
                )
            }
        }
    }
}

// give temps a physical location
// either as an immediate or in a register or memory
fn lookup_operand(operand: Operand, allocation: &Allocation) -> X86Operand {
    match operand {
        Operand::Imm(value) => X86Operand::Imm(value),
        Operand::Reg(register) => X86Operand::Reg(register),
        Operand::Temp(temp) => {
            if let Some(&reg) = allocation.registers.get(&temp) {
                return X86Operand::Reg(reg);
            }

            if let Some(Register::Spill(slot)) = allocation.spills.get(&temp) {
                return X86Operand::Mem(Memory {
                    offset: -4 * (*slot as i32 + 1),
                    base: Register::Rbp,
                });
            }

            // special regs
            X86Operand::Reg(Register::from_temp(temp))
        }
    }
}

// reconfigure invalid mem to mem operations
fn move_value(dest: X86Operand, src: X86Operand, width: Width, scratch: Register) -> Vec<X86Instr> {
    if dest == src {
        return Vec::new();
    }

    match (dest, src) {
        (X86Operand::Mem(dst), X86Operand::Mem(src)) => {
            vec![
                X86Instr::Mov {
                    width,
                    dest: X86Operand::Reg(scratch),
                    src: X86Operand::Mem(src),
                },
                X86Instr::Mov {
                    width,
                    dest: X86Operand::Mem(dst),
                    src: X86Operand::Reg(scratch),
                },
            ]
        }
        (dest, src) => {
            vec![X86Instr::Mov { width, dest, src }]
        }
    }
}

fn apply_op(op: PseudoOp, width: Width, dest: X86Operand, src: X86Operand) -> X86Instr {
    match op {
        PseudoOp::Add => X86Instr::Add { width, dest, src },
        PseudoOp::Sub => X86Instr::Sub { width, dest, src },
        PseudoOp::And | PseudoOp::BitAnd => X86Instr::And { width, dest, src },
        PseudoOp::Or | PseudoOp::BitOr => X86Instr::Or { width, dest, src },
        PseudoOp::BitXor => X86Instr::Xor { width, dest, src },
        _ => panic!("todo"),
    }
}

fn simple_binop(
    op: PseudoOp,
    dest: X86Operand,
    lhs: X86Operand,
    rhs: X86Operand,
    scratch: Register,
    width: Width,
) -> Vec<X86Instr> {
    let scratch_op = X86Operand::Reg(scratch);

    // ex:
    // r1 = r2 + r1
    // mov r2, r1
    // add r1, r1
    // is incorrect
    //
    // r1 = r2 + r1
    // mov r1, %s
    // add r2, %s
    // mov %s, r1
    // is correct
    if dest == rhs || matches!(dest, X86Operand::Mem(_)) {
        let mut result = move_value(scratch_op, lhs, width, scratch);
        result.push(apply_op(op, width, scratch_op, rhs));
        result.extend(move_value(dest, scratch_op, width, scratch));
        result
    } else {
        let mut result = move_value(dest, lhs, width, scratch);
        result.push(apply_op(op, width, dest, rhs));
        result
    }
}

fn binary_op(
    op: PseudoOp,
    dest: X86Operand,
    lhs: X86Operand,
    rhs: X86Operand,
    scratch: Register,
) -> Vec<X86Instr> {
    const WIDTH: Width = Width::Dword;
    match op {
        PseudoOp::Add
        | PseudoOp::Sub
        | PseudoOp::And
        | PseudoOp::Or
        | PseudoOp::BitAnd
        | PseudoOp::BitOr
        | PseudoOp::BitXor => simple_binop(op, dest, lhs, rhs, scratch, WIDTH),
        PseudoOp::Mul => {
            let mut result = Vec::new();
            result.extend(move_value(
                X86Operand::Reg(Register::Rax),
                lhs,
                WIDTH,
                scratch,
            ));
            result.push(X86Instr::Imul {
                width: WIDTH,
                dest: X86Operand::Reg(Register::Rax),
                src: rhs,
            });
            result.extend(move_value(
                dest,
                X86Operand::Reg(Register::Rax),
                WIDTH,
                scratch,
            ));

            result
        }
        PseudoOp::Div | PseudoOp::Mod => {
            let mut result = Vec::new();
            // eax holds dividend
            result.extend(move_value(
                X86Operand::Reg(Register::Rax),
                lhs,
                WIDTH,
                scratch,
            ));

            // scratch holds divisor
            result.extend(move_value(X86Operand::Reg(scratch), rhs, WIDTH, scratch));
            // edx:eax
            result.push(X86Instr::Cdq); // sign extend
            result.push(X86Instr::Idiv {
                width: WIDTH,
                src: X86Operand::Reg(scratch),
            });

            // quotient in eax
            // remainder in edx
            let result_register = match op {
                PseudoOp::Div => Register::Rax,
                PseudoOp::Mod => Register::Rdx,
                _ => unreachable!(),
            };

            result.extend(move_value(
                dest,
                X86Operand::Reg(result_register),
                WIDTH,
                scratch,
            ));

            result
        }
        PseudoOp::Less
        | PseudoOp::LessEq
        | PseudoOp::Greater
        | PseudoOp::GreaterEq
        | PseudoOp::EqualEq
        | PseudoOp::NotEq => comparison_op(op, dest, lhs, rhs, scratch),
    }
}

fn comparison_op(
    op: PseudoOp,
    dest: X86Operand,
    lhs: X86Operand,
    rhs: X86Operand,
    scratch: Register,
) -> Vec<X86Instr> {
    let scratch_op = X86Operand::Reg(scratch);
    // use cmp
    // move it to scratch
    // use movz to extend it
    // move from scratch to dest
    let mut result = Vec::new();

    result.extend(move_value(scratch_op, lhs, Width::Dword, scratch));
    result.push(X86Instr::Cmp {
        width: Width::Dword,
        lhs: rhs,
        rhs: scratch_op,
    });

    let instr = match op {
        PseudoOp::Less => X86Instr::Setl { dest: scratch },
        PseudoOp::LessEq => X86Instr::Setle { dest: scratch },
        PseudoOp::Greater => X86Instr::Setg { dest: scratch },
        PseudoOp::GreaterEq => X86Instr::Setge { dest: scratch },
        PseudoOp::EqualEq => X86Instr::Sete { dest: scratch },
        PseudoOp::NotEq => X86Instr::Setne { dest: scratch },
        _ => panic!("bad comparison op"),
    };

    result.push(instr);
    result.push(X86Instr::Movzx {
        src: scratch,
        dest: scratch,
    });
    result.extend(move_value(dest, scratch_op, Width::Dword, scratch));

    result
}

fn align(bytes: u32, alignment: u32) -> u32 {
    assert!(alignment > 0);
    ((bytes + (alignment - 1)) / alignment) * alignment
}

// convert IR to x86
pub fn lower(instrs: &[Instr], allocation: &Allocation) -> Vec<X86Instr> {
    const VALUE_WIDTH: Width = Width::Dword;
    let scratch = Register::R15;

    let n_spill_slots = allocation
        .spills
        .values()
        .filter_map(|register| match register {
            Register::Spill(slot) => Some(*slot + 1),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    // todo: link 4 to the width
    let stack_size = n_spill_slots * 4 as u32;
    let stack_size = align(stack_size, 16);
    let mut result = Vec::new();

    // prologue
    result.push(X86Instr::Push(Register::Rbp));

    result.push(X86Instr::Mov {
        width: Width::Qword,
        dest: X86Operand::Reg(Register::Rbp),
        src: X86Operand::Reg(Register::Rsp),
    });

    // allocate stack space for spilled variables
    // stack grows downwards
    if stack_size > 0 {
        result.push(X86Instr::SubStack(stack_size as i32));
    }

    for instr in instrs {
        match instr.clone() {
            Instr::Move { dest, src } => {
                let dest = lookup_operand(dest, allocation);
                let src = lookup_operand(src, allocation);
                result.extend(move_value(dest, src, VALUE_WIDTH, scratch));
            }
            Instr::BinOp { op, dest, lhs, rhs } => {
                let dest = lookup_operand(dest, allocation);
                let lhs = lookup_operand(lhs, allocation);
                let rhs = lookup_operand(rhs, allocation);
                result.extend(binary_op(op, dest, lhs, rhs, scratch));
            }
            Instr::Jump(label) => {
                result.push(X86Instr::Jmp(label));
            }
            Instr::CJump {
                lhs,
                op,
                rhs,
                target,
            } => {
                let lhs = lookup_operand(lhs, allocation);
                let rhs = lookup_operand(rhs, allocation);
                result.push(X86Instr::Cmp {
                    width: VALUE_WIDTH,
                    lhs: rhs,
                    rhs: lhs,
                });
                let jump = match op {
                    PseudoOp::Less => X86Instr::Jl(target),
                    PseudoOp::LessEq => X86Instr::Jle(target),
                    PseudoOp::Greater => X86Instr::Jg(target),
                    PseudoOp::GreaterEq => X86Instr::Jge(target),
                    PseudoOp::EqualEq => X86Instr::Je(target),
                    PseudoOp::NotEq => X86Instr::Jne(target),

                    _ => panic!("expected comparison op"),
                };

                result.push(jump);
            }
            Instr::Ret(operand) => {
                let operand = lookup_operand(operand, allocation);

                result.extend(move_value(
                    X86Operand::Reg(Register::Rax),
                    operand,
                    VALUE_WIDTH,
                    scratch,
                ));
            }
            Instr::Label(label) => {
                result.push(X86Instr::Label(label));
            }
            Instr::Comment(comment) => {
                result.push(X86Instr::Comment(comment.clone()));
            }
            Instr::Directive(directive) => {
                result.push(X86Instr::Directive(directive.clone()));
            }
        }
    }

    // epilogue
    if stack_size > 0 {
        result.push(X86Instr::AddStack(stack_size as i32));
    }

    result.push(X86Instr::Pop(Register::Rbp));
    result.push(X86Instr::Ret);

    result
}

pub fn emit_assembly(aasm: &Program, allocation: &Allocation, function_name: &str) -> String {
    let x86 = lower(&aasm.0, allocation);
    let mut output = String::new();

    output.push_str(".text\n");
    output.push_str(&format!(".globl {function_name}\n"));
    output.push_str(&format!(".type {function_name}, @function\n"));
    output.push_str(&format!("{function_name}:\n"));

    for instr in x86 {
        output.push_str(&format!("{instr}\n"));
    }

    output.push_str(&format!(".size {function_name}, .-{function_name}\n"));

    output
}
