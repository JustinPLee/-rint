// Function IR -> assembly
// LATER WILL BE deconstructed ssa -> assembly

use crate::allocate::{Allocation, PhysLoc, SpillSlot, allocate};
use crate::ir_function::{Function, FunctionSlot, Instr as IrInstr, Module, Operand, PseudoOp};
use crate::ir_linear::ValueWidth;
use crate::utils::{LABEL_ABORT_NULL_DEREF, Label, LabelGen};
use core::fmt;

// System V ABI for linux, macos, bsd

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Ord, PartialOrd)]
pub enum Register {
    Rax,
    Rcx,
    Rdx,
    Rbx,
    Rdi,
    Rsi,
    Rbp,
    Rsp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}
// additional arguments are passed on the stack
pub const ARG_REGS: [Register; 6] = [
    Register::Rdi,
    Register::Rsi,
    Register::Rdx,
    Register::Rcx,
    Register::R8,
    Register::R9,
];
const SCRATCH: Register = Register::R11;
pub const RETURN_REG: Register = Register::Rax;
const STACK_ALIGNMENT: u32 = 16;
const BYTES_PER_SPILL: u32 = 8;
const BYTES_PER_REG: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Byte,
    Word,
    Dword,
    Qword,
}

impl Width {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Byte => "b",
            Self::Word => "w",
            Self::Dword => "l",
            Self::Qword => "q",
        }
    }
}

impl Register {
    pub const CALLER_SAVED: [Self; 9] = [
        Self::Rax,
        Self::Rcx,
        Self::Rdx,
        Self::Rdi,
        Self::Rsi,
        Self::R8,
        Self::R9,
        Self::R10,
        Self::R11,
    ];
    // no rbp
    pub const CALLEE_SAVED: [Self; 5] = [Self::Rbx, Self::R12, Self::R13, Self::R14, Self::R15];
    pub const DIV_REGS: [Self; 2] = [Self::Rax, Self::Rdx];

    // caller saved registers are near the front (will be allocated more frequently)
    // excludes r11 (scratch register), rbp, and rsp
    pub const AVAILABLE: &'static [Self] = &[
        Self::Rax,
        Self::Rcx,
        Self::Rdx,
        Self::Rdi,
        Self::Rsi,
        Self::R8,
        Self::R9,
        Self::R10,
        Self::Rbx,
        Self::R12,
        Self::R13,
        Self::R14,
        Self::R15,
    ];

    pub fn to_function_slot(self) -> Option<FunctionSlot> {
        if self == RETURN_REG {
            Some(FunctionSlot::ReturnValue)
        } else {
            for (index, &register) in ARG_REGS.iter().enumerate() {
                if register == self {
                    return Some(FunctionSlot::Arg(index));
                }
            }
            None
        }
    }

    pub fn name(self, width: Width) -> &'static str {
        match (self, width) {
            (Self::Rax, Width::Dword) => "%eax",
            (Self::Rbx, Width::Dword) => "%ebx",
            (Self::Rcx, Width::Dword) => "%ecx",
            (Self::Rdx, Width::Dword) => "%edx",
            (Self::Rsi, Width::Dword) => "%esi",
            (Self::Rdi, Width::Dword) => "%edi",
            (Self::Rbp, Width::Dword) => "%ebp",
            (Self::Rsp, Width::Dword) => "%esp",
            (Self::R8, Width::Dword) => "%r8d",
            (Self::R9, Width::Dword) => "%r9d",
            (Self::R10, Width::Dword) => "%r10d",
            (Self::R11, Width::Dword) => "%r11d",
            (Self::R12, Width::Dword) => "%r12d",
            (Self::R13, Width::Dword) => "%r13d",
            (Self::R14, Width::Dword) => "%r14d",
            (Self::R15, Width::Dword) => "%r15d",
            (Self::Rax, Width::Qword) => "%rax",
            (Self::Rbx, Width::Qword) => "%rbx",
            (Self::Rcx, Width::Qword) => "%rcx",
            (Self::Rdx, Width::Qword) => "%rdx",
            (Self::Rsi, Width::Qword) => "%rsi",
            (Self::Rdi, Width::Qword) => "%rdi",
            (Self::Rbp, Width::Qword) => "%rbp",
            (Self::Rsp, Width::Qword) => "%rsp",
            (Self::R8, Width::Qword) => "%r8",
            (Self::R9, Width::Qword) => "%r9",
            (Self::R10, Width::Qword) => "%r10",
            (Self::R11, Width::Qword) => "%r11",
            (Self::R12, Width::Qword) => "%r12",
            (Self::R13, Width::Qword) => "%r13",
            (Self::R14, Width::Qword) => "%r14",
            (Self::R15, Width::Qword) => "%r15",
            (Self::R11, Width::Byte) => "%r11b",
            (Self::R11, Width::Word) => "%r11w",
            (Self::R15, Width::Byte) => "%r15b",
            (Self::R15, Width::Word) => "%r15w",
            _ => panic!("unsupported register width {self:?}|{width:?}"),
        }
    }
}

