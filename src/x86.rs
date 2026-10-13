// Function IR -> assembly
// LATER WILL BE deconstructed ssa -> assembly

use crate::allocate::{Allocation, PhysLoc, SpillSlot, allocate};
use crate::ir_function::ValueWidth;
use crate::ir_function::{
    CallTarget, Function, FunctionSlot, Instr as IrInstr, Module, Operand, PseudoOp,
};
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
const SCRATCH2: Register = Register::R10;
pub const RETURN_REG: Register = Register::Rax;
const STACK_ALIGNMENT: u32 = 16;
const BYTES_PER_SPILL: u32 = 8;
const BYTES_PER_REG: u32 = 8;

pub fn function_slot_register(slot: FunctionSlot) -> Option<Register> {
    match slot {
        FunctionSlot::Arg(index) => ARG_REGS.get(index).copied(),
        FunctionSlot::ReturnValue => Some(RETURN_REG),
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
        Register::Rbx => "lee9",
        Register::Rbp => "lee10",
        Register::R12 => "lee11",
        Register::R13 => "lee12",
        Register::R14 => "lee13",
        Register::R15 => "lee14",
        Register::Rsp => "rsp",
    }
}

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
    // excludes r10 and r11 (scratch registers), rbp, and rsp
    pub const AVAILABLE: &'static [Self] = &[
        Self::Rax,
        Self::Rcx,
        Self::Rdx,
        Self::Rdi,
        Self::Rsi,
        Self::R8,
        Self::R9,
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
    FunctionAddress {
        symbol: String,
        dest: Register,
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

    CallDirect(String),
    CallIndirect(X86Operand),
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

struct Emitter<'a> {
    instrs: Vec<Instr>,
    allocation: &'a Allocation,
    frame: ActivationRecord,
    epilogue_label: Label,
}

