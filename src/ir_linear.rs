// ast -> linear IR

use crate::ast::{
    BinOp, Expr as AstExpr, GlobalDecl, LBinOp, LBlock, LExpr, LIdent, LStmt, LTyp,
    Program as AstProgram, Stmt as AstStmt,
};
use crate::location::{Located, Location, loc};
use crate::utils::{Label, LabelGen, Temp, TempGen};
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
    },
    BinOp {
        dest: LOperand,
        lhs: LOperand,
        op: LPseudoOp,
        rhs: LOperand,
    },
    Call {
        dest: Option<Temp>, // void functions don't need a destination temp
        callee: LIdent,
        args: Vec<LOperand>,
    },
    Return(Option<LOperand>),
    Jump(Label),
    CJump {
        lhs: LOperand,
        op: LPseudoOp,
        rhs: LOperand,
        true_target: Label,
        false_target: Label,
    },
    Label(Label),
    Abort, // called by a false assertion
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
    pub ret_type: LTyp,
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
    tempgen: &'a mut TempGen,
    labelgen: &'a mut LabelGen,
    loop_stack: Vec<LoopLabels>,
    stmts: Vec<LInstr>,
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

    fn trans_call(&mut self, name: &LIdent, args: &[Box<LExpr>], location: Location) -> LOperand {
        let mut call_args = Vec::new();
        // evaluate arguments left to right, call by value
        for arg in args {
            call_args.push(self.trans_expr(arg));
        }

        let dest = self.tempgen.fresh();
        self.push(
            Instr::Call {
                dest: Some(dest),
                callee: name.clone(),
                args: call_args,
            },
            location.clone(),
        );
        loc(Operand::Temp(dest), location)
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
            let lhs = self.trans_expr(lhs);
            let rhs = self.trans_expr(rhs);
            self.push(
                Instr::CJump {
                    lhs,
                    op: loc(Ctx::op_to_pseudo(op.data.clone()), op.location.clone()),
                    rhs,
                    true_target,
                    false_target,
                },
                location,
            );
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
            },
            location,
        );
    }

    fn trans_expr(&mut self, expr: &LExpr) -> LOperand {
        let source_loc = expr.location.clone();

        match &expr.data {
            AstExpr::Ident(name) => loc(
                Operand::Temp(*self.env.get(name).expect("variable is in env")),
                source_loc,
            ),
            AstExpr::Int(value) => loc(Operand::Imm(*value), source_loc),
            AstExpr::True => loc(Operand::Imm(1), source_loc),
            AstExpr::False => loc(Operand::Imm(0), source_loc),
            AstExpr::BinOp { op, lhs, rhs } => {
                // evaluate left to right
                let lhs = self.trans_expr(lhs);
                let rhs = self.trans_expr(rhs);
                let dest = self.tempgen.fresh();
                self.push(
                    Instr::BinOp {
                        dest: loc(Operand::Temp(dest), source_loc.clone()),
                        lhs,
                        op: loc(Self::op_to_pseudo(op.data.clone()), op.location.clone()),
                        rhs,
                    },
                    source_loc.clone(),
                );
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
                self.push(
                    Instr::Move {
                        dest: loc(Operand::Temp(result), source_loc.clone()),
                        src: true_value,
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
        flow
    }

    fn trans_assign(&mut self, name: &LIdent, value: &LExpr, location: Location) -> Flow {
        let dest = *self.env.get(&name.data).expect("variable is in env");
        let src = self.trans_expr(value);
        self.push(
            Instr::Move {
                dest: loc(Operand::Temp(dest), name.location.clone()),
                src,
            },
            location,
        );

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
            AstStmt::Decl { name, block, .. } => self.trans_decl(name, block),
            AstStmt::Assign { name, value } => self.trans_assign(name, value, source_loc),
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

fn make_edges_explicit(stmts: Vec<LInstr>) -> Vec<LInstr> {
    let mut result = Vec::with_capacity(stmts.len());

    for instr in stmts {
        let Some(label) = (match &instr.data {
            Instr::Label(label) => Some(*label),
            _ => None,
        }) else {
            result.push(instr);
            continue;
        };

        let previous_is_terminator = result.last().is_some_and(|previous| {
            matches!(
                previous.data,
                Instr::Jump(_) | Instr::CJump { .. } | Instr::Return(_) | Instr::Abort
            )
        });

        if !result.is_empty() && !previous_is_terminator {
            result.push(loc(Instr::Jump(label), instr.location.clone()));
        }
        result.push(instr);
    }

    result
}

pub fn translate(ast: &AstProgram, temps: &mut TempGen, labels: &mut LabelGen) -> Module {
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
            param_temps.push(temp);
        }

        // then translate the body
        ctx.trans_block(body);

        functions.push(Function {
            name: name.clone(),
            params: param_temps,
            ret_type: ret_typ.clone(),
            body: Program(make_edges_explicit(ctx.stmts)),
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
            Instr::Move { dest, src } => {
                write!(f, "{dest} ← {src}")
            }
            Instr::BinOp { dest, lhs, op, rhs } => {
                write!(f, "{dest} ← {lhs} {op} {rhs}")
            }
            Instr::Call { dest, callee, args } => {
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
            } => {
                write!(
                    f,
                    "if {lhs} {op} {rhs} jump {true_target} else jump {false_target}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{elaboration::elaborate, lexer::Lexer, parser::Parser};

    fn trans_source(source: &str) -> Module {
        let tokens = Lexer::new(source.as_bytes()).lex();
        let mut parser = Parser::new(&tokens);
        let ast_parse = parser.parse();
        assert!(
            parser.errors().is_none(),
            "parser errors: {:?}",
            parser.errors()
        );

        let ast = elaborate(ast_parse);
        let mut temps = TempGen::new();
        let mut labels = LabelGen::new();
        translate(&ast.data, &mut temps, &mut labels)
    }

    #[test]
    fn test_linear_ir() {
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

        let module = trans_source(source);
        insta::assert_snapshot!(format!("{module}"));
    }

    #[test]
    fn test_pow() {
        let source = r#"
            int pow(int b, int e) {
                if (e == 0) {
                    return 1;
                } else {
                    return b * pow(b, e - 1);
                }
            }
        "#;

        let module = trans_source(source);
        insta::assert_snapshot!(format!("{module}"));
    }
}
