// ast -> linear IR

use crate::analysis::{Env, RType, tc_expr};
use crate::ast::{
    AsnOp, BinOp, Expr as AstExpr, GlobalDecl, LBinOp, LBlock, LExpr, LIdent, LLValue, LStmt,
    LValue, PostOp, Program as AstProgram, Stmt as AstStmt,
};
use crate::location::{Located, Location, loc};
use crate::utils::{LABEL_ABORT_NULL_DEREF, Label, LabelGen, Temp, TempGen};
use std::collections::HashMap;
use std::fmt;

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
    Less,
    LessEq,
    Greater,
    GreaterEq,
    EqualEq,
    NotEq,
}

#[derive(Clone, Debug, Hash, PartialOrd, Ord, Eq, PartialEq)]
pub enum Operand {
    Imm(i32),
    Temp(Temp),
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
    Call {
        dest: Option<Temp>, // void functions don't need a destination temp
        dest_width: Option<ValueWidth>,
        callee: LIdent,
        args: Vec<LOperand>,
        arg_widths: Vec<ValueWidth>,
    },
    Return(Option<LOperand>),
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
    Abort, // called by a false assertion
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueWidth {
    Dword,
    Qword,
}

pub type LInstr = Located<Instr>;
pub type LOperand = Located<Operand>;
pub type LPseudoOp = Located<PseudoOp>;

#[derive(Debug)]
pub struct Program(pub Vec<LInstr>); // name should maybe be body or similar

#[derive(Debug)]
pub struct Function {
    pub name: LIdent,
    pub params: Vec<Temp>,
    pub param_widths: Vec<ValueWidth>,
    pub ret_width: ValueWidth,
    pub body: Program,
}

// no global variables or constants yet
// one module, no importing or exporting
#[derive(Debug)]
pub struct Module {
    pub functions: Vec<Function>,
}

// for *some* optimization of duplicated labels
// ex:
//  if (true) {
//      return 2;
//  } else {
//      return 3;
//  }
//
//  both branches terminate so the label after false branch that true branch usually
//  jumps to is redundant and never jumped to
//
//  instructions of a particular block also stop being emitted if all paths of that block return
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    FallsThrough,
    Terminates,
}

// for nested while and for loops
#[derive(Clone, Copy, Debug)]
struct LoopLabels {
    break_label: Label,
    continue_label: Label,
}

#[derive(Debug)]
struct Ctx<'a> {
    env: HashMap<String, Temp>, // variable -> temp
    types: Env,
    tempgen: &'a mut TempGen,
    labelgen: &'a mut LabelGen,
    loop_stack: Vec<LoopLabels>,
    stmts: Vec<LInstr>,
}

enum Place {
    Local(Temp, RType),
    Memory(LOperand, RType),
}

fn type_width(typ: &RType) -> ValueWidth {
    match typ {
        RType::Pointer(_) | RType::Null => ValueWidth::Qword,
        _ => ValueWidth::Dword,
    }
}

impl Flow {
    fn falls_through(self) -> bool {
        self == Self::FallsThrough
    }
}

impl<'a> Ctx<'a> {
    fn push(&mut self, instr: Instr, location: Location) {
        self.stmts.push(loc(instr, location));
    }

    // TODO: this recomputes types for expressions, find a way to avoid redoing this work
    // works for now..
    fn get_type(&self, expr: &LExpr) -> RType {
        tc_expr(expr, &self.types).unwrap()
    }

    fn check_null_ptr(&mut self, address: &LOperand, location: Location) {
        let ok = self.labelgen.fresh();
        self.push(
            Instr::CJump {
                lhs: address.clone(),
                op: loc(PseudoOp::NotEq, location.clone()),
                rhs: loc(Operand::Imm(0), location.clone()),
                true_target: ok,
                false_target: LABEL_ABORT_NULL_DEREF,
                width: ValueWidth::Qword,
            },
            location.clone(),
        );
        self.push(Instr::Label(ok), location);
    }

    fn field_address(&mut self, base: LOperand, typ: &RType, field: &LIdent) -> LOperand {
        let offset = self.types.field_offset(typ, &field.data);
        let dest = self.tempgen.fresh();
        self.push(
            Instr::Address {
                dest: loc(Operand::Temp(dest), field.location.clone()),
                base,
                index: None,
                scale: 1,
                displacement: offset,
            },
            field.location.clone(),
        );
        loc(Operand::Temp(dest), field.location.clone())
    }