impl Emitter<'_> {
    fn push(&mut self, instr: Instr) {
        self.instrs.push(instr);
    }

    // reconfigure invalid mem to mem operations
    fn normalize_move(&mut self, dest: X86Operand, src: X86Operand, width: Width) {
        // remove useless self moves
        if dest == src {
            return;
        }

        match (dest, src) {
            // change mem to mem moves
            (X86Operand::Mem(dst), X86Operand::Mem(src)) => {
                self.push(Instr::Mov {
                    width,
                    dest: X86Operand::Reg(SCRATCH),
                    src: X86Operand::Mem(src),
                });
                self.push(Instr::Mov {
                    width,
                    dest: X86Operand::Mem(dst),
                    src: X86Operand::Reg(SCRATCH),
                });
            }
            (dest, src) => {
                self.push(Instr::Mov { width, dest, src });
            }
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

impl Emitter<'_> {
    fn lower_basic_binop(
        &mut self,
        op: PseudoOp,
        dest: X86Operand,
        lhs: X86Operand,
        rhs: X86Operand,
        width: Width,
    ) {
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
            self.normalize_move(scratch_op, lhs, width);
            self.push(op_to_instr(op, width, scratch_op, rhs));
            self.normalize_move(dest, scratch_op, width);
        } else {
            self.normalize_move(dest, lhs, width);
            self.push(op_to_instr(op, width, dest, rhs));
        }
    }

    fn lower_binop_width(
        &mut self,
        op: PseudoOp,
        dest: X86Operand,
        lhs: X86Operand,
        rhs: X86Operand,
        width: Width,
    ) {
        match op {
            PseudoOp::Add
            | PseudoOp::Sub
            | PseudoOp::And
            | PseudoOp::Or
            | PseudoOp::BitAnd
            | PseudoOp::BitOr
            | PseudoOp::BitXor => self.lower_basic_binop(op, dest, lhs, rhs, width),
            PseudoOp::Mul => self.lower_mul(dest, lhs, rhs, width),
            PseudoOp::Div | PseudoOp::Mod => self.lower_div_mod(op, dest, lhs, rhs, width),
            PseudoOp::Less
            | PseudoOp::LessEq
            | PseudoOp::Greater
            | PseudoOp::GreaterEq
            | PseudoOp::EqualEq
            | PseudoOp::NotEq => self.comparison_op(op, dest, lhs, rhs, width),
        }
    }

    fn lower_mul(&mut self, dest: X86Operand, lhs: X86Operand, rhs: X86Operand, width: Width) {
        if let X86Operand::Reg(dest_reg) = dest {
            // movq  rbx, rax
            // imulq rcx, rax
            // or
            // imulq rax, rbx
            if dest != rhs {
                self.normalize_move(dest, lhs, width);
            }
            self.push(Instr::Imul {
                width,
                dest: X86Operand::Reg(dest_reg),
                src: if dest == rhs { lhs } else { rhs },
            });
        } else {
            // move lhs to scratch
            // perform mul
            // move scratch to dest
            let scratch = X86Operand::Reg(SCRATCH);
            self.normalize_move(scratch, lhs, width);
            self.push(Instr::Imul {
                width,
                dest: scratch,
                src: rhs,
            });
            self.normalize_move(dest, scratch, width);
        }
    }

    fn lower_div_mod(
        &mut self,
        op: PseudoOp,
        dest: X86Operand,
        lhs: X86Operand,
        rhs: X86Operand,
        width: Width,
    ) {
        // dividend to rax
        // divisor to scratch
        // result in edx:eax
        self.normalize_move(X86Operand::Reg(SCRATCH), rhs, width);
        self.normalize_move(X86Operand::Reg(Register::Rax), lhs, width);
        self.push(Instr::Cdq); // sign extend eax to edx:eax
        self.push(Instr::Idiv {
            width,
            src: X86Operand::Reg(SCRATCH),
        });

        let result_reg = match op {
            PseudoOp::Div => Register::Rax,
            PseudoOp::Mod => Register::Rdx,
            _ => panic!(),
        };
        self.normalize_move(dest, X86Operand::Reg(result_reg), width);
    }

    fn comparison_op(
        &mut self,
        op: PseudoOp,
        dest: X86Operand,
        lhs: X86Operand,
        rhs: X86Operand,
        width: Width,
    ) {
        // mov    lhs, scratch
        // cmp    rhs, scratch
        // set    scratch
        // movzb  scratch, scratch // expand to full register
        // mov    scratch, dest
        let scratch = X86Operand::Reg(SCRATCH);
        self.normalize_move(scratch, lhs, width);
        self.push(Instr::Cmp {
            width,
            lhs: rhs,
            rhs: scratch,
        });

        let set = comparison_op_to_set(op);
        self.push(set);
        self.push(Instr::Movzx {
            src: SCRATCH,
            dest: SCRATCH,
        });
        self.normalize_move(dest, scratch, Width::Dword);
    }
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

impl Emitter<'_> {
    fn lower_move_width(&mut self, dest: &Operand, src: &Operand, width: Width) {
        let dest = match dest {
            Operand::FunctionSlot(FunctionSlot::Arg(index)) => resolve_outgoing_arg(*index),
            operand => resolve_operand(operand, self.allocation, &self.frame),
        };
        let src = resolve_operand(src, self.allocation, &self.frame);
        self.normalize_move(dest, src, width);
    }

    fn lower_function_address(&mut self, dest: &Operand, symbol: &str) {
        let dest = resolve_operand(dest, self.allocation, &self.frame);
        let (register, spill_dest) = match dest {
            X86Operand::Reg(register) => (register, None),
            X86Operand::Mem(memory) => (SCRATCH, Some(X86Operand::Mem(memory))),
            X86Operand::Imm(_) => panic!("function address can't write to an immediate"),
        };

        self.push(Instr::FunctionAddress {
            symbol: symbol.to_string(),
            dest: register,
        });
        if let Some(dest) = spill_dest {
            self.normalize_move(dest, X86Operand::Reg(register), Width::Qword);
        }
    }
}

