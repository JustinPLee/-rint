// liveness on function IR

use crate::ir_function::{FunctionSlot, Instr, LOperand, Operand, PseudoOp, abstract_name};
use crate::utils::{Label, Temp};
use crate::x86::Register;
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Ord, PartialOrd)]
pub enum Node {
    Temp(Temp),
    Register(Register),
    FunctionSlot(FunctionSlot),
}

impl Node {
    pub fn from_register(register: Register) -> Self {
        register
            .to_function_slot()
            .map(Node::FunctionSlot)
            .unwrap_or(Node::Register(register))
    }

    fn from_operand(operand: &Operand) -> Option<Self> {
        match operand {
            Operand::Temp(temp) => Some(Self::Temp(*temp)),
            Operand::Register(register) => Some(Self::from_register(*register)),
            Operand::FunctionSlot(slot) => Some(Self::FunctionSlot(*slot)),
            Operand::Imm(_) | Operand::StackArgIndex(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterferenceGraph(pub HashMap<Node, HashSet<Node>>);

// view notes for a refresher
#[derive(Clone, Debug)]
pub struct Liveness {
    pub instr: Instr,

    pub uses: HashSet<Node>,
    pub defs: HashSet<Node>,

    pub live_in: HashSet<Node>,  // live before this line executes
    pub live_out: HashSet<Node>, // live after this line executes
}
fn successors(
    index: usize,
    instr: &Instr,
    instrs: &[Instr],
    labels: &HashMap<Label, usize>,
) -> Vec<usize> {
    match instr {
        Instr::Jump(label) => {
            vec![*labels.get(label).expect("jump target must exist")]
        }
        Instr::CJump {
            true_target,
            false_target,
            ..
        } => vec![
            *labels.get(true_target).expect("true label must exist"),
            *labels.get(false_target).expect("false label must exist"),
        ],
        Instr::Return | Instr::Abort => Vec::new(),
        _ => {
            // fallthrough case or last block
            if index + 1 < instrs.len() {
                vec![index + 1]
            } else {
                Vec::new()
            }
        }
    }
}

fn uses(instr: &Instr) -> HashSet<Node> {
    match instr {
        Instr::Move { src, .. } => to_node(src).into_iter().collect(),
        Instr::BinOp { lhs, rhs, .. } => {
            let mut result = HashSet::new();
            if let Some(node) = to_node(lhs) {
                result.insert(node);
            }
            if let Some(node) = to_node(rhs) {
                result.insert(node);
            }
            result
        }
        Instr::Call { arg_count, .. } => (0..*arg_count)
            .map(|index| Node::FunctionSlot(FunctionSlot::Arg(index)))
            .collect(),
        Instr::Push(register) => HashSet::from([Node::from_register(*register)]),
        Instr::Pop(_) => HashSet::new(),
        Instr::Jump(_) => HashSet::new(),
        Instr::Abort => HashSet::new(),
        Instr::CJump { lhs, rhs, .. } => {
            let mut result = HashSet::new();
            if let Some(node) = to_node(lhs) {
                result.insert(node);
            }
            if let Some(node) = to_node(rhs) {
                result.insert(node);
            }
            result
        }
        // Callee-saved registers used by temps are preserved by the backend's
        // prologue and epilogue, so their incoming values are not liveness nodes.
        Instr::Return => HashSet::from([Node::FunctionSlot(FunctionSlot::ReturnValue)]),
        Instr::Label(_) => HashSet::new(),
    }
}

fn defs(instr: &Instr) -> HashSet<Node> {
    match instr {
        Instr::Move { dest, .. } => to_node(dest).into_iter().collect(),
        Instr::BinOp { dest, op, .. } => {
            let mut result: HashSet<Node> = to_node(dest).into_iter().collect();
            if matches!(op.data, PseudoOp::Div | PseudoOp::Mod) {
                result.extend(Register::DIV_REGS.into_iter().map(Node::from_register));
            }
            result
        }
        Instr::Call { .. } => {
            let mut result = HashSet::new();
            result.insert(Node::FunctionSlot(FunctionSlot::ReturnValue));
            // the callee may have used any caller-saved registers so mark them as defined
            result.extend(Register::CALLER_SAVED.into_iter().map(Node::from_register));
            result
        }
        Instr::Pop(register) => HashSet::from([Node::from_register(*register)]),
        Instr::Jump(_)
        | Instr::Abort
        | Instr::CJump { .. }
        | Instr::Return
        | Instr::Label(_)
        | Instr::Push(_) => HashSet::new(),
    }
}

pub fn to_node(op: &LOperand) -> Option<Node> {
    Node::from_operand(&op.data)
}

pub fn do_liveness(instrs: &[Instr]) -> Vec<Liveness> {
    // do liveness on each instruction
    let mut result: Vec<Liveness> = instrs
        .iter()
        .map(|instr| Liveness {
            instr: instr.clone(),
            uses: uses(instr),
            defs: defs(instr),
            live_in: HashSet::new(),
            live_out: HashSet::new(),
        })
        .collect();

    let labels = label_to_instr(instrs);

    // loop until no live sets change
    // view slides for more details
    // OPTIMIZE: this is liveness per instruction, not variable, which is slower
    loop {
        let mut changed = false;
        for i in (0..result.len()).rev() {
            let successors = successors(i, &result[i].instr, instrs, &labels);
            let mut new_live_out = HashSet::new();
            for successor in successors {
                new_live_out.extend(result[successor].live_in.iter().copied());
            }
            let mut new_live_in = result[i].uses.clone();
            for temp in &new_live_out {
                if !result[i].defs.contains(temp) {
                    new_live_in.insert(*temp);
                }
            }
            if new_live_in != result[i].live_in || new_live_out != result[i].live_out {
                changed = true;
            }

            result[i].live_in = new_live_in;
            result[i].live_out = new_live_out;
        }

        // termination condition
        if !changed {
            break;
        }
    }

    result
}

fn write_nodes(f: &mut fmt::Formatter<'_>, nodes: &HashSet<Node>) -> fmt::Result {
    let mut nodes: Vec<Node> = nodes.iter().copied().collect();
    nodes.sort();

    write!(f, "[")?;
    for (index, node) in nodes.iter().enumerate() {
        if index > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{node}")?;
    }
    write!(f, "]")
}

fn label_to_instr(instrs: &[Instr]) -> HashMap<Label, usize> {
    let mut labels = HashMap::new();
    for (i, instr) in instrs.iter().enumerate() {
        if let Instr::Label(label) = instr {
            labels.insert(*label, i);
        }
    }
    labels
}

impl InterferenceGraph {
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.0.keys()
    }

    pub fn neighbors(&self, node: &Node) -> Option<&HashSet<Node>> {
        self.0.get(node)
    }

    pub fn add_node(&mut self, node: Node) {
        self.0.entry(node).or_default();
    }

    pub fn remove_node(&mut self, node: Node) {
        if let Some(neighbors) = self.0.remove(&node) {
            for neighbor in neighbors {
                if let Some(nodes) = self.0.get_mut(&neighbor) {
                    nodes.remove(&node);
                }
            }
        }
    }

    pub fn add_edge(&mut self, a: Node, b: Node) {
        if a == b {
            return;
        }

        self.0.entry(a).or_default().insert(b);
        self.0.entry(b).or_default().insert(a);
    }

    // edge between nodes if they interfere
    pub fn build(instrs: &[Liveness]) -> Self {
        let mut graph = Self::default();
        for instr in instrs {
            for &node in &instr.uses {
                graph.add_node(node);
            }
            for &node in &instr.defs {
                graph.add_node(node);
            }
            for &def in &instr.defs {
                for &live in &instr.live_out {
                    graph.add_edge(def, live);
                }
            }

            // connect each use to all other uses
            let uses: Vec<Node> = instr.uses.iter().copied().collect();
            for (index, &use_node) in uses.iter().enumerate() {
                for &other_use in &uses[index + 1..] {
                    graph.add_edge(use_node, other_use);
                }
            }
        }
        graph
    }
}
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Temp(temp) => write!(f, "{temp}"),
            Node::Register(register) => f.write_str(abstract_name(*register)),
            Node::FunctionSlot(location) => write!(f, "{location}"),
        }
    }
}

