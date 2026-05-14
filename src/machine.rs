use log::{debug, error, warn};
use std::fmt::Debug;

use crate::{
    instruction::{
        FunctionOp,
        Instruction::{self},
    },
    types::{FunctionData, FunctionDataError, Input, Output, ProgramData, ProgramDataError},
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

type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone)]
pub struct CoreMachine<'a> {
    function_data: FunctionData,
    program_data: ProgramData<'a>,
    output: Output,
    input: Input,
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

    pub fn function_get(&self, name: &str) -> Result<&Instruction> {
        Ok(self.function_data.get(name)?)
    }

    pub fn function_insert(&mut self, name: String, instr: Instruction) -> Result<()> {
        Ok(self.function_data.insert(name, instr)?)
    }

    pub fn function_insert_current(&mut self, name: String) -> Result<()> {
        let current = self.program_data.get_current()?;

        debug!("Function '{}' will point to {:?}", name, current);

        self.function_insert(name, current.to_owned()) // PERF: to_owned()
    }

    pub fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        Self {
            function_data: self.function_data.clone(), // PERF: clone()
            program_data: ProgramData::new(program),
            output: self.output.clone(),
            input: self.input.clone(),
        }
    }

    pub fn common_function_logic(&mut self, arg: &str) -> Result<()> {
        let mut definitions = Vec::new();
        definitions.push(arg);

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

        for name in definitions {
            self.function_insert_current(name.to_owned())?;
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