#[derive(Debug)]
struct ActivationRecord {
    stack_size: u32,
    pushed_registers: Vec<Register>,
}

// at&t syntax
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    pub offset: i32,
    pub base: Register,
    pub index: Option<Register>,
    pub scale: u8,
}

// no temps
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Operand {
    Reg(Register),
    Imm(i32),
    Mem(Memory),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instr {
    Mov {
        width: Width,
        dest: X86Operand,
        src: X86Operand,
    },
    // calculate address
    Lea {
        dest: Register,
        src: Memory,
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

    Call(String),
    Ret,
}

/// give temps a physical location
/// either as an immediate or register or memory offset
fn resolve_operand(
    operand: &Operand,
    allocation: &Allocation,
    frame: &ActivationRecord,
) -> X86Operand {
    match operand {
        Operand::Imm(value) => X86Operand::Imm(*value),
        Operand::Register(register) => X86Operand::Reg(*register),
        Operand::StackArgIndex(_) => todo!(),
        Operand::Temp(temp) => {
            let location = allocation.locations().get(temp).expect("temp is allocated");
            match location {
                PhysLoc::Register(register) => X86Operand::Reg(*register),
                PhysLoc::SpillSlot(SpillSlot(slot)) => X86Operand::Mem(Memory {
                    offset: frame.spill_offset(SpillSlot(*slot)),
                    base: Register::Rbp,
                    index: None,
                    scale: 1,
                }),
            }
        }
        Operand::FunctionSlot(FunctionSlot::ReturnValue) => X86Operand::Reg(RETURN_REG),
        Operand::FunctionSlot(FunctionSlot::Arg(index)) => resolve_incoming_arg(*index),
    }
}

fn resolve_incoming_arg(index: usize) -> X86Operand {
    if let Some(&register) = ARG_REGS.get(index) {
        X86Operand::Reg(register)
    } else {
        // the return address is at +8, so the first stack argument is at +16.
        // go UP the stack
        X86Operand::Mem(Memory {
            offset: ((2 * BYTES_PER_REG) + ((index - ARG_REGS.len()) as u32 * BYTES_PER_REG))
                as i32,
            base: Register::Rbp,
            index: None,
            scale: 1,
        })
    }
}

// reconfigure invalid mem to mem operations
fn normalize_move(dest: X86Operand, src: X86Operand, width: Width) -> Vec<Instr> {
    // remove useless self moves
    if dest == src {
        return Vec::new();
    }

    match (dest, src) {
        // change mem to mem moves
        (X86Operand::Mem(dst), X86Operand::Mem(src)) => {
            vec![
                Instr::Mov {
                    width,
                    dest: X86Operand::Reg(SCRATCH),
                    src: X86Operand::Mem(src),
                },
                Instr::Mov {
                    width,
                    dest: X86Operand::Mem(dst),
                    src: X86Operand::Reg(SCRATCH),
                },
            ]
        }
        (dest, src) => {
            vec![Instr::Mov { width, dest, src }]
        }
    }
}

fn op_to_instr(op: PseudoOp, width: Width, dest: X86Operand, src: X86Operand) -> Instr {
    match op {
        PseudoOp::Add => Instr::Add { width, dest, src },
        PseudoOp::Sub => Instr::Sub { width, dest, src },
        PseudoOp::And | PseudoOp::BitAnd => Instr::And { width, dest, src },
        PseudoOp::Or | PseudoOp::BitOr => Instr::Or { width, dest, src },
        PseudoOp::BitXor => Instr::Xor { width, dest, src },
        _ => panic!("unsupported op"),
    }
}

fn lower_basic_binop(
    op: PseudoOp,
    dest: X86Operand,
    lhs: X86Operand,
    rhs: X86Operand,
    width: Width,
) -> Vec<Instr> {
    let scratch_op = X86Operand::Reg(SCRATCH);

    // ex:
    // r1 = r2 + r1
    // mov r2, r1
    // add r1, r1
    // is incorrect
    //
    // r1 = r2 + r1
    // mov r2, %s
    // add r1, %s
    // mov %s, r1
    // is correct
    if dest == rhs || matches!(dest, X86Operand::Mem(_)) {
        let mut result = normalize_move(scratch_op, lhs, width);
        result.push(op_to_instr(op, width, scratch_op, rhs));
        result.extend(normalize_move(dest, scratch_op, width));
        result
    } else {
        let mut result = normalize_move(dest, lhs, width);
        result.push(op_to_instr(op, width, dest, rhs));
        result
    }
}

fn lower_binop_width(
    op: PseudoOp,
    dest: X86Operand,
    lhs: X86Operand,
    rhs: X86Operand,
    width: Width,
) -> Vec<Instr> {
    match op {
        PseudoOp::Add
        | PseudoOp::Sub
        | PseudoOp::And
        | PseudoOp::Or
        | PseudoOp::BitAnd
        | PseudoOp::BitOr
        | PseudoOp::BitXor => lower_basic_binop(op, dest, lhs, rhs, width),
        PseudoOp::Mul => lower_mul(dest, lhs, rhs, width),
        PseudoOp::Div | PseudoOp::Mod => lower_div_mod(op, dest, lhs, rhs, width),
        PseudoOp::Less
        | PseudoOp::LessEq
        | PseudoOp::Greater
        | PseudoOp::GreaterEq
        | PseudoOp::EqualEq
        | PseudoOp::NotEq => comparison_op(op, dest, lhs, rhs, width),
    }
}

fn lower_mul(dest: X86Operand, lhs: X86Operand, rhs: X86Operand, width: Width) -> Vec<Instr> {
    if let X86Operand::Reg(dest_reg) = dest {
        // movq  rbx, rax
        // imulq rcx, rax
        // or
        // imulq rax, rbx
        let mut result = if dest == rhs {
            Vec::new()
        } else {
            normalize_move(dest, lhs, width)
        };
        result.push(Instr::Imul {
            width,
            dest: X86Operand::Reg(dest_reg),
            src: if dest == rhs { lhs } else { rhs },
        });
        result
    } else {
        // move lhs to scratch
        // perform mul
        // move scratch to dest
        let scratch = X86Operand::Reg(SCRATCH);
        let mut result = normalize_move(scratch, lhs, width);
        result.push(Instr::Imul {
            width,
            dest: scratch,
            src: rhs,
        });
        result.extend(normalize_move(dest, scratch, width));
        result
    }
}

fn lower_div_mod(
    op: PseudoOp,
    dest: X86Operand,
    lhs: X86Operand,
    rhs: X86Operand,
    width: Width,
) -> Vec<Instr> {
    let mut result = Vec::new();

    // dividend to rax
    // divisor to scratch
    // result in edx:eax
    result.extend(normalize_move(X86Operand::Reg(SCRATCH), rhs, width));
    result.extend(normalize_move(X86Operand::Reg(Register::Rax), lhs, width));
    result.push(Instr::Cdq); // sign extend eax to edx:eax
    result.push(Instr::Idiv {
        width,
        src: X86Operand::Reg(SCRATCH),
    });

    let result_reg = match op {
        PseudoOp::Div => Register::Rax,
        PseudoOp::Mod => Register::Rdx,
        _ => panic!(),
    };
    result.extend(normalize_move(dest, X86Operand::Reg(result_reg), width));
    result
}

fn comparison_op(
    op: PseudoOp,
    dest: X86Operand,
    lhs: X86Operand,
    rhs: X86Operand,
    width: Width,
) -> Vec<Instr> {
    // mov    lhs, scratch
    // cmp    rhs, scratch
    // set    scratch
    // movzb  scratch, scratch // expand to full register
    // mov    scratch, dest
    let scratch = X86Operand::Reg(SCRATCH);
    let mut result = Vec::new();

    result.extend(normalize_move(scratch, lhs, width));
    result.push(Instr::Cmp {
        width,
        lhs: rhs,
        rhs: scratch,
    });

    let set = comparison_op_to_set(op);
    result.push(set);
    result.push(Instr::Movzx {
        src: SCRATCH,
        dest: SCRATCH,
    });
    result.extend(normalize_move(dest, scratch, Width::Dword));

    result
}

fn comparison_op_to_set(op: PseudoOp) -> Instr {
    match op {
        PseudoOp::Less => Instr::Setl { dest: SCRATCH },
        PseudoOp::LessEq => Instr::Setle { dest: SCRATCH },
        PseudoOp::Greater => Instr::Setg { dest: SCRATCH },
        PseudoOp::GreaterEq => Instr::Setge { dest: SCRATCH },
        PseudoOp::EqualEq => Instr::Sete { dest: SCRATCH },
        PseudoOp::NotEq => Instr::Setne { dest: SCRATCH },
        _ => panic!(),
    }
}

fn lower_move_width(
    dest: &Operand,
    src: &Operand,
    allocation: &Allocation,
    frame: &ActivationRecord,
    width: Width,
) -> Vec<Instr> {
    let dest = match dest {
        Operand::FunctionSlot(FunctionSlot::Arg(index)) => resolve_outgoing_arg(*index),
        operand => resolve_operand(operand, allocation, frame),
    };
    let src = resolve_operand(src, allocation, frame);
    normalize_move(dest, src, width)
}

fn value_width(width: ValueWidth) -> Width {
    match width {
        ValueWidth::Dword => Width::Dword,
        ValueWidth::Qword => Width::Qword,
    }
}

fn address_component(
    operand: &Operand,
    scratch: Register,
    allocation: &Allocation,
    frame: &ActivationRecord,
) -> (Vec<Instr>, Register) {
    match resolve_operand(operand, allocation, frame) {
        X86Operand::Reg(register) => (Vec::new(), register),
        source => (
            normalize_move(X86Operand::Reg(scratch), source, Width::Qword),
            scratch,
        ),
    }
}

fn lower_address(
    dest: &Operand,
    base: &Operand,
    index: Option<&Operand>,
    scale: u8,
    displacement: i32,
    allocation: &Allocation,
    frame: &ActivationRecord,
) -> Vec<Instr> {
    assert!(matches!(scale, 1 | 2 | 4 | 8), "invalid x86 address scale");

    let (mut result, base) = address_component(base, SCRATCH, allocation, frame);
    let index = if let Some(index) = index {
        let (instructions, register) = address_component(index, Register::R10, allocation, frame);
        result.extend(instructions);
        Some(register)
    } else {
        None
    };

    let dest_location = resolve_operand(dest, allocation, frame);
    let (dest_register, spilled_dest) = match dest_location {
        X86Operand::Reg(register) => (register, false),
        X86Operand::Mem(_) => (SCRATCH, true),
        X86Operand::Imm(_) => panic!("address destination must be a temp"),
    };

    result.push(Instr::Lea {
        dest: dest_register,
        src: Memory {
            offset: displacement,
            base,
            index,
            scale,
        },
    });
    if spilled_dest {
        result.extend(normalize_move(
            dest_location,
            X86Operand::Reg(dest_register),
            Width::Qword,
        ));
    }
    result
}

fn lower_load(
    dest: &Operand,
    address: &Operand,
    width: ValueWidth,
    allocation: &Allocation,
    frame: &ActivationRecord,
) -> Vec<Instr> {
    let width = value_width(width);
    let address = resolve_operand(address, allocation, frame);
    let mut result = normalize_move(X86Operand::Reg(Register::R10), address, Width::Qword);
    let source = X86Operand::Mem(Memory {
        offset: 0,
        base: Register::R10,
        index: None,
        scale: 1,
    });
    let dest = resolve_operand(dest, allocation, frame);

    result.extend(normalize_move(dest, source, width));

    result
}

fn lower_store(
    address: &Operand,
    src: &Operand,
    width: ValueWidth,
    allocation: &Allocation,
    frame: &ActivationRecord,
) -> Vec<Instr> {
    let width = value_width(width);
    let address = resolve_operand(address, allocation, frame);
    let mut result = normalize_move(X86Operand::Reg(Register::R10), address, Width::Qword);
    let dest = X86Operand::Mem(Memory {
        offset: 0,
        base: Register::R10,
        index: None,
        scale: 1,
    });
    let src = resolve_operand(src, allocation, frame);

    result.extend(normalize_move(dest, src, width));

    result
}

fn resolve_outgoing_arg(index: usize) -> X86Operand {
    if let Some(&register) = ARG_REGS.get(index) {
        X86Operand::Reg(register)
    } else {
        X86Operand::Mem(Memory {
            offset: ((index - ARG_REGS.len()) as i32) * BYTES_PER_REG as i32,
            base: Register::Rsp,
            index: None,
            scale: 1,
        })
    }
}

fn lower_cjump(
    lhs: &Operand,
    op: PseudoOp,
    rhs: &Operand,
    false_label: Label,
    allocation: &Allocation,
    frame: &ActivationRecord,
    width: Width,
) -> Vec<Instr> {
    let lhs = resolve_operand(lhs, allocation, frame);
    let rhs = resolve_operand(rhs, allocation, frame);
    let mut result = if !matches!(lhs, X86Operand::Imm(_))
        && !matches!((lhs, rhs), (X86Operand::Mem(_), X86Operand::Mem(_)))
    {
        vec![Instr::Cmp {
            width,
            lhs: rhs,
            rhs: lhs,
        }]
    } else {
        let mut result = normalize_move(X86Operand::Reg(SCRATCH), lhs, width);
        result.push(Instr::Cmp {
            width,
            lhs: rhs,
            rhs: X86Operand::Reg(SCRATCH),
        });
        result
    };

    // the assembly only produces one false label, no true label
    // the true branch falls through
    // we need to invert the comparison op to jump to the false label
    result.push(inverse_comparison_jump(op, false_label));
    result
}

fn inverse_comparison_jump(op: PseudoOp, target: Label) -> Instr {
    match op {
        PseudoOp::Less => Instr::Jge(target),
        PseudoOp::LessEq => Instr::Jg(target),
        PseudoOp::Greater => Instr::Jle(target),
        PseudoOp::GreaterEq => Instr::Jl(target),
        PseudoOp::EqualEq => Instr::Jne(target),
        PseudoOp::NotEq => Instr::Je(target),
        _ => panic!(),
    }
}

fn lower_instr(
    instr: &IrInstr,
    allocation: &Allocation,
    frame: &ActivationRecord,
    epilogue_label: Label,
) -> Vec<Instr> {
    match instr {
        IrInstr::Move { dest, src, width } => lower_move_width(
            &dest.data,
            &src.data,
            allocation,
            frame,
            value_width(*width),
        ),
        IrInstr::BinOp {
            op,
            dest,
            lhs,
            rhs,
            width,
        } => {
            let dest = resolve_operand(&dest.data, allocation, frame);
            let lhs = resolve_operand(&lhs.data, allocation, frame);
            let rhs = resolve_operand(&rhs.data, allocation, frame);
            lower_binop_width(op.data, dest, lhs, rhs, value_width(*width))
        }
        IrInstr::Load {
            dest,
            address,
            width,
        } => lower_load(&dest.data, &address.data, *width, allocation, frame),
        IrInstr::Store {
            address,
            src,
            width,
        } => lower_store(&address.data, &src.data, *width, allocation, frame),
        IrInstr::Address {
            dest,
            base,
            index,
            scale,
            displacement,
        } => lower_address(
            &dest.data,
            &base.data,
            index.as_ref().map(|index| &index.data),
            *scale,
            *displacement,
            allocation,
            frame,
        ),
        IrInstr::Call { callee, .. } => vec![Instr::Call(callee.data.clone())],
        IrInstr::Return => vec![Instr::Jmp(epilogue_label)],
        IrInstr::Jump(label) => vec![Instr::Jmp(*label)],
        IrInstr::CJump {
            lhs,
            op,
            rhs,
            false_target: false_label,
            width,
            ..
        } => lower_cjump(
            &lhs.data,
            op.data,
            &rhs.data,
            *false_label,
            allocation,
            frame,
            value_width(*width),
        ),
        IrInstr::Abort => vec![Instr::Call("abort".to_string())],
        IrInstr::Label(label) => vec![Instr::Label(*label)],
        IrInstr::Push(_) | IrInstr::Pop(_) => {
            panic!("push/pop are only inserted by prologue/epilogue. they should not appear here")
        }
    }
}

fn emit_prologue(frame: &ActivationRecord) -> Vec<Instr> {
    let mut result = vec![Instr::Push(Register::Rbp)];
    result.push(Instr::Mov {
        width: Width::Qword,
        dest: X86Operand::Reg(Register::Rbp),
        src: X86Operand::Reg(Register::Rsp),
    });

    for &register in &frame.pushed_registers {
        result.push(Instr::Push(register));
    }

    if frame.stack_size > 0 {
        result.push(Instr::SubStack(frame.stack_size as i32));
    }

    result
}

fn emit_epilogue(frame: &ActivationRecord, epilogue_label: Label) -> Vec<Instr> {
    let mut result = vec![Instr::Label(epilogue_label)];
    if frame.stack_size > 0 {
        result.push(Instr::AddStack(frame.stack_size as i32));
    }

    for &register in frame.pushed_registers.iter().rev() {
        result.push(Instr::Pop(register));
    }

    result.push(Instr::Pop(Register::Rbp));
    result.push(Instr::Ret);
    result
}

// convert function IR to x86
pub fn lower_instrs(
    instrs: &[IrInstr],
    allocation: &Allocation,
    labelgen: &mut LabelGen,
) -> Vec<Instr> {
    let frame = ActivationRecord::new(allocation, instrs);
    let epilogue_label = labelgen.fresh();

    let mut result = emit_prologue(&frame);

    let mut instrs = instrs.iter().peekable();
    while let Some(instr) = instrs.next() {
        if matches!(instr, IrInstr::Label(label) if *label == LABEL_ABORT_NULL_DEREF) {
            // although the ir has abort for each function,
            // there should only be one for the whole program
            // kind of messy
            if matches!(instrs.peek(), Some(IrInstr::Abort)) {
                instrs.next();
            }
            continue;
        }
        result.extend(lower_instr(instr, allocation, &frame, epilogue_label));
    }

    result.extend(emit_epilogue(&frame, epilogue_label));

    result
}

fn emit_function_assembly(
    function: &Function,
    allocation: &Allocation,
    labelgen: &mut LabelGen,
) -> String {
    let instrs: Vec<IrInstr> = function
        .body
        .0
        .iter()
        .map(|instr| instr.data.clone())
        .collect();
    let instrs = lower_instrs(&instrs, allocation, labelgen);
    let function_name = &function.name.data;

    let mut output = String::new();

    output.push_str(&format!(".globl {function_name}\n"));
    output.push_str(&format!(".type {function_name}, @function\n"));
    output.push_str(&format!("{function_name}:\n"));

    for instr in instrs {
        output.push_str(&format!("{instr}\n"));
    }

    output.push_str(&format!(".size {function_name}, .-{function_name}\n"));

    output
}

pub fn emit_assembly(module: &Module, labelgen: &mut LabelGen) -> String {
    let mut text = ".text\n".to_string();
    for function in &module.functions {
        let allocation = allocate(&function.body);

        text.push_str(&emit_function_assembly(function, &allocation, labelgen));
        text.push('\n');
    }
    text.push_str(&format!("{LABEL_ABORT_NULL_DEREF}:\n    call abort\n"));
    // added to silence warnings (?)
    text.push_str(".section .note.GNU-stack,\"\",@progbits\n");

    text
}

impl ActivationRecord {
    fn new(allocation: &Allocation, instrs: &[IrInstr]) -> Self {
        let pushed_registers = allocation.pushed_registers();
        let num_spill_slots = allocation
            .spills()
            .map(|(_, SpillSlot(slot))| slot + 1) // 0-indexed
            .max()
            .unwrap_or(0);
        let max_spill_bytes = num_spill_slots * BYTES_PER_SPILL;
        // the caller reserves space on the stack for arguments as needed, not the callee
        let mut max_arg_bytes_for_any_call = 0;
        for instr in instrs {
            let IrInstr::Call { arg_count, .. } = instr else {
                continue;
            };

            // saturating sub/mul never becomes negative
            let required_arg_bytes = arg_count
                .saturating_sub(ARG_REGS.len())
                .saturating_mul(BYTES_PER_REG as usize) as u32;
            max_arg_bytes_for_any_call = max_arg_bytes_for_any_call.max(required_arg_bytes);
        }
        let pushed_bytes = pushed_registers.len() as u32 * BYTES_PER_REG;
        let unaligned_size = pushed_bytes + max_spill_bytes + max_arg_bytes_for_any_call;
        // formula for aligning to the next multiple of STACK_ALIGNMENT
        let stack_padding = (STACK_ALIGNMENT - unaligned_size % STACK_ALIGNMENT) % STACK_ALIGNMENT;

        Self {
            stack_size: max_spill_bytes + max_arg_bytes_for_any_call + stack_padding,
            pushed_registers,
        }
    }

    fn spill_offset(&self, SpillSlot(slot): SpillSlot) -> i32 {
        let offset =
            self.pushed_registers.len() as u32 * BYTES_PER_REG + BYTES_PER_SPILL * (slot + 1);
        -(offset as i32)
    }
}

impl Memory {
    fn asm(self) -> String {
        let base = self.base.name(Width::Qword);
        let displacement = if self.offset == 0 {
            String::new()
        } else {
            self.offset.to_string()
        };
        match self.index {
            Some(index) => format!(
                "{displacement}({base},{},{})",
                index.name(Width::Qword),
                self.scale
            ),
            None => format!("{displacement}({base})"),
        }
    }
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

impl Instr {
    fn base(&self) -> &'static str {
        match self {
            Self::Mov { .. } => "mov",
            Self::Lea { .. } => "leaq",
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
            Self::Call(_) => "call",
            Self::Ret => "ret",

            Self::Sete { .. } => "sete",
            Self::Setne { .. } => "setne",
            Self::Setl { .. } => "setl",
            Self::Setle { .. } => "setle",
            Self::Setg { .. } => "setg",
            Self::Setge { .. } => "setge",

            Self::Movzx { .. } => "movzbl",
            Self::Comment(_) => unimplemented!(),
            Self::Directive(_) => unimplemented!(),
        }
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.clone() {
            Self::Lea { dest, src } => write!(
                f,
                "    {} {}, {}",
                self.base(),
                src.asm(),
                dest.name(Width::Qword)
            ),
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
                write!(f, "    {}", self.base())
            }
            Self::Push(reg) | Self::Pop(reg) => {
                write!(f, "    {} {}", self.base(), reg.name(Width::Qword))
            }
            Self::AddStack(amount) | Self::SubStack(amount) => {
                write!(
                    f,
                    "    {} ${amount}, {}",
                    self.base(),
                    Register::Rsp.name(Width::Qword)
                )
            }
            Self::Ret => {
                write!(f, "    {}", self.base())
            }
            Self::Call(name) => {
                write!(f, "    {} {name}", self.base())
            }
            Self::Cmp { lhs, rhs, width } => {
                write!(
                    f,
                    "    {}{} {}, {}",
                    self.base(),
                    width.suffix(),
                    lhs.asm(width),
                    rhs.asm(width)
                )
            }
            Self::Jmp(label)
            | Self::Je(label)
            | Self::Jne(label)
            | Self::Jl(label)
            | Self::Jle(label)
            | Self::Jg(label)
            | Self::Jge(label) => write!(f, "    {} {label}", self.base()),

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
                    "    {} {}, {}",
                    self.base(),
                    src.name(Width::Byte),
                    dest.name(Width::Dword)
                )
            }
        }
    }
}
