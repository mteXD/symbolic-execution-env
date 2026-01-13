use crate::{
    instructions::{self, Instruction, Operator},
    types::{MachineError, Register},
};

pub struct Machine {
    cells: Vec<i64>,
}

impl Machine {
    pub fn new() -> Self {
        Machine { cells: Vec::new() }
    }

    pub(crate) fn push(&mut self, value: i64) -> Result<(), MachineError> {
        self.cells.push(value);
        Ok(())
    }

    pub(crate) fn pop(&mut self) -> Result<i64, MachineError> {
        if let Some(value) = self.cells.pop() {
            Ok(value)
        } else {
            Err(MachineError::StackUnderflow)
        }
    }

    pub(crate) fn multi_pop(&mut self, n: Register) -> Result<(), MachineError> {
        for _ in 0..n {
            self.pop()?;
        }
        Ok(())
    }

    pub(crate) fn read(&self, reg: Register) -> Result<&i64, MachineError> {
        match self.cells.get::<usize>(reg.into()) {
            Some(value) => Ok(value),
            None => Err(MachineError::InvalidRegister),
        }
    }

    fn evaluate_instruction(&mut self, instruction: &Instruction) -> Result<(), MachineError> {
        use instructions::Instruction::*;

        match instruction {
            AluNullary(nullop) => nullop.eval(self, ())?,
            AluUnaryImm(unop_imm, imm) => unop_imm.eval(self, *imm)?,
            AluUnaryReg(unop_reg, reg) => unop_reg.eval(self, *reg)?,
            AluBinary(binop, reg1, reg2) => binop.eval(self, (*reg1, *reg2))?,
        }

        Ok(())
    }

    pub fn run(&mut self, program: &[Instruction]) -> Result<Option<&i64>, MachineError> {
        self.run_until(program, program.len())
    }

    pub fn run_until(
        &mut self,
        program: &[Instruction],
        limit: usize,
    ) -> Result<Option<&i64>, MachineError> {
        program
            .iter()
            .take(limit)
            .try_for_each(|instruction| self.evaluate_instruction(instruction))?;

        Ok(self.cells.last())
    }
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        tests::add_instr,
        instructions::{
            Instruction::{AluUnaryImm, AluUnaryReg},
            UnaryOpImm,
            UnaryOpReg,
        },
        machine::Machine,
        types::MachineError,
    };

    #[test]
    fn test_push_pop() {
        let mut machine = Machine::default();
        let prog = vec![
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
        ];
        machine.run(&prog).unwrap();
        assert_eq!(machine.cells[0], 1);
        assert_eq!(machine.cells[1], 2);
        assert_eq!(machine.cells[2], 3);

        let prog = vec![add_instr!(R Pop, 1)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, Some(&2));

        let prog = vec![add_instr!(R Pop, 2)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, None);

        let prog = vec![add_instr!(R Pop, 1)];
        let result = machine.run(&prog);
        assert!(matches!(result, Err(MachineError::StackUnderflow)));
    }

    #[test]
    fn test_read() {
        let mut machine = Machine::default();
        let program = vec![
            add_instr!(Push, 100),
            add_instr!(Push, 200),
            add_instr!(R Read, 0),
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&100));
        assert_eq!(machine.cells[0], 100);
        assert_eq!(machine.cells[1], 200);
    }
}
