use std::fmt::Debug;

use crate::{
    instruction::{
        FunctionOp,
        Instruction::{self},
        
    },
    types::{Address, FunctionData, ProgramData},
};

mod executor;
mod verifier;

#[derive(Debug, Clone)]
pub enum CoreError {
    StackUnderflow,
    InvalidCell,
    DivisionByZero,
    NoSavedCells,
    RebaseError,
    NoRebasedCells,
    FunctionRedefinition,
    FunctionUndefined,
    FunctionCallError,
    InstructionError(String),
    OtherError(String),
    ProgramNotLoaded,
}

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

    pub fn function_exists(&self, name: &str) -> bool {
        self.function_data.contains_key(name)
    }

    pub fn function_get(&self, name: &str) -> Result<&Address, CoreError> {
        self.function_data
            .get(name)
            .ok_or(CoreError::FunctionUndefined)
    }

    pub fn function_insert(&mut self, name: String, instr_pc: Address) {
        self.function_data.insert(name, instr_pc);
    }

    pub fn function_insert_current(&mut self, name: String) {
        let instr_pc = self.program_data.get_pc();
        self.function_insert(name, instr_pc);
    }

    pub fn load_program(&mut self, program: &'a [Instruction]) {
        self.program_data = ProgramData::new(program)
    }

    pub fn sub_machine(&mut self, program: &'a [Instruction]) -> Self {
        // TODO: Optimize
        let mut sub_machine = Self::new(program);
        sub_machine.function_data = self.function_data.clone();
        sub_machine
    }

    fn get_status(&self) -> String {
        // Multiline
        format!("PC: {}", self.program_data.get_pc())
    }

    pub fn get_current_instruction(&self) -> Option<&Instruction> {
        self.program_data.get_current()
    }

    // pub fn run(&mut self) -> Result<Option<&i64>, CoreError> {
    //     while let Some(instr) = self.program_data.next() {
    //         instr.eval(self).map_err(|e| {
    //             // eprintln!(
    //             //     "Error executing instruction {:?}. Error: {:?} | cells: {:?}",
    //             //     instr, e, self.cells
    //             // );
    //             e
    //         })?;
    //
    //     }
    //
    //     // TODO: Move this return into Executor
    //     // Ok(self.cells.last())
    //     Ok(None)
    // }

    pub fn common_function_logic(&mut self, arg: &str) -> Result<(), CoreError> {
        if self.function_exists(&arg) {
            return Err(CoreError::FunctionRedefinition);
        }

        let mut definitions = Vec::new();
        definitions.push(arg);

        while let Some(Instruction::AluFunction(FunctionOp::FunctionDefine, name)) = self.next() {
            definitions.push(name);
        }

        definitions
            .iter()
            .map(|name| self.function_insert_current(String::from(*name)))
            .for_each(drop);

        Ok(())
    }
}

// impl Default for CoreMachine<'_> {
//     fn default() -> Self {
//         Self::new(&[])
//     }
// }

impl<'a> Iterator for CoreMachine<'a> {
    type Item = &'a Instruction;

    fn next(&mut self) -> Option<Self::Item> {
        self.program_data.next()
    }
}

// #[cfg(test)]
// pub mod machine_tests;