    fn trans_place(&mut self, value: &LLValue) -> Place {
        match &value.data {
            LValue::Ident(name) => Place::Local(
                *self.env.get(&name.data).unwrap(),
                self.types.variable_type(&name.data),
            ),
            LValue::Deref(pointer) => {
                let RType::Pointer(typ) = self.get_type(pointer) else {
                    panic!()
                };
                let address = self.trans_expr(pointer);
                self.check_null_ptr(&address, value.location.clone());
                Place::Memory(address, *typ)
            }
            LValue::Field { base, field } => {
                let Place::Memory(address, typ) = self.trans_place(base) else {
                    panic!()
                };
                let field_address = self.field_address(address, &typ, field);
                let field_type = self.types.field_type(&typ, field).unwrap();
                Place::Memory(field_address, field_type)
            }
            LValue::Index { .. } => todo!(),
        }
    }

    fn trans_address_of_expr(&mut self, expr: &LExpr) -> LOperand {
        match &expr.data {
            AstExpr::Deref(pointer) => {
                let address = self.trans_expr(pointer);
                self.check_null_ptr(&address, expr.location.clone());
                address
            }
            AstExpr::Field { ident, field } => {
                let typ = self.get_type(ident);
                let base = self.trans_address_of_expr(ident);
                self.field_address(base, &typ, field)
            }
            AstExpr::LValue(value) => {
                let Place::Memory(address, _) = self.trans_place(value) else {
                    panic!()
                };
                address
            }
            _ => panic!(),
        }
    }

    fn read_place(&mut self, place: &Place, location: Location) -> (LOperand, RType) {
        match place {
            Place::Local(temp, typ) => (loc(Operand::Temp(*temp), location), typ.clone()),
            // structs in memory return address of struct
            Place::Memory(address, typ) if matches!(typ, RType::Struct(_)) => {
                (address.clone(), typ.clone())
            }
            // other values in memory require a load into a temporary
            Place::Memory(address, typ) => {
                let dest = self.tempgen.fresh();
                self.push(
                    Instr::Load {
                        dest: loc(Operand::Temp(dest), location.clone()),
                        address: address.clone(),
                        width: type_width(typ),
                    },
                    location.clone(),
                );
                (loc(Operand::Temp(dest), location), typ.clone())
            }
        }
    }

    fn write_place(&mut self, place: Place, src: LOperand, location: Location) {
        match place {
            Place::Local(temp, typ) => self.push(
                Instr::Move {
                    dest: loc(Operand::Temp(temp), location.clone()),
                    src,
                    width: type_width(&typ),
                },
                location,
            ),
            Place::Memory(address, typ) => self.push(
                Instr::Store {
                    address,
                    src,
                    width: type_width(&typ),
                },
                location,
            ),
        }
    }

    fn assignment_op(op: AsnOp) -> PseudoOp {
        match op {
            AsnOp::PlusEq => PseudoOp::Add,
            AsnOp::MinusEq => PseudoOp::Sub,
            AsnOp::TimesEq => PseudoOp::Mul,
            AsnOp::DivEq => PseudoOp::Div,
            AsnOp::ModEq => PseudoOp::Mod,
            AsnOp::AndEq => PseudoOp::BitAnd,
            AsnOp::OrEq => PseudoOp::BitOr,
            AsnOp::XorEq => PseudoOp::BitXor,
            AsnOp::Eq | AsnOp::LShiftEq | AsnOp::RShiftEq => {
                panic!();
            }
        }
    }

    fn trans_call(&mut self, name: &LIdent, args: &[Box<LExpr>], location: Location) -> LOperand {
        let (param_types, ret_type) = self.types.function_type(&name.data);
        let mut call_args = Vec::new();
        // evaluate arguments left to right, call by value
        for arg in args {
            call_args.push(self.trans_expr(arg));
        }

        let dest = (ret_type != RType::Void).then(|| self.tempgen.fresh());
        self.push(
            Instr::Call {
                dest,
                dest_width: dest.map(|_| type_width(&ret_type)),
                callee: name.clone(),
                args: call_args,
                arg_widths: param_types.iter().map(type_width).collect(),
            },
            location.clone(),
        );
        loc(dest.map(Operand::Temp).unwrap_or(Operand::Imm(0)), location)
    }

