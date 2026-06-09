use log::{debug, error, warn};
use std::{fmt::Debug, rc::Rc};

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

#[derive(Debug, Clone)]
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
    pub function_data: FunctionData<Tag>,
    pub program_data: ProgramData<Tag>,
    pub output: Output,
    pub input: Input,
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

    pub fn common_function_logic(&mut self, function_name: &str) -> CoreResult<()> {
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

        for alias in aliases {
            self.function_insert(alias, FdEntry::Str(function_name.clone()))?;
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

#[derive(Clone, Debug)]
pub enum Frame<T> {
    Block { start: usize, saved_below: Vec<T> },
    IfElseBranch,
}

#[derive(Clone, Debug, Default)]
pub struct StackFrames<T: Copy> {
    pub cells: Vec<T>,
    pub base: usize,
    pub frames: Vec<Frame<T>>,
}

impl<T: Copy> StackFrames<T> {
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
                        saved_below.push(popped);
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
                let result = self.cells.last().copied();
                self.cells.truncate(start);
                self.cells.extend(saved_below.iter().rev().copied());
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
        match self.frames.pop().expect("exit_ifelse_branch: no frame") {
            Frame::IfElseBranch => {}
            Frame::Block { .. } => {
                panic!("exit_ifelse_branch called but topmost frame is Block")
            }
        }
    }

    /// Drains `cells[..base]` into the innermost enclosing `Block` frame's
    /// `saved_below`. Returns `Err` if `base > cells.len()` or if the
    /// immediately-topmost frame is an `IfElseBranch` (rebase is forbidden
    /// inside ifelse branches).
    pub fn rebase(&mut self) -> Result<(), CoreError> {
        if self.base > self.cells.len() {
            return Err(RebaseError);
        }

        match self.frames.last_mut() {
            Some(Frame::IfElseBranch) => Err(RebaseError), // Not rebase-able
            Some(Frame::Block { start, saved_below }) => {
                // Temporarily save the cells below `base`
                saved_below.extend(self.cells.drain(..self.base).rev());
                *start = 0;
                Ok(())
            }
            None => {
                // No frame: legacy behavior was to drain (and drop) the cells.
                self.cells.drain(..self.base);
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct PairedStack<Value: Copy, Tag: Copy> {
    values: StackFrames<Value>,
    tags: StackFrames<Tag>,
}

/// Saved base positions for the value and tag sides of a [`PairedStack`].
#[derive(Clone, Copy, Debug)]
pub struct PairedBlockBases {
    values: usize,
    tags: usize,
}

impl<Value: Copy, Tag: Copy> PairedStack<Value, Tag> {
    pub fn new() -> Self {
        Self {
            values: StackFrames::new(),
            tags: StackFrames::new(),
        }
    }

    /// Debug assertion that checks that value and tag stacks are of the same length.
    fn debug_assert_synchronized(&self) {
        debug_assert_eq!(self.values.cells.len(), self.tags.cells.len());
        debug_assert_eq!(self.values.base, self.tags.base);
        debug_assert_eq!(self.values.frames.len(), self.tags.frames.len());
    }

    pub fn values(&self) -> &StackFrames<Value> {
        &self.values
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags.cells
    }

    /// Gets the value and tag at the given index, if it exists.
    pub fn get(&self, index: usize) -> Option<(Value, Tag)> {
        self.values
            .get(index)
            .copied()
            .zip(self.tags.get(index).copied())
    }

    /// Pushes a value and tag together.
    pub fn push(&mut self, value: Value, tag: Tag) {
        self.values.push(value);
        self.tags.push(tag);
        self.debug_assert_synchronized();
    }

    pub fn pop(&mut self) -> Option<(Value, Tag)> {
        let value = self.values.pop()?;
        let tag = self
            .tags
            .pop()
            .expect("paired tag stack is shorter than value stack");
        self.debug_assert_synchronized();
        Some((value, tag))
    }

    pub fn enter_block(&mut self) -> PairedBlockBases {
        let saved_bases = PairedBlockBases {
            values: self.values.enter_block(),
            tags: self.tags.enter_block(),
        };
        self.debug_assert_synchronized();
        saved_bases
    }

    pub fn exit_block(&mut self, saved_bases: PairedBlockBases) -> (Option<(Value, Tag)>, usize) {
        let (value, value_size) = self.values.exit_block(saved_bases.values);
        let (tag, tag_size) = self.tags.exit_block(saved_bases.tags);
        assert_eq!(value_size, tag_size, "paired block stacks diverged");
        self.debug_assert_synchronized();
        (value.zip(tag), value_size)
    }

    pub fn enter_ifelse_branch(&mut self) {
        self.values.enter_ifelse_branch();
        self.tags.enter_ifelse_branch();
        self.debug_assert_synchronized();
    }

    pub fn exit_ifelse_branch(&mut self) {
        self.values.exit_ifelse_branch();
        self.tags.exit_ifelse_branch();
        self.debug_assert_synchronized();
    }

    pub fn rebase(&mut self) -> Result<(), CoreError> {
        self.values.rebase()?;
        self.tags
            .rebase()
            .expect("paired tag stack rejected a value-stack rebase");
        self.debug_assert_synchronized();
        Ok(())
    }

    pub fn set_values_for_unmonitored(&mut self, values: Vec<Value>, default_tag: Tag) {
        self.values.cells = values;
        self.tags.cells = vec![default_tag; self.values.cells.len()];
        self.debug_assert_synchronized();
    }
}

impl<Value: Copy, Tag: Copy> Default for PairedStack<Value, Tag> {
    fn default() -> Self {
        Self::new()
    }
}
