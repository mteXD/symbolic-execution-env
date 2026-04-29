use std::fmt::Debug;

use crate::{
    add_instr,
    instruction::{
        FunctionOp,
        Instruction::{self},
    },
    make_block,
    types::{Address, FunctionData, FunctionDataError, ProgramData, ProgramDataError},
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
        let instr_pc = self.program_data.get_pc() - 1;  // WARN: Avoid going back to preceeding PC

        eprintln!("Inserting function '{}' at PC {}", name, instr_pc);
        eprintln!("That is instruction: {:?}", self.program_data.get_at(instr_pc)?);

        // TODO: to_owned copies data, find a way to use references without the borrow checker
        // complaining.
        self.function_insert(name, self.program_data.get_at(instr_pc)?.to_owned())
    }

    pub fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        // TODO: Optimize
        let mut sub_machine = Self::new(program);
        sub_machine.function_data = self.function_data.clone();
        sub_machine
    }

    fn get_status(&self) -> String {
        // Multiline
        format!("PC: {}", self.program_data.get_pc())
    }

    pub fn get_current_instruction(&self) -> Result<&Instruction> {
        Ok(self.program_data.get_current()?)
    }

    pub fn get_instruction_at(&self, pc: Address) -> Result<&Instruction> {
        Ok(self.program_data.get_at(pc)?)
    }

    pub fn common_function_logic(&mut self, arg: &str) -> Result<()> {
        // TODO: Remove String::from and use &str instead (or to_owned)
        let mut definitions = Vec::new();
        definitions.push(arg);

        while let Some(Instruction::AluFunction(FunctionOp::FunctionDefine, name)) = self.next() {
            definitions.push(name);
        }

        for name in definitions {
            self.function_insert_current(String::from(name))?;
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
