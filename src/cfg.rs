// function IR -> cfg IR

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use crate::analysis::{AnalysisError, AnalysisErrorKind};
use crate::ir_function::{
    Function, FunctionSlot, Instr, LInstr, LOperand, Module, Operand, Program,
};
use crate::location::{Location, loc};
use crate::utils::{LABEL_ABORT_NULL_DEREF, Label};

pub type BlockId = usize; // for cyclic references
const ENTRY_LABEL: Label = Label(i32::MAX); // sentinel label

#[derive(Debug, Clone)]
pub struct Block {
    pub label: Option<Label>,
    pub instrs: Vec<LInstr>,
    pub preds: Vec<BlockId>,
    pub succs: Vec<BlockId>,

    pub uses: HashSet<Operand>,
    pub defs: HashSet<Operand>,
    // gen, kill?
}

#[derive(Debug, Clone)]
pub struct Cfg {
    blocks: Vec<Block>,
    entry: BlockId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitError {
    pub oper: Operand,
    pub location: Location,
}

fn write_operands(f: &mut fmt::Formatter<'_>, opers: &HashSet<Operand>) -> fmt::Result {
    let mut opers: Vec<&Operand> = opers.iter().collect();
    opers.sort();

    write!(f, "[")?;
    for (i, oper) in opers.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{oper}")?;
    }
    write!(f, "]")
}

fn operands_with_names(oper: &LOperand) -> Option<LOperand> {
    oper.data.has_name().then(|| oper.clone())
}

impl Instr {
    fn use_def(&self) -> (Vec<LOperand>, Vec<Operand>) {
        match self {
            Instr::Move { dest, src, .. } => {
                let uses = operands_with_names(src).into_iter().collect();
                let defs = operands_with_names(dest)
                    .into_iter()
                    .map(|op| op.data)
                    .collect();
                (uses, defs)
            }
            Instr::BinOp { dest, lhs, rhs, .. } => {
                let mut uses: Vec<LOperand> = operands_with_names(lhs).into_iter().collect();
                uses.extend(operands_with_names(rhs));

                let defs = operands_with_names(dest)
                    .into_iter()
                    .map(|op| op.data)
                    .collect();
                (uses, defs)
            }
            Instr::Load { dest, address, .. } => {
                let uses = operands_with_names(address).into_iter().collect();
                let defs = operands_with_names(dest)
                    .into_iter()
                    .map(|op| op.data)
                    .collect();
                (uses, defs)
            }
            Instr::Store { address, src, .. } => {
                let mut uses: Vec<LOperand> = operands_with_names(address).into_iter().collect();
                uses.extend(operands_with_names(src));
                (uses, Vec::new())
            }
            Instr::Address {
                dest, base, index, ..
            } => {
                let mut uses: Vec<LOperand> = operands_with_names(base).into_iter().collect();
                if let Some(index) = index {
                    uses.extend(operands_with_names(index));
                }
                let defs = operands_with_names(dest)
                    .into_iter()
                    .map(|op| op.data)
                    .collect();
                (uses, defs)
            }
            Instr::Call { arg_count, .. } => {
                let uses = (0..*arg_count)
                    .map(|index| {
                        loc(
                            Operand::FunctionSlot(FunctionSlot::Arg(index)),
                            Location::default(), // fix later
                        )
                    })
                    .collect();
                let defs = vec![Operand::FunctionSlot(FunctionSlot::ReturnValue)];
                (uses, defs)
            }
            Instr::CJump { lhs, rhs, .. } => {
                let mut uses: Vec<LOperand> = operands_with_names(lhs).into_iter().collect();
                uses.extend(operands_with_names(rhs));
                (uses, Vec::new())
            }
            Instr::Jump(_)
            | Instr::Label(_)
            | Instr::Abort
            | Instr::Push(_)
            | Instr::Pop(_)
            | Instr::Return => (Vec::new(), Vec::new()),
        }
    }
}

pub fn analyze_cfg(module: &Module) -> Result<(), AnalysisError> {
    for function in &module.functions {
        Cfg::check_function(function)?;
    }

    Ok(())
}