    fn trans_cond(
        &mut self,
        expr: &LExpr,
        true_target: Label,
        false_target: Label,
        location: Location,
    ) {
        // optimization: place lhs and rhs in cond directly without generating temps for the lhs and
        // rhs result
        if let AstExpr::BinOp { op, lhs, rhs } = &expr.data
            && Ctx::is_comparison(op)
        {
            let large = type_width(&self.get_type(lhs)) == ValueWidth::Qword
                || type_width(&self.get_type(rhs)) == ValueWidth::Qword;
            let width = if large {
                ValueWidth::Qword
            } else {
                ValueWidth::Dword
            };
            let lhs = self.trans_expr(lhs);
            let rhs = self.trans_expr(rhs);
            let comparison = Instr::CJump {
                lhs,
                op: loc(Ctx::op_to_pseudo(op.data.clone()), op.location.clone()),
                rhs,
                true_target,
                false_target,
                width,
            };
            self.push(comparison, location);
            return;
        }

        let cond = self.trans_expr(expr);
        self.push(
            // cond == 1 is true
            Instr::CJump {
                lhs: cond,
                op: loc(PseudoOp::EqualEq, expr.location.clone()),
                rhs: loc(Operand::Imm(1), expr.location.clone()),
                true_target,
                false_target,
                width: ValueWidth::Dword,
            },
            location,
        );
    }

    fn trans_expr(&mut self, expr: &LExpr) -> LOperand {
        let source_loc = expr.location.clone();

        match &expr.data {
            AstExpr::Ident(name) => loc(
                Operand::Temp(*self.env.get(&name.data).expect("variable is in env")),
                source_loc,
            ),
            AstExpr::Int(value) => loc(Operand::Imm(*value), source_loc),
            AstExpr::True => loc(Operand::Imm(1), source_loc),
            AstExpr::False => loc(Operand::Imm(0), source_loc),
            AstExpr::Null => loc(Operand::Imm(0), source_loc),
            AstExpr::LValue(value) => {
                let place = self.trans_place(value);
                self.read_place(&place, source_loc).0
            }
            AstExpr::Field { .. } | AstExpr::Deref(_) => {
                let typ = self.get_type(expr);
                let address = self.trans_address_of_expr(expr);
                self.read_place(&Place::Memory(address, typ), source_loc).0
            }
            AstExpr::Alloc(typ) => {
                let size = self.types.layout(&self.types.type_of(typ)).0;
                let dest = self.tempgen.fresh();
                self.push(
                    Instr::Call {
                        dest: Some(dest),
                        dest_width: Some(ValueWidth::Qword),
                        callee: loc("calloc".to_string(), source_loc.clone()),
                        args: vec![
                            loc(Operand::Imm(1), source_loc.clone()), // number of elements
                            loc(Operand::Imm(size), source_loc.clone()), // size of each element
                        ],
                        arg_widths: vec![ValueWidth::Qword, ValueWidth::Qword],
                    },
                    source_loc.clone(),
                );
                let result = loc(Operand::Temp(dest), source_loc.clone());
                self.check_null_ptr(&result, source_loc);
                result
            }
            AstExpr::Index { .. } | AstExpr::AllocArray { .. } => {
                todo!()
            }
            AstExpr::BinOp { op, lhs, rhs } => {
                // evaluate left to right
                let large = type_width(&self.get_type(lhs)) == ValueWidth::Qword
                    || type_width(&self.get_type(rhs)) == ValueWidth::Qword;
                let width = if large {
                    ValueWidth::Qword
                } else {
                    ValueWidth::Dword
                };
                let lhs = self.trans_expr(lhs);
                let rhs = self.trans_expr(rhs);
                let dest = self.tempgen.fresh();
                let ir = Instr::BinOp {
                    dest: loc(Operand::Temp(dest), source_loc.clone()),
                    lhs,
                    op: loc(Self::op_to_pseudo(op.data.clone()), op.location.clone()),
                    rhs,
                    width,
                };
                self.push(ir, source_loc.clone());
                loc(Operand::Temp(dest), source_loc)
            }
            AstExpr::TernOp {
                cond,
                true_expr,
                false_expr,
            } => {
                let result = self.tempgen.fresh();
                let label_true = self.labelgen.fresh();
                let label_false = self.labelgen.fresh();
                let label_end = self.labelgen.fresh();
                self.trans_cond(cond, label_true, label_false, source_loc.clone());

                // true branch
                self.push(Instr::Label(label_true), source_loc.clone());
                let true_value = self.trans_expr(true_expr);
                let width = type_width(&self.get_type(expr));
                self.push(
                    Instr::Move {
                        dest: loc(Operand::Temp(result), source_loc.clone()),
                        src: true_value,
                        width,
                    },
                    source_loc.clone(),
                );
                self.push(Instr::Jump(label_end), source_loc.clone());

                // false branch
                self.push(Instr::Label(label_false), source_loc.clone());
                let false_value = self.trans_expr(false_expr);
                self.push(
                    Instr::Move {
                        dest: loc(Operand::Temp(result), source_loc.clone()),
                        src: false_value,
                        width,
                    },
                    source_loc.clone(),
                );
                self.push(Instr::Jump(label_end), source_loc.clone());
                self.push(Instr::Label(label_end), source_loc);
                loc(Operand::Temp(result), expr.location.clone())
            }
            AstExpr::FunCall { name, args } => self.trans_call(name, args, source_loc),
        }
    }

