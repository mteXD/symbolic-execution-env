use log::{debug, error, warn};
use std::{collections::HashMap, fmt::Debug, rc::Rc};

use crate::{
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{self},
        IntrinsicArg, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::CoreError::RebaseError,
    types::{
        CellIndex, FdEntry, FunctionData, FunctionDataError, Immediate, Input, Output, ProgramData,
        ProgramDataError,
    },
};

pub mod executor;
pub mod verifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    FunctionDataError(FunctionDataError),
    ProgramDataError(ProgramDataError),
    RebaseError,
    IoReadError,
    IoWriteError,
}

impl From<FunctionDataError> for CoreError {
    fn from(err: FunctionDataError) -> Self {
        CoreError::FunctionDataError(err)
    }
}

impl From<ProgramDataError> for CoreError {
    fn from(err: ProgramDataError) -> Self {
        CoreError::ProgramDataError(err)
    }
}

type CoreResult<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone)]
pub struct CoreMachine<Tag = ()> {
    function_data: FunctionData<Tag>,
    program_data: ProgramData<Tag>,
    output: Output,
    input: Input,
}

impl<Tag: Clone + Debug> CoreMachine<Tag> {
    pub fn new(program: impl Into<Rc<[Instruction<Tag>]>>) -> Self {
        Self {
            function_data: FunctionData::default(),
            program_data: ProgramData::new(program),
            output: Output::Stdout,
            input: Input::Stdin,
        }
    }

    pub fn function_get(&self, name: &str) -> CoreResult<&Instruction<Tag>> {
        self.function_data.get(name).map_err(Into::into)
    }

    pub fn function_insert(&mut self, name: String, entry: FdEntry<Tag>) -> CoreResult<()> {
        self.function_data.insert(name, entry)?;
        Ok(())
    }

    pub fn function_insert_current(&mut self, name: String) -> CoreResult<()> {
        let current = self.program_data.get_current()?;

        debug!("Function '{}' will point to {:?}", name, current);

        self.function_insert(name, FdEntry::Inst(current.to_owned())) // PERF: to_owned()
    }

    pub fn common_function_logic(&mut self, function_name: &str) -> CoreResult<Vec<String>> {
        let mut aliases = Vec::new();

        while let Some(Instruction::AluFunction(FunctionOp::FunctionDefine, name)) = self.next() {
            debug!("Found consecutive definition: '{}'", name);
            aliases.push(name);
        }

        match self.program_data.get_current() {
            Ok(Instruction::Block(_)) => (),
            Ok(instr) => {
                warn!(
                    "Expected block after function definitions, but found instruction: {:?}",
                    instr
                );
            }
            Err(err) => {
                error!(
                    "Error while fetching instruction for function definition: {:?}",
                    err
                );
            }
        }

        let function_name = function_name.to_owned();
        self.function_insert_current(function_name.clone())?;

        for alias in &aliases {
            self.function_insert(alias.clone(), FdEntry::Str(function_name.clone()))?;
        }

        Ok(aliases)
    }
}

impl<Tag: Clone + Debug> Iterator for CoreMachine<Tag> {
    type Item = Instruction<Tag>;

    fn next(&mut self) -> Option<Self::Item> {
        self.program_data.next()
    }
}

/// Dispatches instructions
trait Evaluate<Tag: Debug = ()> {
    type Error;

