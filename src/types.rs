use std::{collections::HashMap, fmt::Debug};

use crate::instruction::Instruction;

pub type Cell = u16;
pub type Immediate = i64;
pub type Address = usize;

#[derive(Debug, Clone)]
pub enum FunctionDataError {
    FunctionRedefinition(String),
    FunctionUndefined(String),
}
use FunctionDataError::*;

#[derive(Debug, Clone, Default)]
pub struct FunctionData {
    function_table: HashMap<String, Instruction>,
}

impl FunctionData {
    pub fn new() -> Self {
        FunctionData::default()
    }

    pub fn insert(&mut self, name: String, instr: Instruction) -> Result<(), FunctionDataError> {
        if self.function_table.contains_key(&name) {
            return Err(FunctionRedefinition(name));
        }

        self.function_table.insert(name, instr);

        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<&Instruction, FunctionDataError> {
        self.function_table
            .get(name)
            .ok_or(FunctionUndefined(name.to_owned()))
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.function_table.contains_key(name)
    }
}

#[derive(Debug, Clone)]
pub enum ProgramDataError {
    InvalidPC,
}
use ProgramDataError::*;

#[derive(Debug, Clone, Default)]
pub struct ProgramData<'a> {
    program: &'a [Instruction],
    pc: Address,
}

impl<'a> ProgramData<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self {
            program,
            pc: 0,
        }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
    }

    pub fn get_pc(&self) -> Address {
        self.pc
    }

    pub fn get_current(&self) -> Result<&Instruction, ProgramDataError> {
        self.program.get(self.pc).ok_or(InvalidPC)
    }

    pub fn get_at(&self, pc: Address) -> Result<&Instruction, ProgramDataError> {
        self.program.get(pc).ok_or(InvalidPC)
    }
}

impl<'a> Iterator for ProgramData<'a> {
    type Item = &'a Instruction;

    fn next(&mut self) -> Option<Self::Item> {
        let instr = self.program.get(self.pc)?;
        self.pc += 1;
        Some(instr)
    }
}