fn build_blocks(instrs: &[LInstr]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current_instrs = Vec::new();

    for instr in instrs {
        match &instr.data {
            // start of new block
            // start with a label
            Instr::Label(_) => {
                if !current_instrs.is_empty() {
                    blocks.push(new_block(&current_instrs));
                    current_instrs.clear();
                }

                current_instrs.push(instr.clone());
            }
            // end of a block
            // end with an explicit control transfer
            Instr::Jump(_) | Instr::CJump { .. } | Instr::Return | Instr::Abort => {
                current_instrs.push(instr.clone());

                blocks.push(new_block(&current_instrs));
                current_instrs.clear();
            }
            // add all other instructions
            _ => {
                current_instrs.push(instr.clone());
            }
        }
    }

    // capture dangling instructions
    if !current_instrs.is_empty() {
        blocks.push(new_block(&current_instrs));
    }

    blocks
}

fn new_block(instrs: &[LInstr]) -> Block {
    let label = instrs
        .iter()
        .find_map(|instr| match &instr.data {
            Instr::Label(label) => Some(label),
            _ => None,
        })
        .copied();

    Block {
        label,
        instrs: instrs.to_vec(),
        preds: Vec::new(),
        succs: Vec::new(),
        uses: HashSet::new(),
        defs: HashSet::new(),
    }
}

fn successors(
    block: &Block,
    block_id: BlockId,
    block_count: usize,
    label_to_block: &HashMap<Label, BlockId>,
) -> Vec<BlockId> {
    match block.instrs.last().map(|instr| &instr.data) {
        Some(Instr::Jump(target)) => vec![*label_to_block.get(target).expect("label exists")],
        Some(Instr::CJump {
            true_target,
            false_target,
            ..
        }) => vec![
            *label_to_block.get(true_target).expect("true label exists"),
            *label_to_block
                .get(false_target)
                .expect("false label exists"),
        ],
        Some(Instr::Return | Instr::Abort) => Vec::new(),
        // fallthrough
        // prevent fallthrough to the abort label
        _ if block_id + 1 < block_count
            && label_to_block.get(&LABEL_ABORT_NULL_DEREF).copied() != Some(block_id + 1) =>
        {
            vec![block_id + 1]
        }
        _ => Vec::new(),
    }
}

impl Block {
    fn entry() -> Block {
        Block {
            label: Some(ENTRY_LABEL),
            instrs: Vec::new(),
            preds: Vec::new(),
            succs: Vec::new(),
            uses: HashSet::new(),
            defs: HashSet::new(),
        }
    }

    fn compute_use_def(&mut self) {
        // temps that are not defined in this block, but are used
        let mut uses = HashSet::new();
        // temps that are defined in this block
        let mut defs = HashSet::new();

        for instr in &self.instrs {
            let (instr_uses, instr_defs) = instr.data.use_def();

            for temp in instr_uses {
                if !defs.contains(&temp.data) {
                    uses.insert(temp.data);
                }
            }

            defs.extend(instr_defs);
        }

        self.uses = uses;
        self.defs = defs;
    }
}

impl Cfg {
    pub fn new(program: &Program) -> Self {
        let mut blocks = build_blocks(&program.0);

        let entry = 0;
        blocks.insert(entry, Block::entry());
        let mut cfg = Cfg { blocks, entry };
        cfg.build_edges();

        for block in &mut cfg.blocks {
            block.compute_use_def();
        }

        cfg
    }