    fn evaluate_instruction(&mut self, instr: &Instruction<Tag>) -> Result<(), Self::Error> {
        use Instruction::*;
        use log::debug;

        match instr {
            AluNullary(instr) => {
                debug!("Evaling: {:?}", instr);
                self.evaluate_alu_nullary(instr)
            }
            AluUnaryImm(instr, imm) => {
                debug!("Evaling: {:?}, imm: {:?}", instr, imm);
                self.evaluate_alu_unary_imm(instr, *imm)
            }
            AluUnaryCell(instr, cell) => {
                debug!("Evaling: {:?}, cell: {:?}", instr, cell);
                self.evaluate_alu_unary_cell(instr, *cell)
            }
            AluBinary(instr, arg1, arg2) => {
                debug!("Evaling: {:?}; args: {:?}, {:?}", instr, arg1, arg2);
                self.evaluate_alu_binary(instr, *arg1, *arg2)
            }
            Block(instrs) => {
                debug!("Entering block...");
                self.evaluate_block(instrs.clone())
            }
            IfElse(cond_idx, when_true, when_false) => {
                debug!("Entering if-else block...");
                self.evaluate_ifelse(*cond_idx, when_true.clone(), when_false.clone()) // Cheap clone
            }
            AluFunction(instr, fun) => {
                debug!("Evaling: {:?}, fun: '{}'", instr, fun);
                self.evaluate_function(instr, fun)
            }
            AluIntrinsic(instr, arg) => {
                debug!("Evaling: {:?}, arg: {:?}", instr, arg);
                self.evaluate_intrinsic(instr, arg)
            }
        }?;

        Ok(())
    }

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<(), Self::Error>;
    fn evaluate_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm<Tag>,
        arg: Immediate,
    ) -> Result<(), Self::Error>;
    fn evaluate_alu_unary_cell(
        &mut self,
        instr: &UnaryOpCell,
        arg: CellIndex,
    ) -> Result<(), Self::Error>;
    fn evaluate_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> Result<(), Self::Error>;
    fn evaluate_block(&mut self, instrs: Rc<[Instruction<Tag>]>) -> Result<(), Self::Error>;
    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction<Tag>>,
        when_false: Rc<Instruction<Tag>>,
    ) -> Result<(), Self::Error>;
    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &str) -> Result<(), Self::Error>;
    fn evaluate_intrinsic(
        &mut self,
        instr: &IntrinsicOp,
        arg: &IntrinsicArg,
    ) -> Result<(), Self::Error>;
}

// =============================================================================
// Shared frame-stack mechanics used by both Executor and Verifier.
//
// Two variants:
//   * `Block`         — an isolating nested context (block / function body).
//                       Pops below `start` save displaced parent cells, and
//                       `Rebase` is permitted (it drains parent cells into the
//                       frame's `saved_below`). On exit, body-local cells are
//                       dropped and `saved_below` is replayed.
//   * `IfElseBranch`  — a NON-isolating marker. Pops are not trapped, `Rebase`
//                       is forbidden, and cell changes persist after exit.
//                       Pops that drop below an *enclosing* `Block`'s `start`
//                       are still saved against that outer block (so the
//                       enclosing block's restore-on-exit remains correct).
// =============================================================================

/// Per-cell record of how many times each named downgrader has downgraded the
/// value occupying this cell. Carried as cell metadata so it is saved/restored
/// together with the value through pops, blocks, and rebases, and discarded for
/// good when the cell is finally popped.
#[derive(Clone, Debug, Default)]
pub struct DowngradeCounts(HashMap<String, usize>);