fn value_width(width: ValueWidth) -> Width {
    match width {
        ValueWidth::Dword => Width::Dword,
        ValueWidth::Qword => Width::Qword,
    }
}

impl Emitter<'_> {
    fn address_component(&mut self, operand: &Operand, scratch: Register) -> Register {
        match resolve_operand(operand, self.allocation, &self.frame) {
            X86Operand::Reg(register) => register,
            source => {
                self.normalize_move(X86Operand::Reg(scratch), source, Width::Qword);
                scratch
            }
        }
    }

    fn lower_address(
        &mut self,
        dest: &Operand,
        base: &Operand,
        index: Option<&Operand>,
        scale: u8,
        displacement: i32,
    ) {
        assert!(matches!(scale, 1 | 2 | 4 | 8), "invalid x86 address scale");

        let base = self.address_component(base, SCRATCH);
        let index = index.map(|index| self.address_component(index, SCRATCH2));

        let dest_location = resolve_operand(dest, self.allocation, &self.frame);
        let (dest_register, spilled_dest) = match dest_location {
            X86Operand::Reg(register) => (register, false),
            X86Operand::Mem(_) => (SCRATCH, true),
            X86Operand::Imm(_) => panic!("address destination must be a temp"),
        };

        self.push(Instr::Lea {
            dest: dest_register,
            src: Memory {
                offset: displacement,
                base,
                index,
                scale,
            },
        });
        if spilled_dest {
            self.normalize_move(dest_location, X86Operand::Reg(dest_register), Width::Qword);
        }
    }

    fn lower_load(&mut self, dest: &Operand, address: &Operand, width: ValueWidth) {
        let width = value_width(width);
        let address = resolve_operand(address, self.allocation, &self.frame);
        self.normalize_move(X86Operand::Reg(SCRATCH2), address, Width::Qword);
        let source = X86Operand::Mem(Memory {
            offset: 0,
            base: SCRATCH2,
            index: None,
            scale: 1,
        });
        let dest = resolve_operand(dest, self.allocation, &self.frame);

        self.normalize_move(dest, source, width);
    }

    fn lower_store(&mut self, address: &Operand, src: &Operand, width: ValueWidth) {
        let width = value_width(width);
        let address = resolve_operand(address, self.allocation, &self.frame);
        self.normalize_move(X86Operand::Reg(SCRATCH2), address, Width::Qword);
        let dest = X86Operand::Mem(Memory {
            offset: 0,
            base: SCRATCH2,
            index: None,
            scale: 1,
        });
        let src = resolve_operand(src, self.allocation, &self.frame);

        self.normalize_move(dest, src, width);
    }
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

impl Emitter<'_> {
    fn lower_cjump(
        &mut self,
        lhs: &Operand,
        op: PseudoOp,
        rhs: &Operand,
        false_label: Label,
        width: Width,
    ) {
        let lhs = resolve_operand(lhs, self.allocation, &self.frame);
        let rhs = resolve_operand(rhs, self.allocation, &self.frame);
        if !matches!(lhs, X86Operand::Imm(_))
            && !matches!((lhs, rhs), (X86Operand::Mem(_), X86Operand::Mem(_)))
        {
            self.push(Instr::Cmp {
                width,
                lhs: rhs,
                rhs: lhs,
            });
        } else {
            self.normalize_move(X86Operand::Reg(SCRATCH), lhs, width);
            self.push(Instr::Cmp {
                width,
                lhs: rhs,
                rhs: X86Operand::Reg(SCRATCH),
            });
        }

        // the assembly only produces one false label, no true label
        // the true branch falls through
        // we need to invert the comparison op to jump to the false label
        self.push(inverse_comparison_jump(op, false_label));
    }
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

