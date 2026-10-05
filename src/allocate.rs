use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ir_function::{FunctionSlot, Instr, Program, abstract_name};
use crate::liveness::{InterferenceGraph, Node, do_liveness, to_node};
use crate::utils::Temp;
use crate::x86::Register;

#[derive(Clone, Copy, Debug)]
pub enum PhysLoc {
    Register(Register),
    SpillSlot(SpillSlot),
}

#[derive(Clone, Copy, Debug)]
pub struct SpillSlot(pub u32);

#[derive(Debug)]
pub struct Allocation {
    locations: HashMap<Temp, PhysLoc>,
}

// see notes, allows for greedy ordering
pub fn max_card_search(graph: &InterferenceGraph) -> Vec<Node> {
    let mut precolored: Vec<Node> = graph
        .nodes()
        .copied()
        .filter(|node| matches!(node, Node::Register(_) | Node::FunctionSlot(_)))
        .collect();
    precolored.sort();

    let mut weights: HashMap<Node, usize> = graph
        .nodes()
        .filter(|node| matches!(node, Node::Temp(_)))
        .map(|&node| {
            let precolored_neighbors = graph
                .neighbors(&node)
                .expect("node in graph")
                .iter()
                .filter(|neighbor| matches!(neighbor, Node::Register(_) | Node::FunctionSlot(_)))
                .count();
            (node, precolored_neighbors)
        })
        .collect();

    let mut unallocated: HashSet<Node> = weights.keys().copied().collect();

    let mut order = precolored;
    while let Some(next) = unallocated
        .iter()
        .max_by_key(|node| (weights.get(node), *node))
        .copied()
    {
        // get neighbor with most neighbors
        order.push(next);
        unallocated.remove(&next);

        if let Some(neighbors) = graph.neighbors(&next) {
            for &neighbor in neighbors {
                if unallocated.contains(&neighbor) {
                    *weights.entry(neighbor).or_default() += 1;
                }
            }
        }
    }

    order
}

fn first_free_register(
    neighbors: &HashSet<Node>,
    registers: &HashMap<Node, Register>,
) -> Option<Register> {
    for &reg in Register::AVAILABLE {
        let used = neighbors
            .iter()
            .any(|temp| registers.get(temp) == Some(&reg));

        if !used {
            return Some(reg);
        }
    }
    None
}

// find arg0 <- t or t <- arg0 pair
fn arg_move_pair(instr: &Instr) -> Option<(Node, Node)> {
    let Instr::Move { dest, src, .. } = instr else {
        return None;
    };

    let (dest, src) = (to_node(dest)?, to_node(src)?);
    match (dest, src) {
        (Node::Temp(_), Node::FunctionSlot(FunctionSlot::Arg(_)))
        | (Node::FunctionSlot(FunctionSlot::Arg(_)), Node::Temp(_)) => Some((dest, src)),
        _ => None,
    }
}

// prefer arg move pairs first because parameter register locations are precolored/fixed
// this may eliminate unnecessary moves
fn arg_move_preferences(instrs: &[Instr]) -> HashMap<Node, Vec<Node>> {
    let mut preferences: HashMap<Node, Vec<Node>> = HashMap::new();

    for (dest, src) in instrs.iter().filter_map(arg_move_pair) {
        if dest == src {
            continue;
        }

        // add preference both ways
        let dest_preferences = preferences.entry(dest).or_default();
        if !dest_preferences.contains(&src) {
            dest_preferences.push(src);
        }

        let src_preferences = preferences.entry(src).or_default();
        if !src_preferences.contains(&dest) {
            src_preferences.push(dest);
        }
    }

    preferences
}

pub fn color(
    graph: &InterferenceGraph,
    order: &[Node],
    preferences: &HashMap<Node, Vec<Node>>,
) -> HashMap<Node, Register> {
    let mut registers = HashMap::new();
    for &node in graph.nodes() {
        match node {
            Node::Register(register) => {
                registers.insert(node, register);
            }
            Node::FunctionSlot(location) => {
                if let Some(register) = location.register() {
                    registers.insert(node, register);
                }
            }
            Node::Temp(_) => {}
        }
    }

    for &node in order {
        let Node::Temp(_) = node else {
            continue;
        };

        let neighbors = graph.neighbors(&node).cloned().unwrap_or_default();
        let preferred = preferences
            .get(&node)
            .into_iter()
            .flatten()
            .find_map(|source| {
                let register = registers.get(source).copied()?;
                let available = Register::AVAILABLE.contains(&register);
                let used = neighbors
                    .iter()
                    .any(|neighbor| registers.get(neighbor) == Some(&register));

                if available && !used {
                    Some(register)
                } else {
                    None
                }
            });

        if let Some(reg) = preferred.or_else(|| first_free_register(&neighbors, &registers)) {
            registers.insert(node, reg);
        }
    }

    registers
}

fn assign_spills(spills: &HashSet<Node>) -> HashMap<Temp, SpillSlot> {
    let mut ordered_temps: Vec<Temp> = spills
        .iter()
        .filter_map(|node| match node {
            Node::Temp(temp) => Some(*temp),
            Node::Register(_) | Node::FunctionSlot(_) => None,
        })
        .collect();
    // for determinism
    ordered_temps.sort_by_key(|t| t.0);

    ordered_temps
        .into_iter()
        .enumerate()
        .map(|(i, temp)| (temp, SpillSlot(i as u32)))
        .collect()
}