    /// checks use of initialized variables and all paths return
    fn check_function(function: &Function) -> Result<(), AnalysisError> {
        let cfg = Self::new(&function.body);

        if !cfg.all_paths_return() {
            return Err(AnalysisError::new(
                AnalysisErrorKind::NoReturn,
                function.name.location.clone(),
            ));
        }

        let mut params: HashSet<Operand> =
            function.params.iter().cloned().map(Operand::Temp).collect();
        params.extend(
            (0..function.params.len()).map(|index| Operand::FunctionSlot(FunctionSlot::Arg(index))),
        );
        if let Some(error) = cfg.check_initialized(&params) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::UseUninitializedVariable,
                error.location,
            ));
        }

        Ok(())
    }

    fn reachable_blocks(&self) -> HashSet<BlockId> {
        let mut reachable = HashSet::new();
        let mut worklist = VecDeque::from([self.entry]);

        while let Some(block_id) = worklist.pop_front() {
            // visited
            if !reachable.insert(block_id) {
                continue;
            }

            worklist.extend(self.blocks[block_id].succs.iter().copied());
        }

        reachable
    }

    fn all_paths_return(&self) -> bool {
        for block_id in self.reachable_blocks() {
            let block = &self.blocks[block_id];

            // return false on the first path that definitely doesn't return
            // otherwise continue searching
            match block.instrs.last().map(|instr| &instr.data) {
                // abort means termination and returns
                Some(Instr::Return) | Some(Instr::Abort) => {}
                // other normal blocks are "stuck" if they don't have a successor
                // return false
                Some(_) | None if block.succs.is_empty() => {
                    return false;
                }
                _ => {}
            }
        }

        true
    }

    // forward-must
    // in[entry] = parameters
    // in[b] = intersection of out[pred]
    // out[b] = in[b] union def[b]
    fn check_initialized(&self, params: &HashSet<Operand>) -> Option<InitError> {
        let reachable = self.reachable_blocks();

        // top of lattice contains all reachable definitions
        let mut top = params.clone();
        for &block_id in &reachable {
            top.extend(self.blocks[block_id].uses.iter().cloned());
            top.extend(self.blocks[block_id].defs.iter().cloned());
        }

        // generate in and out sets for all blocks
        // in set corresponds to initialized variables for this block
        // out set corresponds to initialized variables for successor blocks
        // default all sets to top because intersection always removes values, it cannot add
        // values to an empty set
        let mut block_in = vec![top.clone(); self.blocks.len()];
        let mut block_out = vec![top.clone(); self.blocks.len()];
        block_in[self.entry] = params.clone();
        block_out[self.entry] = params.clone();

        let mut worklist: VecDeque<BlockId> = (0..self.blocks.len())
            .filter(|block_id| reachable.contains(block_id))
            .collect();

        while let Some(block_id) = worklist.pop_front() {
            // in[b] = intersection of out[pred]
            let incoming: HashSet<Operand> = {
                if block_id == self.entry {
                    params.clone()
                } else {
                    let pred_ids: Vec<BlockId> = self.blocks[block_id]
                        .preds
                        .iter()
                        .copied()
                        .filter(|pred| reachable.contains(pred))
                        .collect();

                    let mut pred_outgoing =
                        block_out[*pred_ids.first().expect("all blocks have pred")].clone();
                    for pred_id in pred_ids.into_iter().skip(1) {
                        pred_outgoing = pred_outgoing
                            .intersection(&block_out[pred_id])
                            .cloned()
                            .collect();
                    }
                    pred_outgoing
                }
            };

            // out[b] = in[b] union def[b]
            let mut outgoing = incoming.clone();
            outgoing.extend(self.blocks[block_id].defs.iter().cloned());

            // important! if no updates are made, we terminate
            if incoming == block_in[block_id] && outgoing == block_out[block_id] {
                continue;
            }

            block_in[block_id] = incoming;
            block_out[block_id] = outgoing;

            worklist.extend(
                self.blocks[block_id]
                    .succs
                    .iter()
                    .copied()
                    .filter(|succ| reachable.contains(succ)),
            );
        }

        // re-traverse every block and find a use that is not initialized
        for block_id in 0..self.blocks.len() {
            if !reachable.contains(&block_id) {
                continue;
            }

            let mut initialized = block_in[block_id].clone();
            for instr in &self.blocks[block_id].instrs {
                let (uses, defs) = instr.data.use_def();

                for oper in uses {
                    if !initialized.contains(&oper.data) {
                        return Some(InitError {
                            oper: oper.data,
                            location: oper.location,
                        });
                    }
                }

                initialized.extend(defs);
            }
        }

        None
    }

    fn build_edges(&mut self) {
        let mut label_to_block_id = HashMap::new();

        for (id, block) in self.blocks.iter().enumerate() {
            if let Some(label) = block.label {
                label_to_block_id.insert(label, id);
            }
        }

        let block_count = self.blocks.len();
        for block_id in 0..block_count {
            let succs = successors(
                &self.blocks[block_id],
                block_id,
                block_count,
                &label_to_block_id,
            );
            self.blocks[block_id].succs = succs;
        }

        for block_id in 0..block_count {
            let succs = self.blocks[block_id].succs.clone();
            for succ in succs {
                self.blocks[succ].preds.push(block_id);
            }
        }
    }
}

impl fmt::Display for Cfg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (id, block) in self.blocks.iter().enumerate() {
            let label = match block.label {
                Some(label) if label == ENTRY_LABEL => "entry".to_string(),
                Some(label) => label.to_string(),
                None => "-".to_string(),
            };
            writeln!(f, "block {id} [{label}]")?;
            for instr in &block.instrs {
                writeln!(f, "  {}", instr.data)?;
            }
            writeln!(f, "  preds: {:?}", block.preds)?;
            writeln!(f, "  succs: {:?}", block.succs)?;
            write!(f, "  uses: ")?;
            write_operands(f, &block.uses)?;
            writeln!(f)?;
            write!(f, "  defs: ")?;
            write_operands(f, &block.defs)?;
            writeln!(f)?;
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir_function::PseudoOp;
    use crate::ir_linear::ValueWidth;
    use crate::location::{Located, Location};
    use crate::utils::Temp;

