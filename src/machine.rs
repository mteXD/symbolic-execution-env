use log::{debug, error, warn};
use std::{fmt::Debug, rc::Rc};

use crate::{
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{self},
        IntrinsicArg, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    }, machine::CoreError::RebaseError, types::{
        CellIndex, FdEntry, FunctionData, FunctionDataError, Immediate, Input, Output, ProgramData,
        ProgramDataError,
    }
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
pub struct CoreMachine {
    pub function_data: FunctionData,
    pub program_data: ProgramData,
    pub output: Output,
    pub input: Input,
}

impl CoreMachine {
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        Self {
            function_data: FunctionData::default(),
            program_data: ProgramData::new(program),
            output: Output::Stdout,
            input: Input::Stdin,
        }
    }

    pub fn function_get(&self, name: &str) -> CoreResult<&Instruction> {
        Ok(self.function_data.get(name)?)
    }

    pub fn function_insert(&mut self, name: String, entry: FdEntry) -> CoreResult<()> {
        Ok(self.function_data.insert(name, entry)?)
    }

    pub fn function_insert_current(&mut self, name: String) -> CoreResult<()> {
        let current = self.program_data.get_current()?;

        debug!("Function '{}' will point to {:?}", name, current);

        self.function_insert(name, FdEntry::Inst(current.to_owned())) // PERF: to_owned()
    }

    pub fn common_function_logic(&mut self, arg: &str) -> CoreResult<()> {
        let mut definitions: Vec<String> = Vec::new();

        while let Some(Instruction::AluFunction(FunctionOp::FunctionDefine, name)) = self.next() {
            debug!("Found consecutive definition: '{}'", name);
            definitions.push(name);
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

        self.function_insert_current(arg.to_owned())?;

        for name in definitions {
            self.function_insert(name, FdEntry::Str(arg.to_owned()))?; // PERF: to_owned()
        }

        Ok(())
    }
}

impl Iterator for CoreMachine {
    type Item = Instruction;

    fn next(&mut self) -> Option<Self::Item> {
        self.program_data.next()
    }
}

trait Evaluate {
    type Error;

    fn evaluate_instruction(&mut self, instr: &Instruction) -> Result<(), Self::Error> {
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
        instr: &UnaryOpImm,
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
    fn evaluate_block(&mut self, instrs: Rc<[Instruction]>) -> Result<(), Self::Error>;
    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction>,
        when_false: Rc<Instruction>,
    ) -> Result<(), Self::Error>;
    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> Result<(), Self::Error>;
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
    /// (which the caller must pass to [`exit_block`] to restore).
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
