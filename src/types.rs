use std::{collections::HashMap, fmt::Debug};

use crate::instruction::{Instruction};

pub type Cell = u16;
pub type Immediate = i64;
pub type Address = usize;

#[derive(Debug, Clone, Default)]
pub struct FunctionData {
    function_table: HashMap<String, Address>,
}

impl FunctionData {
    pub fn new() -> Self {
        FunctionData::default()
    }

    pub fn insert(&mut self, name: String, instr_pc: Address) {
        self.function_table.insert(name, instr_pc);
    }

    pub fn get(&self, name: &str) -> Option<&Address> {
        self.function_table.get(name)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.function_table.contains_key(name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProgramData<'a> {
    program: &'a [Instruction],
    pc: Address,
}

impl<'a> ProgramData<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self { program, pc: 0 }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
    }

    pub fn get_pc(&self) -> Address {
        self.pc
    }

    pub fn get_current(&self) -> Option<&Instruction> {
        self.program.get(self.pc)
    }

    pub fn get_at(&self, pc: Address) -> Option<&[Instruction]> {
        self.program.get(pc..pc + 1)
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

trait Runnable {
    // A method that has a pre-execute function, a post-execute function, and a
    //
}