    fn locate<T>(data: T) -> Located<T> {
        Located {
            data,
            location: Location::default(),
        }
    }

    #[test]
    fn test_all_paths_return() {
        let returning_program = Program(vec![
            locate(Instr::CJump {
                lhs: locate(Operand::Imm(0)),
                op: locate(PseudoOp::EqualEq),
                rhs: locate(Operand::Imm(1)),
                true_target: Label(0),
                false_target: Label(1),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Label(Label(1))),
            locate(Instr::Return),
            locate(Instr::Label(Label(0))),
            locate(Instr::Return),
        ]);

        let fallthrough_program = Program(vec![
            locate(Instr::CJump {
                lhs: locate(Operand::Imm(0)),
                op: locate(PseudoOp::EqualEq),
                rhs: locate(Operand::Imm(1)),
                true_target: Label(1),
                false_target: Label(0),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Label(Label(0))),
            locate(Instr::Return),
            locate(Instr::Label(Label(1))),
            locate(Instr::Move {
                dest: locate(Operand::Temp(Temp::new(0))),
                src: locate(Operand::Imm(2)),
                width: ValueWidth::Dword,
            }),
        ]);

        assert!(Cfg::new(&returning_program).all_paths_return());
        assert!(!Cfg::new(&fallthrough_program).all_paths_return());
    }

    #[test]
    fn test_if_does_not_return() {
        let program = Program(vec![
            locate(Instr::Label(Label(0))),
            locate(Instr::CJump {
                lhs: locate(Operand::Imm(1)),
                op: locate(PseudoOp::EqualEq),
                rhs: locate(Operand::Imm(1)),
                true_target: Label(0),
                false_target: Label(1),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Label(Label(1))),
        ]);

        assert!(!Cfg::new(&program).all_paths_return());
    }

    #[test]
    fn test_parameters_are_initialized() {
        let param = Temp::new(0);
        let program = Program(vec![
            locate(Instr::Move {
                dest: locate(Operand::FunctionSlot(FunctionSlot::ReturnValue)),
                src: locate(Operand::Temp(param)),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Return),
        ]);
        let cfg = Cfg::new(&program);
        let params = HashSet::from([Operand::Temp(param)]);

        assert!(cfg.check_initialized(&params).is_none());
    }

    #[test]
    fn test_unreachable_uninitialized_use_is_ignored() {
        let program = Program(vec![
            locate(Instr::Return),
            locate(Instr::Move {
                dest: locate(Operand::Temp(Temp::new(0))),
                src: locate(Operand::Temp(Temp::new(1))),
                width: ValueWidth::Dword,
            }),
        ]);
        let cfg = Cfg::new(&program);

        assert!(cfg.check_initialized(&HashSet::new()).is_none());
    }

    #[test]
    fn test_cfg() {
        let t0 = Operand::Temp(Temp::new(0));
        let t1 = Operand::Temp(Temp::new(1));
        let program = Program(vec![
            locate(Instr::Move {
                dest: locate(t0.clone()),
                src: locate(Operand::Imm(1)),
                width: ValueWidth::Dword,
            }),
            locate(Instr::CJump {
                lhs: locate(t0.clone()),
                op: locate(PseudoOp::EqualEq),
                rhs: locate(Operand::Imm(1)),
                true_target: Label(0),
                false_target: Label(2),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Label(Label(2))),
            locate(Instr::Move {
                dest: locate(t1.clone()),
                src: locate(Operand::Imm(2)),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Jump(Label(1))),
            locate(Instr::Label(Label(0))),
            locate(Instr::Move {
                dest: locate(t1.clone()),
                src: locate(Operand::Imm(3)),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Jump(Label(1))),
            locate(Instr::Label(Label(1))),
            locate(Instr::Move {
                dest: locate(Operand::FunctionSlot(FunctionSlot::ReturnValue)),
                src: locate(t1),
                width: ValueWidth::Dword,
            }),
            locate(Instr::Return),
        ]);

        let cfg = Cfg::new(&program);
        insta::assert_snapshot!(format!("{cfg}"));
    }
}