fn location_of_spills(
    graph: &InterferenceGraph,
    registers: &HashMap<Node, Register>,
) -> HashMap<Temp, PhysLoc> {
    let spills = graph
        .nodes()
        .copied()
        .filter(|node| matches!(node, Node::Temp(_)) && !registers.contains_key(node))
        .collect();

    assign_spills(&spills)
        .into_iter()
        .map(|(temp, slot)| (temp, PhysLoc::SpillSlot(slot)))
        .collect()
}

fn location_of_registers(registers: HashMap<Node, Register>) -> HashMap<Temp, PhysLoc> {
    registers
        .into_iter()
        .filter_map(|(node, register)| match node {
            Node::Temp(temp) => Some((temp, PhysLoc::Register(register))),
            Node::Register(_) | Node::FunctionSlot(_) => None,
        })
        .collect()
}

pub fn allocate(program: &Program) -> Allocation {
    // Greedy coloring with preferences for ABI argument moves.
    // NOTE: revisit when doing SSA

    let instrs: Vec<Instr> = program.0.iter().map(|instr| instr.data.clone()).collect();
    let arg_preferences = arg_move_preferences(&instrs);
    let liveness = do_liveness(&instrs);
    let graph = InterferenceGraph::build(&liveness);
    let order = max_card_search(&graph);
    let registers = color(&graph, &order, &arg_preferences);
    let spills = location_of_spills(&graph, &registers);
    let registers = location_of_registers(registers);
    let locations = spills.into_iter().chain(registers).collect();
    Allocation { locations }
}

impl Allocation {
    pub fn pushed_registers(&self) -> Vec<Register> {
        Register::CALLEE_SAVED
            .iter()
            .copied()
            .filter(|register| self.registers().any(|(_, assigned)| assigned == *register))
            .collect()
    }

    pub fn locations(&self) -> &HashMap<Temp, PhysLoc> {
        &self.locations
    }

    // keep in iterator form
    pub fn registers(&self) -> impl Iterator<Item = (Temp, Register)> {
        self.locations
            .iter()
            .filter_map(|(&temp, location)| match location {
                PhysLoc::Register(register) => Some((temp, *register)),
                PhysLoc::SpillSlot(_) => None,
            })
    }

    // keep in iterator form
    pub fn spills(&self) -> impl Iterator<Item = (Temp, SpillSlot)> {
        self.locations
            .iter()
            .filter_map(|(&temp, location)| match location {
                PhysLoc::Register(_) => None,
                PhysLoc::SpillSlot(slot) => Some((temp, *slot)),
            })
    }
}

impl fmt::Display for Allocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Registers:")?;
        let mut sorted_vec: Vec<(&Temp, &PhysLoc)> = self.locations.iter().collect();
        sorted_vec.sort_by(|a, b| a.0.cmp(b.0));
        for (temp, physloc) in sorted_vec {
            match physloc {
                PhysLoc::Register(register) => {
                    writeln!(f, "  {temp} → {}", abstract_name(*register))?;
                }
                PhysLoc::SpillSlot(spill_slot) => {
                    writeln!(f, "  {temp} → %s{}", spill_slot.0)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir_function::{FunctionSlot, Operand, PseudoOp};
    use crate::ir_linear::ValueWidth;
    use crate::location::{Location, loc};

    #[test]
    fn keeps_a_value_live_across_a_call_in_a_callee_saved_register() {
        let operand = |value| loc(value, Location::default());
        let b = Operand::Temp(Temp::new(0));
        let result = Operand::Temp(Temp::new(1));
        let product = Operand::Temp(Temp::new(2));
        let instrs = vec![
            Instr::Move {
                dest: operand(b.clone()),
                src: operand(Operand::FunctionSlot(FunctionSlot::Arg(0))),
                width: ValueWidth::Dword,
            },
            Instr::Move {
                dest: operand(Operand::FunctionSlot(FunctionSlot::Arg(0))),
                src: operand(b.clone()),
                width: ValueWidth::Dword,
            },
            Instr::Call {
                callee: loc("pow".to_string(), Location::default()),
                arg_count: 1,
            },
            Instr::Move {
                dest: operand(result.clone()),
                src: operand(Operand::FunctionSlot(FunctionSlot::ReturnValue)),
                width: ValueWidth::Dword,
            },
            Instr::BinOp {
                dest: operand(product.clone()),
                lhs: operand(b),
                op: loc(PseudoOp::Mul, Location::default()),
                rhs: operand(result),
                width: ValueWidth::Dword,
            },
            Instr::Move {
                dest: operand(Operand::FunctionSlot(FunctionSlot::ReturnValue)),
                src: operand(product),
                width: ValueWidth::Dword,
            },
            Instr::Return,
        ];
        let program = Program(
            instrs
                .iter()
                .cloned()
                .map(|instr| loc(instr, Location::default()))
                .collect(),
        );
        let allocation = allocate(&program);

        assert!(allocation.spills().next().is_none());
        assert_eq!(
            allocation
                .locations()
                .get(&Temp::new(0))
                .map(|location| match location {
                    PhysLoc::Register(register) => *register,
                    PhysLoc::SpillSlot(_) => panic!("b was spilled"),
                }),
            Some(Register::Rbx)
        );

        assert!(allocation.pushed_registers().contains(&Register::Rbx));
    }
}
