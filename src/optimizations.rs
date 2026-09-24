// broken!
// simplify temps into constants, and constants in binary ops to constants and so on
// basic version, no ssa, doesn't do control flow
// use crate::ir_linear::{Instr, Operand, Program, PseudoOp};
// use crate::location::loc;
// use crate::utils::Temp;
// use std::collections::HashMap;
//
// type Env = HashMap<Temp, Operand>;
//
// fn simplify(operand: Operand, env: &Env) -> Operand {
//     match operand {
//         Operand::Temp(temp) => match env.get(&temp) {
//             Some(Operand::Temp(parent)) if *parent != temp => simplify(Operand::Temp(*parent), env),
//             Some(value) => value.clone(),
//             None => Operand::Temp(temp),
//         },
//         Operand::Imm(_) | Operand::Register(_) | Operand::StackArgIndex(_) => operand,
//     }
// }
//
// fn fold_constants(op: PseudoOp, lhs: Operand, rhs: Operand) -> Option<Operand> {
//     let (Operand::Imm(lhs), Operand::Imm(rhs)) = (&lhs, &rhs) else {
//         return None;
//     };
//
//     let value = match op {
//         PseudoOp::Add => lhs.wrapping_add(*rhs),
//         PseudoOp::Sub => lhs.wrapping_sub(*rhs),
//         PseudoOp::Mul => lhs.wrapping_mul(*rhs),
//         PseudoOp::Div if *rhs != 0 => lhs.wrapping_div(*rhs),
//         PseudoOp::Mod if *rhs != 0 => lhs.rem_euclid(*rhs),
//
//         PseudoOp::And => {
//             return Some(Operand::Imm((*lhs != 0 && *rhs != 0) as i32));
//         }
//         PseudoOp::Or => {
//             return Some(Operand::Imm((*lhs != 0 || *rhs != 0) as i32));
//         }
//
//         PseudoOp::BitAnd => lhs & rhs,
//         PseudoOp::BitOr => lhs | rhs,
//         PseudoOp::BitXor => lhs ^ rhs,
//
//         PseudoOp::Less => (*lhs < *rhs) as i32,
//         PseudoOp::LessEq => (*lhs <= *rhs) as i32,
//         PseudoOp::Greater => (*lhs > *rhs) as i32,
//         PseudoOp::GreaterEq => (*lhs >= *rhs) as i32,
//         PseudoOp::EqualEq => (*lhs == *rhs) as i32,
//         PseudoOp::NotEq => (*lhs != *rhs) as i32,
//
//         _ => panic!("div by 0"),
//     };
//
//     Some(Operand::Imm(value))
// }
//
// fn simplify_binop(
//     lhs: Operand,
//     op: PseudoOp,
//     rhs: Operand,
//     env: &Env,
// ) -> (Operand, PseudoOp, Operand) {
//     let lhs = simplify(lhs, env);
//     let rhs = simplify(rhs, env);
//     (lhs, op, rhs)
// }
//
// pub fn copy_const_prop(program: Program) -> Program {
//     let mut env: Env = HashMap::new();
//     let mut result = Vec::with_capacity(program.0.len());
//
//     for instr in program.0 {
//         let instr_location = instr.location;
//         let instr = match instr.data {
//             Instr::Move { dest, src } => {
//                 let src = loc(simplify(src.data, &env), src.location);
//                 if let Operand::Temp(temp) = &dest.data {
//                     env.insert(*temp, src.data.clone());
//                 }
//                 Instr::Move { dest, src }
//             }
//             Instr::BinOp { dest, lhs, op, rhs } => {
//                 let lhs_location = lhs.location.clone();
//                 let rhs_location = rhs.location.clone();
//                 let op_location = op.location.clone();
//                 let (lhs, op, rhs) = simplify_binop(lhs.data, op.data, rhs.data, &env);
//                 let lhs = loc(lhs, lhs_location);
//                 let op = loc(op, op_location);
//                 let rhs = loc(rhs, rhs_location);
//                 if let Some(value) = fold_constants(op.data, lhs.data.clone(), rhs.data.clone()) {
//                     if let Operand::Temp(temp) = &dest.data {
//                         env.insert(*temp, value.clone());
//                     }
//                     Instr::Move {
//                         dest,
//                         src: loc(value, instr_location.clone()),
//                     }
//                 } else {
//                     if let Operand::Temp(temp) = &dest.data {
//                         env.remove(temp);
//                     }
//                     Instr::BinOp { dest, lhs, op, rhs }
//                 }
//             }
//             Instr::Call { dest, callee, args } => {
//                 let args = args
//                     .into_iter()
//                     .map(|arg| loc(simplify(arg.data, &env), arg.location))
//                     .collect();
//                 env.clear();
//                 Instr::Call { dest, callee, args }
//             }
//             Instr::Return(value) => {
//                 let value = value.map(|value| loc(simplify(value.data, &env), value.location));
//                 Instr::Return(value)
//             }
//             Instr::Abort => {
//                 env.clear();
//                 Instr::Abort
//             }
//             Instr::CJump {
//                 lhs,
//                 op,
//                 rhs,
//                 target,
//             } => {
//                 let lhs = loc(simplify(lhs.data, &env), lhs.location);
//                 let rhs = loc(simplify(rhs.data, &env), rhs.location);
//                 env.clear();
//                 Instr::CJump {
//                     lhs,
//                     op,
//                     rhs,
//                     target,
//                 }
//             }
//
//             Instr::Jump(label) => {
//                 env.clear();
//                 Instr::Jump(label)
//             }
//
//             Instr::Label(label) => {
//                 env.clear();
//                 Instr::Label(label)
//             }
//         };
//
//         result.push(loc(instr, instr_location));
//     }
//
//     Program(result)
// }
