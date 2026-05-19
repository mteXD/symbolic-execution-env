use log::{debug, error, warn};
use std::fmt::Debug;

use crate::{
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{self},
        IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    types::{
        Cell, CellIndex, FdEntry, FunctionData, FunctionDataError, Immediate, Input, Output,
        ProgramData, ProgramDataError,
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
pub struct CoreMachine<'a> {
    pub function_data: FunctionData,
    pub program_data: ProgramData<'a>,
    pub output: Output,
    pub input: Input,
}

impl<'a> CoreMachine<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
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

    pub fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        Self {
            function_data: self.function_data.clone(), // PERF: clone()
            program_data: ProgramData::new(program),
            output: self.output.clone(),
            input: self.input.clone(),
        }
    }

    pub fn common_function_logic(&mut self, arg: &str) -> CoreResult<()> {
        let mut definitions = Vec::new();

        while let Some(Instruction::AluFunction(FunctionOp::FunctionDefine, name)) = self.next() {
            debug!("Found consecutive definition: '{}'", name);
            definitions.push(name);
        }

        match self.program_data.get_current() {
            Ok(Instruction::Block(_)) => {}
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
            self.function_insert(name.to_owned(), FdEntry::Str(arg.to_owned()))?; // PERF: to_owned()
        }

        Ok(())
    }
}

impl<'a> Iterator for CoreMachine<'a> {
    type Item = &'a Instruction;

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
                self.evaluate_block(instrs)
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
    fn evaluate_block(&mut self, instrs: &[Instruction]) -> Result<(), Self::Error>;
    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> Result<(), Self::Error>;
    fn evaluate_intrinsic(
        &mut self,
        instr: &IntrinsicOp,
        arg: CellIndex,
    ) -> Result<(), Self::Error>;
}