impl DowngradeCounts {
    /// Increments the counter for `downgrader` and returns the new total.
    pub fn bump(&mut self, downgrader: &str) -> usize {
        let entry = self.0.entry(downgrader.to_owned()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Current count for `downgrader` (0 if never downgraded).
    pub fn get(&self, downgrader: &str) -> usize {
        self.0.get(downgrader).copied().unwrap_or(0)
    }

    /// Element-wise maximum with `other` (per downgrader). Used by the verifier
    /// to merge the two arms of an `if`/`else` conservatively: a value's count
    /// after the branch is the larger of the two possibilities.
    pub fn merge_max(&self, other: &Self) -> Self {
        let mut merged = self.0.clone();
        for (name, &count) in &other.0 {
            let entry = merged.entry(name.clone()).or_insert(0);
            *entry = (*entry).max(count);
        }
        DowngradeCounts(merged)
    }
}

#[derive(Clone, Debug)]
pub enum Frame<T> {
    Block { start: usize, saved_below: Vec<T> },
    IfElseBranch,
}

#[derive(Clone, Debug, Default)]
pub struct StackFrames<T: Clone> {
    cells: Vec<T>,
    base: usize,
    frames: Vec<Frame<T>>,
}

impl<T: Clone> StackFrames<T> {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            base: 0,
            frames: Vec::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, value: T) {
        self.cells.push(value);
    }

    /// Pops the top cell. If the resulting `cells.len()` drops below the start
    /// of the innermost enclosing `Block` frame, the popped value is saved
    /// against that block (and its `start` is decremented). `IfElseBranch`
    /// frames are transparent to this accounting.
    pub fn pop(&mut self) -> Option<T> {
        let popped = self.cells.pop()?;
        let len = self.cells.len();
        for frame in self.frames.iter_mut().rev() {
            match frame {
                Frame::Block { start, saved_below } => {
                    if len < *start {
                        saved_below.push(popped.clone());
                        *start -= 1;
                    }
                    break;
                }
                Frame::IfElseBranch => continue,
            }
        }
        Some(popped)
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<&T> {
        self.cells.get(idx)
    }

    /// Begin a block-style isolating context. Pushes a `Block` frame, sets
    /// `base` to the current stack length, and returns the previous `base`
    /// (which the caller must pass to [`Self::exit_block`] to restore).
    pub fn enter_block(&mut self) -> usize {
        let saved_base = self.base;
        self.base = self.cells.len();
        self.frames.push(Frame::Block {
            start: self.cells.len(),
            saved_below: Vec::new(),
        });
        saved_base
    }

    /// End a block-style context: restore `base`, drop body-local cells, and
    /// replay any displaced parent cells. Returns `(last_cell_at_end_of_body,
    /// body_stack_size)`.
    pub fn exit_block(&mut self, saved_base: usize) -> (Option<T>, usize) {
        self.base = saved_base;

        match self.frames.pop().expect("exit_block: no frame") {
            Frame::Block { start, saved_below } => {
                let body_stack_size = self.cells.len().saturating_sub(start);
                let result = self.cells.last().cloned();
                self.cells.truncate(start);
                self.cells.extend(saved_below.iter().rev().cloned());
                (result, body_stack_size)
            }
            Frame::IfElseBranch => {
                panic!("exit_block called but topmost frame is IfElseBranch")
            }
        }
    }

    /// Begin an ifelse branch. Cells are NOT isolated; the marker exists solely
    /// to forbid `Rebase` and to make pops transparent to enclosing blocks.
    #[inline]
    pub fn enter_ifelse_branch(&mut self) {
        self.frames.push(Frame::IfElseBranch);
    }

    /// End an ifelse branch.
    pub fn exit_ifelse_branch(&mut self) {
        if let Some(Frame::Block { .. }) = self.frames.last() {
            panic!("exit_ifelse_branch called but topmost frame is Block")
        }
    }

    /// Drains `cells[..base]` into the innermost enclosing `Block` frame's
    /// `saved_below`. Returns `Err` if `base > cells.len()` or if the
    /// immediately-topmost frame is an `IfElseBranch` (rebase is forbidden
    /// inside ifelse branches).
    pub fn rebase(&mut self) -> Result<(), CoreError> {
        if self.base > self.cells.len() {
            panic!("Base {} is greater than stack length {}, this should not happen", self.base, self.cells.len());
        }

        match self.frames.last_mut() {
            Some(Frame::Block { start, saved_below }) => {
                // Check that we aren't rebasing twice
                if self.base > *start {
                    return Err(RebaseError);
                }

                // Temporarily save the cells below `base`
                saved_below.extend(self.cells.drain(..self.base).rev());
                *start = 0;
                Ok(())
            }
            Some(Frame::IfElseBranch) | None => {
                Err(RebaseError) // Not rebase-able in IfElse or outside block
            }
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }
}

/// A single stack cell bundling a value with its security tag and per-cell
/// downgrade counters. Keeping all per-value metadata in one record means it
/// travels together automatically through pushes, pops, blocks, and rebases —
/// there is only ever one stack to keep consistent.
#[derive(Clone, Debug)]
pub struct Slot<Value, Tag> {
    value: Value,
    tag: Tag,
    counts: DowngradeCounts,
}

impl<Value, Tag> Slot<Value, Tag> {
    pub fn new(value: Value, tag: Tag) -> Self {
        Self {
            value,
            tag,
            counts: DowngradeCounts::default(),
        }
    }
}

/// A stack of [`Slot`]s providing value/tag/counter views over a single
/// underlying [`StackFrames`]. Because every cell carries its value, tag, and
/// downgrade counters together in one record, the three can never drift out of
/// sync — there is nothing to keep aligned by hand.
#[derive(Clone, Debug)]
pub struct PairedStack<Value: Copy, Tag: Copy> {
    stack: StackFrames<Slot<Value, Tag>>,
}

impl<Value: Copy, Tag: Copy> PairedStack<Value, Tag> {
    pub fn new() -> Self {
        Self {
            stack: StackFrames::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.len() == 0
    }

    /// Base of the innermost block (start of the current frame's own cells).
    pub fn base(&self) -> usize {
        self.stack.base
    }

    /// Values only, cloned out for inspection and error reporting.
    pub fn values(&self) -> Vec<Value> {
        self.stack.cells.iter().map(|slot| slot.value).collect()
    }

    /// Value of the top cell, if any.
    pub fn last_value(&self) -> Option<Value> {
        self.stack.cells.last().map(|slot| slot.value)
    }

    /// Tags only, cloned out for inspection and error reporting.
    pub fn tags(&self) -> Vec<Tag> {
        self.stack.cells.iter().map(|slot| slot.tag).collect()
    }

    /// Tag of the top cell, if any.
    pub fn last_tag(&self) -> Option<Tag> {
        self.stack.cells.last().map(|slot| slot.tag)
    }

    /// Per-cell downgrade counters, cloned out for inspection.
    pub fn counts(&self) -> Vec<DowngradeCounts> {
        self.stack
            .cells
            .iter()
            .map(|slot| slot.counts.clone())
            .collect()
    }

    /// Gets the value and tag at the given index, if it exists.
    pub fn get(&self, index: usize) -> Option<(Value, Tag)> {
        self.stack.get(index).map(|slot| (slot.value, slot.tag))
    }

    /// Value at `index`, if it exists.
    pub fn value_at(&self, index: usize) -> Option<Value> {
        self.stack.get(index).map(|slot| slot.value)
    }

    /// Tag at `index`, if it exists.
    pub fn tag_at(&self, index: usize) -> Option<Tag> {
        self.stack.get(index).map(|slot| slot.tag)
    }

    /// Increments the per-cell downgrade counter for `downgrader` at `index`,
    /// returning the new total (0 if `index` is out of range).
    pub fn bump_count(&mut self, index: usize, downgrader: &str) -> usize {
        self.stack
            .cells
            .get_mut(index)
            .map(|slot| slot.counts.bump(downgrader))
            .unwrap_or(0)
    }

    /// Pushes a value and tag together (with empty downgrade counters).
    pub fn push(&mut self, value: Value, tag: Tag) {
        self.stack.push(Slot::new(value, tag));
    }

    /// Pops the top cell, returning its value and tag (counters are discarded).
    pub fn pop(&mut self) -> Option<(Value, Tag)> {
        self.stack.pop().map(|slot| (slot.value, slot.tag))
    }

    pub fn enter_block(&mut self) -> usize {
        self.stack.enter_block()
    }

    pub fn exit_block(&mut self, saved_base: usize) -> (Option<(Value, Tag)>, usize) {
        let (slot, size) = self.stack.exit_block(saved_base);
        (slot.map(|slot| (slot.value, slot.tag)), size)
    }

    pub fn enter_ifelse_branch(&mut self) {
        self.stack.enter_ifelse_branch();
    }

    pub fn exit_ifelse_branch(&mut self) {
        self.stack.exit_ifelse_branch();
    }

    pub fn rebase(&mut self) -> Result<(), CoreError> {
        self.stack.rebase()
    }

    /// Read-only view of the underlying slots (used by the verifier's if/else
    /// branch merge, which combines values, tags, and counters cell-by-cell).
    pub fn cells(&self) -> &[Slot<Value, Tag>] {
        &self.stack.cells
    }

    /// Replaces the underlying slots, returning the previous contents.
    pub fn replace_cells(&mut self, cells: Vec<Slot<Value, Tag>>) -> Vec<Slot<Value, Tag>> {
        std::mem::replace(&mut self.stack.cells, cells)
    }

    /// Takes the underlying slots, leaving an empty stack behind.
    pub fn take_cells(&mut self) -> Vec<Slot<Value, Tag>> {
        std::mem::take(&mut self.stack.cells)
    }

    /// Overwrites the underlying slots.
    pub fn set_cells(&mut self, cells: Vec<Slot<Value, Tag>>) {
        self.stack.cells = cells;
    }

    pub fn set_values_for_unmonitored(&mut self, values: Vec<Value>, default_tag: Tag) {
        self.stack.cells = values
            .into_iter()
            .map(|value| Slot::new(value, default_tag))
            .collect();
    }
}

impl<Value: Copy, Tag: Copy> Default for PairedStack<Value, Tag> {
    fn default() -> Self {
        Self::new()
    }
}