impl Emitter<'_> {
    fn lower_instr(&mut self, instr: &IrInstr) {
        match instr {
            IrInstr::Move { dest, src, width } => {
                self.lower_move_width(&dest.data, &src.data, value_width(*width))
            }
            IrInstr::BinOp {
                op,
                dest,
                lhs,
                rhs,
                width,
            } => {
                let dest = resolve_operand(&dest.data, self.allocation, &self.frame);
                let lhs = resolve_operand(&lhs.data, self.allocation, &self.frame);
                let rhs = resolve_operand(&rhs.data, self.allocation, &self.frame);
                self.lower_binop_width(op.data, dest, lhs, rhs, value_width(*width))
            }
            IrInstr::Load {
                dest,
                address,
                width,
            } => self.lower_load(&dest.data, &address.data, *width),
            IrInstr::Store {
                address,
                src,
                width,
            } => self.lower_store(&address.data, &src.data, *width),
            IrInstr::Address {
                dest,
                base,
                index,
                scale,
                displacement,
            } => self.lower_address(
                &dest.data,
                &base.data,
                index.as_ref().map(|index| &index.data),
                *scale,
                *displacement,
            ),
            IrInstr::FunctionAddress { dest, function } => {
                self.lower_function_address(&dest.data, &function.data)
            }
            IrInstr::Call { callee, .. } => {
                match callee {
                    CallTarget::Direct(name) => self.push(Instr::CallDirect(name.data.clone())),
                    CallTarget::Indirect(target) => self.push(Instr::CallIndirect(
                        resolve_operand(&target.data, self.allocation, &self.frame),
                    )),
                }
            }
            IrInstr::Return => self.push(Instr::Jmp(self.epilogue_label)),
            IrInstr::Jump(label) => self.push(Instr::Jmp(*label)),
            IrInstr::CJump {
                lhs,
                op,
                rhs,
                false_target: false_label,
                width,
                ..
            } => self.lower_cjump(
                &lhs.data,
                op.data,
                &rhs.data,
                *false_label,
                value_width(*width),
            ),
            IrInstr::Abort => self.push(Instr::CallDirect("abort".to_string())),
            IrInstr::Label(label) => self.push(Instr::Label(*label)),
        }
    }
}

impl Emitter<'_> {
    fn emit_prologue(&mut self) {
        self.push(Instr::Push(Register::Rbp));
        self.push(Instr::Mov {
            width: Width::Qword,
            dest: X86Operand::Reg(Register::Rbp),
            src: X86Operand::Reg(Register::Rsp),
        });

        for &register in &self.frame.pushed_registers {
            self.instrs.push(Instr::Push(register));
        }

        if self.frame.stack_size > 0 {
            self.push(Instr::SubStack(self.frame.stack_size as i32));
        }
    }

    fn emit_epilogue(&mut self) {
        self.push(Instr::Label(self.epilogue_label));
        if self.frame.stack_size > 0 {
            self.push(Instr::AddStack(self.frame.stack_size as i32));
        }

        for &register in self.frame.pushed_registers.iter().rev() {
            self.instrs.push(Instr::Pop(register));
        }

        self.push(Instr::Pop(Register::Rbp));
        self.push(Instr::Ret);
    }
}

// convert function IR to x86
pub fn lower_instrs(
    instrs: &[IrInstr],
    allocation: &Allocation,
    labelgen: &mut LabelGen,
) -> Vec<Instr> {
    let mut emitter = Emitter {
        instrs: Vec::new(),
        allocation,
        frame: ActivationRecord::new(allocation, instrs),
        epilogue_label: labelgen.fresh(),
    };
    emitter.emit_prologue();

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
        emitter.lower_instr(instr);
    }

    emitter.emit_epilogue();

    emitter.instrs
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
            Self::Lea { .. } | Self::FunctionAddress { .. } => "leaq",
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
            Self::CallDirect(_) | Self::CallIndirect(_) => "call",
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
            Self::FunctionAddress { symbol, dest } => {
                write!(f, "    leaq {symbol}(%rip), {}", dest.name(Width::Qword))
            }
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
            Self::CallDirect(name) => {
                write!(f, "    {} {name}", self.base())
            }
            Self::CallIndirect(target) => {
                write!(f, "    {} *{}", self.base(), target.asm(Width::Qword))
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