impl fmt::Display for Liveness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.instr)?;
        writeln!(f)?;
        write!(f, "  uses: ")?;
        write_nodes(f, &self.uses)?;
        writeln!(f)?;
        write!(f, "  defs: ")?;
        write_nodes(f, &self.defs)?;
        writeln!(f)?;
        write!(f, "  live_in: ")?;
        write_nodes(f, &self.live_in)?;
        writeln!(f)?;
        write!(f, "  live_out: ")?;
        write_nodes(f, &self.live_out)
    }
}

impl fmt::Display for InterferenceGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut nodes: Vec<Node> = self.0.keys().copied().collect();
        nodes.sort();

        for node in nodes {
            let mut neighbors: Vec<Node> = self
                .0
                .get(&node)
                .expect("node in graph")
                .iter()
                .copied()
                .collect();
            neighbors.sort();

            write!(f, "{node}: [")?;
            for (index, neighbor) in neighbors.iter().enumerate() {
                if index > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{neighbor}")?;
            }
            writeln!(f, "]")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir_linear::LPseudoOp;
    use crate::location::{Location, loc};

    // for testing purposes
    fn operand(data: Operand) -> LOperand {
        loc(data, Location::default())
    }

    fn pseudo_op(data: crate::ir_linear::PseudoOp) -> LPseudoOp {
        loc(data, Location::default())
    }

    fn branch_instructions() -> Vec<Instr> {
        let t0 = Operand::Temp(Temp::new(0));
        let t1 = Operand::Temp(Temp::new(1));
        let t2 = Operand::Temp(Temp::new(2));

        vec![
            Instr::Move {
                dest: operand(t0.clone()),
                src: operand(Operand::Imm(1)),
            },
            Instr::CJump {
                lhs: operand(t0.clone()),
                op: pseudo_op(crate::ir_linear::PseudoOp::EqualEq),
                rhs: operand(Operand::Imm(1)),
                true_target: Label(0),
                false_target: Label(2),
            },
            Instr::Label(Label(2)),
            Instr::Move {
                dest: operand(t1.clone()),
                src: operand(Operand::Imm(2)),
            },
            Instr::Jump(Label(1)),
            Instr::Label(Label(0)),
            Instr::Move {
                dest: operand(t1.clone()),
                src: operand(Operand::Imm(3)),
            },
            Instr::Jump(Label(1)),
            Instr::Label(Label(1)),
            Instr::BinOp {
                dest: operand(t2.clone()),
                lhs: operand(t0),
                op: pseudo_op(crate::ir_linear::PseudoOp::Add),
                rhs: operand(t1),
            },
            Instr::Move {
                dest: operand(Operand::FunctionSlot(FunctionSlot::ReturnValue)),
                src: operand(t2),
            },
            Instr::Return,
        ]
    }

    #[test]
    fn test_liveness() {
        let instructions = branch_instructions();
        let liveness = do_liveness(&instructions);
        let display: String = liveness
            .iter()
            .enumerate()
            .map(|(index, line)| format!("{index}: {line}\n\n"))
            .collect();

        insta::assert_snapshot!(display);
    }

    #[test]
    fn test_interference_graph() {
        let instructions = branch_instructions();
        let liveness = do_liveness(&instructions);
        let graph = InterferenceGraph::build(&liveness);

        insta::assert_snapshot!(format!("{graph}"));
    }
}