    fn is_comparison(op: &LBinOp) -> bool {
        matches!(
            op.data,
            BinOp::Greater
                | BinOp::GreaterEq
                | BinOp::Less
                | BinOp::LessEq
                | BinOp::EqualEq
                | BinOp::NotEq
        )
    }

    fn op_to_pseudo(op: BinOp) -> PseudoOp {
        match op {
            BinOp::Plus => PseudoOp::Add,
            BinOp::Minus => PseudoOp::Sub,
            BinOp::Times => PseudoOp::Mul,
            BinOp::Div => PseudoOp::Div,
            BinOp::Mod => PseudoOp::Mod,
            BinOp::BitAnd => PseudoOp::BitAnd,
            BinOp::BitOr => PseudoOp::BitOr,
            BinOp::BitXor => PseudoOp::BitXor,

            BinOp::Greater => PseudoOp::Greater,
            BinOp::GreaterEq => PseudoOp::GreaterEq,
            BinOp::Less => PseudoOp::Less,
            BinOp::LessEq => PseudoOp::LessEq,
            BinOp::EqualEq => PseudoOp::EqualEq,
            BinOp::NotEq => PseudoOp::NotEq,

            _ => panic!("op not supported"),
        }
    }

    fn trans_block(&mut self, block: &LBlock) -> Flow {
        for stmt in &block.data.0 {
            let flow = self.trans_stmt(stmt);

            // don't emit unreachable code
            if !flow.falls_through() {
                return flow;
            }
        }

        Flow::FallsThrough
    }

    // doesn't automatically push statements to stmts
    fn trans_branch(&mut self, block: Option<&LBlock>) -> (Vec<LInstr>, Flow) {
        let Some(block) = block else {
            return (Vec::new(), Flow::FallsThrough);
        };

        let mut branch_ctx = Ctx {
            env: self.env.clone(),
            types: self.types.clone(),
            tempgen: self.tempgen,
            labelgen: self.labelgen,
            loop_stack: self.loop_stack.clone(),
            stmts: Vec::new(),
        };
        let flow = branch_ctx.trans_block(block);
        (branch_ctx.stmts, flow)
    }

    fn trans_return(&mut self, expr: Option<&LExpr>, location: Location) -> Flow {
        if let Some(expr) = expr {
            let value = self.trans_expr(expr);
            self.push(Instr::Return(Some(value)), location);
        } else {
            self.push(Instr::Return(None), location);
        }

        Flow::Terminates
    }

