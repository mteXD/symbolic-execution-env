use log::{debug, error, warn};
use std::{fmt::Debug, rc::Rc};

use crate::{
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{self},
        IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
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
            IfElse(when_true, when_false) => {
                debug!("Entering if-else block...");
                self.evaluate_ifelse(when_true.clone(), when_false.clone()) // Cheap clone
            }
            AluFunction(instr, fun) => {
                debug!("Evaling: {:?}, fun: '{}'", instr, fun);
                self.evaluate_function(instr, fun)
            }
            AluIntrinsic(instr, arg) => {
                debug!("Evaling: {:?}, arg: {:?}", instr, arg);
                self.evaluate_intrinsic(instr, *arg)
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
        when_true: Rc<Instruction>,
        when_false: Rc<Instruction>,
    ) -> Result<(), Self::Error>;
    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> Result<(), Self::Error>;
    fn evaluate_intrinsic(
        &mut self,
        instr: &IntrinsicOp,
        arg: CellIndex,
    ) -> Result<(), Self::Error>;
}

// =============================================================================
// Shared frame-stack mechanics used by both Executor and Verifier.
//
// A `Frame` records the cell-stack boundary at the start of a nested context
// (block / function body / ifelse branch). When the body pops below `start`,
// the displaced parent cell is saved into `saved_below`; when `Rebase` drains
// the parent's cells, they too are saved. On exit, body-local cells are
// dropped and `saved_below` is replayed to restore the parent's stack.
// =============================================================================

#[derive(Clone, Debug)]
pub struct Frame<T> {
    pub start: usize,
    pub saved_below: Vec<T>,
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

    pub fn push(&mut self, value: T) {
        self.cells.push(value);
    }

    /// Pops the top cell. If popping reaches into the parent's cells (below the
    /// current frame's `start`), the popped value is saved for restoration on
    /// frame exit and the frame's `start` is decremented to keep accounting consistent.
    pub fn pop(&mut self) -> Option<T> {
        let popped = self.cells.pop()?;
        if let Some(frame) = self.frames.last_mut() {
            if self.cells.len() < frame.start {
                frame.saved_below.push(popped);
                frame.start -= 1;
            }
        }
        Some(popped)
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        self.cells.get(idx)
    }

    /// Begin a nested context. Pushes a fresh frame, sets `base` to the current
    /// stack length, and returns the previous `base` (which the caller must pass
    /// to [`exit`] to restore).
    pub fn enter(&mut self) -> usize {
        let saved_base = self.base;
        self.base = self.cells.len();
        self.frames.push(Frame {
            start: self.cells.len(),
            saved_below: Vec::new(),
        });
        saved_base
    }

    /// End the nested context: restore `base`, drop body-local cells, and replay
    /// any displaced parent cells. Returns `(last_cell_at_end_of_body, body_stack_size)`.
    pub fn exit(&mut self, saved_base: usize) -> (Option<T>, usize) {
        self.base = saved_base;
        let frame = self.frames.pop().expect("exit called without matching enter");
        let body_stack_size = self.cells.len().saturating_sub(frame.start);
        // Match legacy semantics: result is the top of the inherited+body stack.
        let result = self.cells.last().copied();
        self.cells.truncate(frame.start);
        self.cells.extend(frame.saved_below.iter().rev().copied());
        (result, body_stack_size)
    }

    /// Drains `cells[..base]` (the parent's cells visible to the current frame)
    /// into the frame's `saved_below`, so they can be restored on exit. Returns
    /// `Err` if `base > cells.len()`.
    pub fn rebase(&mut self) -> Result<(), ()> {
        if self.base > self.cells.len() {
            return Err(());
        }
        let drained: Vec<T> = self.cells.drain(..self.base).collect();
        if let Some(frame) = self.frames.last_mut() {
            frame.saved_below.extend(drained.into_iter().rev());
            frame.start = 0;
        }
        Ok(())
    }
}
