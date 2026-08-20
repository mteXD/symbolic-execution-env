use log::{debug, warn};
use std::{fmt::Debug, rc::Rc};

use crate::{
    instruction::{
        BinaryOp,
        Instruction::{self},
        NullaryOp, UnaryOpCell, UnaryOpCellAmnt, UnaryOpImm, UnaryOpString,
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
    downgrader_data: FunctionData<Tag>,
    program_data: ProgramData<Tag>,
    output: Output,
    input: Input,
}

impl<Tag: Clone + Debug> CoreMachine<Tag> {
    pub fn new(program: impl Into<Rc<[Instruction<Tag>]>>) -> Self {
        Self {
            function_data: FunctionData::default(),
            downgrader_data: FunctionData::default(),
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

    pub fn downgrader_get(&self, name: &str) -> CoreResult<&Instruction<Tag>> {
        self.downgrader_data.get(name).map_err(Into::into)
    }

    pub fn downgrader_insert(&mut self, name: String, entry: FdEntry<Tag>) -> CoreResult<()> {
        self.downgrader_data.insert(name, entry)?;
        Ok(())
    }

    /// Registers the instruction immediately following a `FunctionDefine` as
    /// the body of `function_name`. A consecutive `FunctionDefine` or a
    /// definition at the end of the program returns `FunctionMissingBody`.
    pub fn common_function_logic(&mut self, function_name: &str) -> CoreResult<()> {
        self.common_definition_logic(function_name, false)
    }

    /// Registers a downgrader separately from ordinary function data.
    pub fn common_downgrader_logic(&mut self, function_name: &str) -> CoreResult<()> {
        self.common_definition_logic(function_name, true)
    }

    fn common_definition_logic(
        &mut self,
        function_name: &str,
        is_downgrader: bool,
    ) -> CoreResult<()> {
        use Instruction::AluUnaryString;
        use UnaryOpString::FunctionDefine;

        let current = match self.next() {
            Some(AluUnaryString(FunctionDefine, _)) | None => {
                return Err(
                    FunctionDataError::FunctionMissingBody(function_name.to_owned()).into(),
                );
            }
            Some(current) => current,
        };

        if !matches!(current, Instruction::Block(_)) {
            warn!(
                "Expected block after function definitions, but found instruction: {:?}",
                current
            );
        }

        let function_name = function_name.to_owned();
        debug!("Function '{}' will point to {:?}", function_name, current);
        if is_downgrader {
            self.downgrader_insert(function_name, FdEntry::Inst(current))?;
        } else {
            self.function_insert(function_name, FdEntry::Inst(current))?;
        }

        Ok(())
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
            AluUnaryCellAmnt(instr, amount) => {
                debug!("Evaling: {:?}, amount: {:?}", instr, amount);
                self.evaluate_alu_unary_cell_amnt(instr, *amount)
            }
            AluUnaryString(instr, name) => {
                debug!("Evaling: {:?}, name: '{}'", instr, name);
                self.evaluate_alu_unary_string(instr, name)
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
    fn evaluate_alu_unary_cell_amnt(
        &mut self,
        instr: &UnaryOpCellAmnt,
        amount: CellIndex,
    ) -> Result<(), Self::Error>;
    fn evaluate_alu_unary_string(
        &mut self,
        instr: &UnaryOpString,
        name: &str,
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

#[derive(Clone, Debug)]
pub enum Frame<V, T> {
    Block {
        start: usize,
        saved_below: Vec<Cell<V, T>>,
    },
    IfElseBranch,
}

#[derive(Clone, Debug)]
pub struct Stack<V, T> {
    cells: Vec<Cell<V, T>>,
    base: usize,
    frames: Vec<Frame<V, T>>,
}

impl<V: Clone, T: Clone> Default for Stack<V, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone, T: Clone> Stack<V, T> {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            base: 0,
            frames: Vec::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, value: Cell<V, T>) {
        self.cells.push(value);
    }

    /// Pops the top cell. If the resulting `cells.len()` drops below the start
    /// of the innermost enclosing `Block` frame, the popped value is saved
    /// against that block (and its `start` is decremented). `IfElseBranch`
    /// frames are transparent to this accounting.
    pub fn pop(&mut self) -> Option<Cell<V, T>> {
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
    pub fn get(&self, idx: usize) -> Option<&Cell<V, T>> {
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
    pub fn exit_block(&mut self, saved_base: usize) -> (Option<Cell<V, T>>, usize) {
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

    /// End an ifelse branch, popping its marker frame.
    pub fn exit_ifelse_branch(&mut self) {
        match self.frames.pop() {
            Some(Frame::IfElseBranch) => (),
            _ => panic!("exit_ifelse_branch called but topmost frame is not IfElseBranch"),
        }
    }

    /// Drains `cells[..base]` into the innermost enclosing `Block` frame's
    /// `saved_below`. Returns `Err` if `base > cells.len()` or if the
    /// immediately-topmost frame is an `IfElseBranch` (rebase is forbidden
    /// inside ifelse branches).
    pub fn rebase(&mut self) -> Result<(), CoreError> {
        if self.base > self.cells.len() {
            panic!(
                "Base {} is greater than stack length {}, this should not happen",
                self.base,
                self.cells.len()
            );
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

    /// A "getter" method for cells' length
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// A "getter" method for cells' is_empty method
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// A "getter" method for base
    pub fn base(&self) -> usize {
        self.base
    }
}

impl<V: Copy, T: Copy> Stack<V, T> {
    /// Values only, cloned out for inspection and error reporting.
    pub fn values(&self) -> Vec<V> {
        self.cells.iter().map(|slot| slot.value).collect()
    }

    /// Value of the top cell, if any.
    pub fn last_value(&self) -> Option<V> {
        self.cells.last().map(|slot| slot.value)
    }

    /// Tags only, cloned out for inspection and error reporting.
    pub fn tags(&self) -> Vec<T> {
        self.cells.iter().map(|slot| slot.tag).collect()
    }

    /// Tag of the top cell, if any.
    pub fn last_tag(&self) -> Option<T> {
        self.cells.last().map(|slot| slot.tag)
    }

    /// Value at `index`, if it exists.
    pub fn value_at(&self, index: usize) -> Option<V> {
        self.cells.get(index).map(|slot| slot.value)
    }

    /// Tag at `index`, if it exists.
    pub fn tag_at(&self, index: usize) -> Option<T> {
        self.cells.get(index).map(|slot| slot.tag)
    }

    /// Read-only view of the underlying slots.
    pub fn slots(&self) -> &[Cell<V, T>] {
        &self.cells
    }

    /// Replaces the underlying slots, returning the previous contents.
    pub fn replace_slots(&mut self, slots: Vec<Cell<V, T>>) -> Vec<Cell<V, T>> {
        std::mem::replace(&mut self.cells, slots)
    }

    /// Takes the underlying slots, leaving an empty stack behind.
    pub fn take_slots(&mut self) -> Vec<Cell<V, T>> {
        std::mem::take(&mut self.cells)
    }

    /// Overwrites the underlying slots.
    pub fn set_slots(&mut self, slots: Vec<Cell<V, T>>) {
        self.cells = slots;
    }
}

/// Converts a `ReadReverse` offset into a normal cell index, like Python's
/// negative indexing: offset 0 is the top cell. Returns `None` if the offset
/// reaches below the bottom of the stack. Used by both runners.
fn reverse_index(stack_len: usize, reverse_offset: CellIndex) -> Option<CellIndex> {
    let last_index = CellIndex::try_from(stack_len).ok()?.checked_sub(1)?;
    last_index.checked_sub(reverse_offset)
}

/// A single stack cell, containing a value and a tag.
#[derive(Clone, Debug)]
pub struct Cell<V, T> {
    value: V,
    tag: T,
}

impl<V, T> Cell<V, T> {
    pub fn new(value: V, tag: T) -> Self {
        Self { value, tag }
    }
}