    fn trans_if(
        &mut self,
        cond: &LExpr,
        true_block: &LBlock,
        false_block: Option<&LBlock>,
        location: Location,
    ) -> Flow {
        let label_true = self.labelgen.fresh();
        let label_false = self.labelgen.fresh();
        self.trans_cond(cond, label_true, label_false, location.clone());

        let (false_stmts, false_flow) = self.trans_branch(false_block);
        let (true_stmts, true_flow) = self.trans_branch(Some(true_block));
        let false_fallthrough = false_flow.falls_through();
        let true_fallthrough = true_flow.falls_through();

        // if both blocks terminate we don't need a label_end
        let label_end = if true_fallthrough || false_fallthrough {
            Some(self.labelgen.fresh())
        } else {
            None
        };

        self.push(Instr::Label(label_true), location.clone());
        self.stmts.extend(true_stmts);
        if true_fallthrough && let Some(label_end) = label_end {
            self.push(Instr::Jump(label_end), location.clone());
        }

        self.push(Instr::Label(label_false), location.clone());
        self.stmts.extend(false_stmts);
        if false_fallthrough && let Some(label_end) = label_end {
            self.push(Instr::Jump(label_end), location.clone());
        }

        if let Some(label_end) = label_end {
            self.push(Instr::Label(label_end), location);
        }

        if true_fallthrough || false_fallthrough {
            Flow::FallsThrough
        } else {
            Flow::Terminates
        }
    }

    fn trans_while(&mut self, cond: &LExpr, body: &LBlock, location: Location) -> Flow {
        let label_body = self.labelgen.fresh();
        let label_cond = self.labelgen.fresh();
        let label_exit = self.labelgen.fresh();

        self.loop_stack.push(LoopLabels {
            break_label: label_exit,
            continue_label: label_cond,
        });

        self.push(Instr::Label(label_cond), location.clone());
        self.trans_cond(cond, label_body, label_exit, location.clone());
        self.push(Instr::Label(label_body), location.clone());

        let body_flow = self.trans_block(body);
        if body_flow.falls_through() {
            self.push(Instr::Jump(label_cond), location.clone());
        }

        self.loop_stack.pop().expect("loops are good");
        self.push(Instr::Label(label_exit), location);

        Flow::FallsThrough
    }

    fn trans_for(
        &mut self,
        init: Option<&LStmt>,
        cond: &LExpr,
        step: Option<&LStmt>,
        body: &LBlock,
        location: Location,
    ) -> Flow {
        if let Some(init) = init {
            let init_flow = self.trans_stmt(init);
            if !init_flow.falls_through() {
                return init_flow;
            }
        }

        let label_body = self.labelgen.fresh();
        let label_cond = self.labelgen.fresh();
        let label_step = self.labelgen.fresh();
        let label_exit = self.labelgen.fresh();

        self.push(Instr::Label(label_cond), location.clone());
        self.trans_cond(cond, label_body, label_exit, location.clone());
        self.push(Instr::Label(label_body), location.clone());

        self.loop_stack.push(LoopLabels {
            break_label: label_exit,
            continue_label: label_step,
        });
        let body_flow = self.trans_block(body);
        self.loop_stack.pop().expect("loops are good");

        if body_flow.falls_through() {
            self.push(Instr::Jump(label_step), location.clone());
        }

        self.push(Instr::Label(label_step), location.clone());
        let step_flow = if let Some(step) = step {
            self.trans_stmt(step)
        } else {
            Flow::FallsThrough
        };
        if step_flow.falls_through() {
            self.push(Instr::Jump(label_cond), location.clone());
        }

        self.push(Instr::Label(label_exit), location);

        Flow::FallsThrough
    }

    fn trans_decl(&mut self, name: &LIdent, block: &LBlock) -> Flow {
        let temp = self.tempgen.fresh();
        // add and remove var from scope
        self.env.insert(name.data.clone(), temp);
        let flow = self.trans_block(block);
        self.env.remove(&name.data);
        self.types.erase_variable(&name.data);
        flow
    }

    fn trans_assign(&mut self, name: &LLValue, value: &LExpr, location: Location) -> Flow {
        let place = self.trans_place(name);
        let src = self.trans_expr(value);
        self.write_place(place, src, location);

        Flow::FallsThrough
    }

    fn trans_loop_jump(&mut self, label: Label, location: Location) -> Flow {
        self.push(Instr::Jump(label), location);
        Flow::Terminates
    }

