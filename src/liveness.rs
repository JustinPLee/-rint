use crate::codegen::{Instr, Operand};
use crate::temps::{Label, Temp};
use std::collections::{HashMap, HashSet};

// view notes for a refresher
#[derive(Clone, Debug)]
pub struct Liveness {
    pub instr: Instr,

    pub uses: HashSet<Temp>,
    pub defs: HashSet<Temp>,

    pub live_in: HashSet<Temp>,  // live before this line executes
    pub live_out: HashSet<Temp>, // live after this line executes
}

fn label_idxs(instrs: &[Instr]) -> HashMap<Label, usize> {
    let mut labels = HashMap::new();
    for (i, instr) in instrs.iter().enumerate() {
        if let Instr::Label(label) = instr {
            labels.insert(*label, i);
        }
    }
    labels
}

// for cfg
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
        Instr::CJump { target, .. } => {
            let target_index = *labels
                .get(target)
                .expect("conditional jump target must exist");

            let mut result = vec![target_index];

            if index + 1 < instrs.len() {
                result.push(index + 1);
            }

            result
        }
        Instr::Ret(_) => Vec::new(),

        _ => {
            if index + 1 < instrs.len() {
                vec![index + 1]
            } else {
                Vec::new()
            }
        }
    }
}

fn uses(instr: &Instr) -> HashSet<Temp> {
    match instr {
        Instr::Move { src, .. } => get_temp(src).into_iter().collect(),
        Instr::BinOp { lhs, rhs, .. } => {
            let mut result = HashSet::new();
            if let Operand::Temp(t) = lhs {
                result.insert(*t);
            }
            if let Operand::Temp(t) = rhs {
                result.insert(*t);
            }
            result
        }
        Instr::Jump(_) => HashSet::new(),
        Instr::CJump { lhs, rhs, .. } => {
            let mut result = HashSet::new();
            if let Operand::Temp(t) = lhs {
                result.insert(*t);
            }
            if let Operand::Temp(t) = rhs {
                result.insert(*t);
            }
            result
        }
        Instr::Ret(operand) => get_temp(operand).into_iter().collect(),
        Instr::Label(_) => HashSet::new(),
        Instr::Directive(_) => HashSet::new(),
        Instr::Comment(_) => HashSet::new(),
    }
}

fn defs(instr: &Instr) -> HashSet<Temp> {
    match instr {
        Instr::Move { dest, .. } | Instr::BinOp { dest, .. } => {
            get_temp(dest).into_iter().collect()
        }
        Instr::Jump(_) => HashSet::new(),
        Instr::CJump { .. } => HashSet::new(),
        Instr::Ret(_) => HashSet::new(),
        Instr::Label(_) => HashSet::new(),
        Instr::Directive(_) => HashSet::new(),
        Instr::Comment(_) => HashSet::new(),
    }
}

fn get_temp(op: &Operand) -> Option<Temp> {
    match op {
        Operand::Temp(t) => Some(*t),
        Operand::Imm(_) | Operand::Reg(_) => None,
    }
}

pub fn do_liveness(instrs: &[Instr]) -> Vec<Liveness> {
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

    let labels = label_idxs(instrs);

    // see notes, don't remember
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

        if !changed {
            break;
        }
    }

    result
}

pub type InterferenceGraph = HashMap<Temp, HashSet<Temp>>;
fn add_node(graph: &mut InterferenceGraph, temp: Temp) {
    graph.entry(temp).or_default();
}
fn add_edge(graph: &mut InterferenceGraph, a: Temp, b: Temp) {
    if a == b {
        return;
    }

    graph.entry(a).or_default().insert(b);
    graph.entry(b).or_default().insert(a);
}

// edge between nodes if they interefere
pub fn build_interference(instrs: &[Liveness]) -> InterferenceGraph {
    let mut graph = HashMap::new();
    for instr in instrs {
        for &temp in &instr.uses {
            add_node(&mut graph, temp);
        }
        for &temp in &instr.defs {
            add_node(&mut graph, temp);
        }
        for &def in &instr.defs {
            for &live in &instr.live_out {
                add_edge(&mut graph, def, live);
            }
        }
    }
    graph
}
