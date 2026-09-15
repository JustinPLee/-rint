use std::collections::{HashMap, HashSet};

use crate::{
    codegen::Program,
    liveness::{InterferenceGraph, build_interference, do_liveness},
    temps::Temp,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    Spill(u32),
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
            Width::Byte => "b",
            Width::Word => "w",
            Width::Dword => "l",
            Width::Qword => "q",
        }
    }
}

impl Register {
    pub fn from_temp(temp: Temp) -> Self {
        match temp.0 {
            -1 => Self::Rax,
            _ => todo!("others shouldnt be used now"),
        }
    }

    pub fn name(self, width: Width) -> &'static str {
        match (self, width) {
            (Register::Rax, Width::Dword) => "%eax",
            (Register::Rbx, Width::Dword) => "%ebx",
            (Register::Rcx, Width::Dword) => "%ecx",
            (Register::Rdx, Width::Dword) => "%edx",
            (Register::Rsi, Width::Dword) => "%esi",
            (Register::Rdi, Width::Dword) => "%edi",
            (Register::Rbp, Width::Dword) => "%ebp",
            (Register::Rsp, Width::Dword) => "%esp",
            (Register::R8, Width::Dword) => "%r8d",
            (Register::R9, Width::Dword) => "%r9d",
            (Register::R10, Width::Dword) => "%r10d",
            (Register::R11, Width::Dword) => "%r11d",
            (Register::R12, Width::Dword) => "%r12d",
            (Register::R13, Width::Dword) => "%r13d",
            (Register::R14, Width::Dword) => "%r14d",
            (Register::R15, Width::Dword) => "%r15d",

            (Register::Rax, Width::Qword) => "%rax",
            (Register::Rbx, Width::Qword) => "%rbx",
            (Register::Rcx, Width::Qword) => "%rcx",
            (Register::Rdx, Width::Qword) => "%rdx",
            (Register::Rsi, Width::Qword) => "%rsi",
            (Register::Rdi, Width::Qword) => "%rdi",
            (Register::Rbp, Width::Qword) => "%rbp",
            (Register::Rsp, Width::Qword) => "%rsp",
            (Register::R8, Width::Qword) => "%r8",
            (Register::R9, Width::Qword) => "%r9",
            (Register::R10, Width::Qword) => "%r10",
            (Register::R11, Width::Qword) => "%r11",
            (Register::R12, Width::Qword) => "%r12",
            (Register::R13, Width::Qword) => "%r13",
            (Register::R14, Width::Qword) => "%r14",
            (Register::R15, Width::Qword) => "%r15",

            _ => panic!("unsupported register width"),
        }
    }

    pub const AVAILABLE: &'static [Register] = &[
        Register::Rcx,
        Register::Rbx,
        Register::Rdi,
        Register::Rsi,
        Register::R8,
        Register::R9,
        Register::R10,
        Register::R11,
        Register::R12,
        Register::R13,
        Register::R14,
    ];

    pub const RAX: Temp = Temp(-1);
    pub const SWAP: Register = Register::R15;
}

// simplicial elimination ordering, see handouts
// maybe in the future: store array of doubly-linked-lists that each hold all nodes with weight w, keep int var tracking index of max, each node has a pointer to its place in the linked list O(|v|+|e|))
pub fn seo(graph: &InterferenceGraph) -> Vec<Temp> {
    let mut weights: HashMap<Temp, usize> = graph.keys().map(|&temp| (temp, 0)).collect();
    let mut remaining: HashSet<Temp> = graph.keys().copied().collect();
    let mut order = Vec::new();

    while !remaining.is_empty() {
        // pick node with max weight
        let next = remaining
            .iter()
            .max_by_key(|temp| (weights.get(temp), temp.0))
            .copied()
            .unwrap();

        order.push(next);
        remaining.remove(&next);

        // increase weight of neighbors, this is simplified from the original
        if let Some(neighbors) = graph.get(&next) {
            for neighbor in neighbors {
                if remaining.contains(neighbor) {
                    *weights.entry(*neighbor).or_default() += 1;
                }
            }
        }
    }

    order
}

#[derive(Debug)]
pub struct Allocation {
    pub registers: HashMap<Temp, Register>,
    pub spills: HashMap<Temp, Register>,
}
// greedy, can use other heuristics to improve in the future
// for now it just picks the lowest numbered available register
// TODO: coalescing
fn choose_register(
    neighbors: &HashSet<Temp>,
    registers: &HashMap<Temp, Register>,
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

pub fn color(graph: &InterferenceGraph, order: &[Temp]) -> Allocation {
    let mut registers = HashMap::new();
    let mut spills = HashSet::new();
    for &temp in order {
        if temp.is_special() {
            continue;
        }

        let neighbors = graph.get(&temp).cloned().unwrap_or_default();
        match choose_register(&neighbors, &registers) {
            Some(reg) => {
                registers.insert(temp, reg);
            }
            None => {
                spills.insert(temp);
            }
        }
    }

    Allocation {
        registers,
        spills: assign_spills(&spills),
    }
}

fn assign_spills(spills: &HashSet<Temp>) -> HashMap<Temp, Register> {
    let mut ordered: Vec<Temp> = spills.iter().copied().collect();
    // for determinism
    ordered.sort_by_key(|t| t.0);

    ordered
        .into_iter()
        .enumerate()
        .map(|(i, temp)| (temp, Register::Spill(i as u32)))
        .collect()
}

pub fn allocate(program: &Program) -> Allocation {
    let live = do_liveness(&program.0);
    let graph = build_interference(&live);
    let order = seo(&graph);
    color(&graph, &order)
}