    fn trans_assert(&mut self, expr: &LExpr, location: Location) -> Flow {
        let label_ok = self.labelgen.fresh();
        let label_abort = self.labelgen.fresh();
        let label_end = self.labelgen.fresh();
        self.trans_cond(expr, label_ok, label_abort, location.clone());

        self.push(Instr::Label(label_ok), location.clone());
        self.push(Instr::Jump(label_end), location.clone());
        self.push(Instr::Label(label_abort), location.clone());
        self.push(Instr::Abort, location.clone());
        self.push(Instr::Label(label_end), location);

        Flow::FallsThrough // assume assert always passes
    }

    fn trans_stmt(&mut self, stmt: &LStmt) -> Flow {
        let source_loc = stmt.location.clone();

        match &stmt.data {
            AstStmt::Decl { typ, name, block } => {
                self.types.assign_type(name.data.clone(), typ);
                self.trans_decl(name, block)
            }
            AstStmt::Assign { name, value } => self.trans_assign(name, value, source_loc),
            AstStmt::CompoundAssign { name, op, value } => {
                let place = self.trans_place(name);
                let old = self.read_place(&place, source_loc.clone()).0;
                let rhs = self.trans_expr(value);
                let dest = self.tempgen.fresh();
                self.push(
                    Instr::BinOp {
                        dest: loc(Operand::Temp(dest), source_loc.clone()),
                        lhs: old,
                        op: loc(Self::assignment_op(op.data.clone()), op.location.clone()),
                        rhs,
                        width: ValueWidth::Dword,
                    },
                    source_loc.clone(),
                );
                self.write_place(
                    place,
                    loc(Operand::Temp(dest), source_loc.clone()),
                    source_loc,
                );
                Flow::FallsThrough
            }
            AstStmt::Postfix { name, op } => {
                let place = self.trans_place(name);
                let old = self.read_place(&place, source_loc.clone()).0;
                let dest = self.tempgen.fresh();
                let operation = match op.data {
                    PostOp::DoublePlus => PseudoOp::Add,
                    PostOp::DoubleMinus => PseudoOp::Sub,
                };
                self.push(
                    Instr::BinOp {
                        dest: loc(Operand::Temp(dest), source_loc.clone()),
                        lhs: old,
                        op: loc(operation, op.location.clone()),
                        rhs: loc(Operand::Imm(1), source_loc.clone()),
                        width: ValueWidth::Dword,
                    },
                    source_loc.clone(),
                );
                self.write_place(
                    place,
                    loc(Operand::Temp(dest), source_loc.clone()),
                    source_loc,
                );
                Flow::FallsThrough
            }
            AstStmt::Return(expr) => self.trans_return(expr.as_ref(), source_loc),
            AstStmt::If {
                cond,
                true_block,
                false_block,
            } => self.trans_if(cond, true_block, false_block.as_ref(), source_loc),
            AstStmt::While { cond, body } => self.trans_while(cond, body, source_loc),
            AstStmt::For {
                init,
                cond,
                step,
                body,
            } => self.trans_for(init.as_deref(), cond, step.as_deref(), body, source_loc),
            AstStmt::Expr(expr) => {
                self.trans_expr(expr);
                Flow::FallsThrough
            }
            AstStmt::Break => {
                self.trans_loop_jump(self.current_loop_labels().break_label, source_loc)
            }
            AstStmt::Continue => {
                self.trans_loop_jump(self.current_loop_labels().continue_label, source_loc)
            }
            AstStmt::Assert(expr) => self.trans_assert(expr, source_loc),
            AstStmt::Block(block) => self.trans_block(block),
        }
    }
    fn current_loop_labels(&self) -> LoopLabels {
        *self.loop_stack.last().expect("loop labels exist")
    }
}

// remove fallthrough, add explicit jumps
fn make_jumps_explicit(stmts: Vec<LInstr>) -> Vec<LInstr> {
    let mut result: Vec<LInstr> = Vec::new();

    for instr in stmts {
        if let Instr::Label(label) = &instr.data
            && result.last().is_some_and(|prev| {
                !matches!(
                    prev.data,
                    Instr::Jump(_) | Instr::CJump { .. } | Instr::Return(_) | Instr::Abort
                )
            })
        {
            result.push(loc(Instr::Jump(*label), instr.location.clone()));
        }
        result.push(instr);
    }

    result
}

