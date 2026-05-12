use std::fmt::Debug;
use log::{debug, error, warn};

use crate::{
    instruction::{
        FunctionOp,
        Instruction::{self},
    },
    types::{FunctionData, FunctionDataError, ProgramData, ProgramDataError},
};

mod executor;
mod verifier;

#[derive(Debug, Clone)]
pub enum CoreError {
    FunctionDataError(FunctionDataError),
    ProgramDataError(ProgramDataError),
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
}

impl<'a> CoreMachine<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self {
            function_data: FunctionData::default(),
            program_data: ProgramData::new(program),
        }
    }

    pub fn function_get(&self, name: &str) -> Result<&Instruction> {
        Ok(self.function_data.get(name)?)
    }

    pub fn function_insert(&mut self, name: String, instr: Instruction) -> Result<()> {
        Ok(self.function_data.insert(name, instr)?)
    }

    pub fn function_insert_current(&mut self, name: String) -> Result<()> {
        // WARN: Avoid going back to preceeding PC
        // This happens because the PC is initially 0, and calling next() returns
        // the current instruction and then increments the PC. 
        let instr_pc = self.program_data.get_pc();

        debug!("Inserting function '{}' at PC {}", name, instr_pc);
        debug!("That is instruction: {:?}", self.program_data.get_at(instr_pc)?);

        // TODO: to_owned copies data, find a way to use references without the borrow checker
        // complaining.
        self.function_insert(name, self.program_data.get_at(instr_pc)?.to_owned())
    }

    pub fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        // TODO: Optimize this function_data clone()
        Self {
            function_data: self.function_data.clone(),
            program_data: ProgramData::new(program),
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
            Ok(Instruction::Block(_)) => {
            }
            Ok(instr) => {
                warn!("Expected block after function definitions, but found instruction: {:?}", instr);
            }
            Err(err) => {
                error!("Error while fetching instruction for function definition: {:?}", err);
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