pub fn translate(
    ast: &AstProgram,
    types: &Env,
    temps: &mut TempGen,
    labels: &mut LabelGen,
) -> Module {
    let mut functions = Vec::new();

    for gdecl in &ast.0 {
        // don't translate typedefs or function declarations
        let GlobalDecl::FunDef {
            ret_typ,
            name,
            params,
            body: Some(body),
        } = &gdecl.data
        else {
            continue;
        };

        let mut ctx = Ctx {
            env: HashMap::new(),
            types: types.clone(),
            tempgen: temps,
            labelgen: labels,
            loop_stack: Vec::new(),
            stmts: Vec::new(),
        };

        // translate parameters first
        let mut param_temps = Vec::new();
        for param in params {
            let temp = ctx.tempgen.fresh();
            ctx.env.insert(param.data.name.data.clone(), temp);
            ctx.types
                .assign_type(param.data.name.data.clone(), &param.data.typ);
            param_temps.push(temp);
        }

        // then translate the body
        ctx.trans_block(body);

        let mut function_body = make_jumps_explicit(ctx.stmts);

        // add one abort location PER FUNCTION at the end
        // this is so cfg analysis works
        // the duplicate labels are removed when assembly is actually emitted
        let abort_location = body.location.clone();
        function_body.push(loc(
            Instr::Label(LABEL_ABORT_NULL_DEREF),
            abort_location.clone(),
        ));
        function_body.push(loc(Instr::Abort, abort_location));

        functions.push(Function {
            name: name.clone(),
            params: param_temps,
            param_widths: params
                .iter()
                .map(|param| type_width(&types.type_of(&param.data.typ)))
                .collect(),
            ret_width: type_width(&types.type_of(ret_typ)),
            body: Program(function_body),
        });
    }

    Module { functions }
}

fn write_instructions(f: &mut fmt::Formatter<'_>, program: &Program) -> fmt::Result {
    for stmt in &program.0 {
        if matches!(&stmt.data, Instr::Label(_)) {
            writeln!(f, "{}", stmt.data)?;
        } else {
            writeln!(f, "    {}", stmt.data)?;
        }
    }

    Ok(())
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "program:")?;
        write_instructions(f, self)
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fn {}(", self.name)?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        writeln!(f, "):")?;
        write_instructions(f, &self.body)
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, function) in self.functions.iter().enumerate() {
            write!(f, "{}", function)?;
            if i + 1 < self.functions.len() {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Temp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "t{}", self.0)
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Imm(x) => write!(f, "{x}"),
            Operand::Temp(t) => write!(f, "{t}"),
        }
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instr::Move { dest, src, width } => {
                let suffix = if *width == ValueWidth::Qword { "q" } else { "" };
                write!(f, "{dest} ←{suffix} {src}")
            }
            Instr::BinOp {
                dest,
                lhs,
                op,
                rhs,
                width,
            } => {
                let suffix = if *width == ValueWidth::Qword { "q" } else { "" };
                write!(f, "{dest} ←{suffix} {lhs} {op} {rhs}")
            }
            Instr::Load {
                dest,
                address,
                width,
            } => {
                write!(f, "{dest} ← load({width:?}) {address}")
            }
            Instr::Store {
                address,
                src,
                width,
            } => {
                write!(f, "store({width:?}) {address} ← {src}")
            }
            Instr::Address {
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
            Instr::Call {
                dest, callee, args, ..
            } => {
                if let Some(dest) = dest {
                    write!(f, "{dest} ← ")?;
                }
                write!(f, "call {callee}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            Instr::Return(operand) => {
                if let Some(operand) = operand {
                    write!(f, "return {operand}")
                } else {
                    write!(f, "return")
                }
            }
            Instr::Abort => {
                write!(f, "abort")
            }
            Instr::Jump(label) => {
                write!(f, "jump {label}")
            }
            Instr::CJump {
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
            Instr::Label(label) => write!(f, "{label}:"),
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

            PseudoOp::Less => "<",
            PseudoOp::LessEq => "<=",
            PseudoOp::Greater => ">",
            PseudoOp::GreaterEq => ">=",
            PseudoOp::EqualEq => "==",
            PseudoOp::NotEq => "!=",
        };

        write!(f, "{op}")
    }
}
