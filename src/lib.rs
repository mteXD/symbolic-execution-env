// A virtual machine that's a hybrid between a register-based and stack-based architecture.
// Registers are temporarily represented as i64 value. These "registers" are referred to as "cells"
// in the code.
// It has a push instruction - it places a value to the next available cell.
// This is similar to Single Static Assignment (SSA) form in compilers.
// Pop doesn't have to exist for reading purposes, as we can read directly from available cell.
// However, pop can be used to free up cells when needed.

use std::fmt::Debug;

pub enum Instruction {
    // Basic instructions = memory instructions
    Push(i64),   // Push a new value into the next available cell
    Pop(usize),  // Pop n cells to free them up
    Read(usize), // Read from a specific cell; cell must exist
    Nop,
    // Arithmetic instructions
    Add,
    Mul,
    Div,
    // Bitwise instructions
    And,
    Or,
    Not,
    Xor,
    // Shifting
    ShiftLeftLogical,
    ShiftRightLogical,
    ShiftRightArithmetic,
    // Comparison (set); Good enough for early stage of development
    SetEqual,
    SetNotEqual,
    SetLessThan,
    SetLessThanOrEqual,
    SetGreaterThan,
    SetGreaterThanOrEqual,
}

pub struct Machine {
    // pc_register: usize,
    cells: Vec<i64>,
    cells_amount: usize,
    next_cell: usize,
    program: Vec<Instruction>,
}

// #[derive(Debug)]
pub enum MachineError {
    StackOverflow,
    StackUnderflow,
    InvalidRegister,
    DivisionByZero,
}

impl Debug for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MachineError::StackOverflow => write!(f, "Stack Overflow"),
            MachineError::StackUnderflow => write!(f, "Stack Underflow"),
            MachineError::InvalidRegister => write!(f, "Invalid Register"),
            MachineError::DivisionByZero => write!(f, "Division By Zero"),
        }
    }
}

fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

impl Machine {
    pub fn new(cells_amount: usize, program: Vec<Instruction>) -> Self {
        Machine {
            // pc_register: 0,
            // cells: Vec::with_capacity(cells_amount),
            cells: Vec::new(),
            cells_amount,
            next_cell: 0,
            program,
        }
    }

    pub fn load_program(&mut self, program: Vec<Instruction>) {
        self.program = program;
    }

    fn push(&mut self, value: i64) -> Result<(), MachineError> {
        self.cells.push(value);
        self.next_cell += 1;
        if self.next_cell > self.cells_amount {
            return Err(MachineError::StackOverflow);
        }
        Ok(())
    }

    fn pop(&mut self) -> Result<i64, MachineError> {
        if let Some(value) = self.cells.pop() {
            self.next_cell -= 1;
            Ok(value)
        } else {
            Err(MachineError::StackUnderflow)
        }
    }

    fn multi_pop(&mut self, n: usize) -> Result<(), MachineError> {
        if n > self.next_cell {
            return Err(MachineError::StackUnderflow);
        }
        for _ in 0..n {
            self.pop()?;
        }
        Ok(())
    }

    fn evaluate_instruction(&mut self, instruction: &Instruction) -> Result<(), MachineError> {
        match instruction {
            Instruction::Push(value) => {
                self.push(*value)?;
            }
            Instruction::Pop(n) => {
                self.multi_pop(*n)?;
            }
            Instruction::Read(reg) => {
                if *reg >= self.next_cell {
                    return Err(MachineError::InvalidRegister);
                }
                self.push(self.cells[*reg])?;
            }
            Instruction::Add => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a + b)?;
            }
            Instruction::Mul => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a * b)?;
            }
            Instruction::Div => {
                let b = self.pop()?;
                let a = self.pop()?;
                if b == 0 {
                    return Err(MachineError::DivisionByZero);
                }
                self.push(a / b)?;
            }
            Instruction::And => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a & b)?;
            }
            Instruction::Or => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a | b)?;
            }
            Instruction::Not => {
                let a = self.pop()?;
                self.push(!a)?;
            }
            Instruction::Xor => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a ^ b)?;
            }
            Instruction::ShiftLeftLogical => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a << b)?;
            }
            Instruction::ShiftRightLogical => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(((a as u64) >> b) as i64)?;
            }
            Instruction::ShiftRightArithmetic => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a >> b)?;
            }
            Instruction::SetEqual => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(bool_to_i64(a == b))?;
            }
            Instruction::SetNotEqual => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(bool_to_i64(a != b))?;
            }
            Instruction::SetLessThan => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(bool_to_i64(a < b))?;
            }
            Instruction::SetLessThanOrEqual => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(bool_to_i64(a <= b))?;
            }
            Instruction::SetGreaterThan => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(bool_to_i64(a > b))?;
            }
            Instruction::SetGreaterThanOrEqual => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(bool_to_i64(a >= b))?;
            }
            Instruction::Nop => {}
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
        Machine::new(64, Vec::new()) // Default to 64 cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut machine = Machine::default();
        let prog = vec![
            Instruction::Push(1),
            Instruction::Push(2),
            Instruction::Push(3),
        ];
        machine.run(&prog).unwrap();
        assert_eq!(machine.cells[0], 1);
        assert_eq!(machine.cells[1], 2);
        assert_eq!(machine.cells[2], 3);
        assert_eq!(machine.next_cell, 3);

        let prog = vec![Instruction::Pop(1)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, Some(&2));
        assert_eq!(machine.next_cell, 2);

        let prog = vec![Instruction::Pop(2)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, None);
        assert_eq!(machine.next_cell, 0);

        let prog = vec![Instruction::Pop(1)];
        let result = machine.run(&prog);
        assert!(matches!(result, Err(MachineError::StackUnderflow)));
    }

    #[test]
    fn test_read() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(100),
            Instruction::Push(200),
            Instruction::Read(0),
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&100));
        assert_eq!(machine.next_cell, 3);
        assert_eq!(machine.cells[0], 100);
        assert_eq!(machine.cells[1], 200);
    }

    #[test]
    fn test_arith() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(10),
            Instruction::Push(-30),
            Instruction::Add,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&-20));

        let program = vec![
            Instruction::Push(10),
            Instruction::Push(3),
            Instruction::Mul,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&30));

        let program = vec![
            Instruction::Push(10),
            Instruction::Push(2),
            Instruction::Div,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&5));

        let program = vec![
            Instruction::Push(10),
            Instruction::Push(0),
            Instruction::Div,
        ];
        let result = machine.run(&program);
        assert!(matches!(result, Err(MachineError::DivisionByZero)));
    }

    #[test]
    fn test_bitwise() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(0b1100),
            Instruction::Push(0b1010),
            Instruction::And,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&0b1000));

        let program = vec![
            Instruction::Push(0b1100),
            Instruction::Push(0b1010),
            Instruction::Or,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&0b1110));

        let program = vec![Instruction::Push(0b1100), Instruction::Not];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&(!0b1100)));

        let program = vec![
            Instruction::Push(0b1100),
            Instruction::Push(0b1010),
            Instruction::Xor,
        ];

        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&0b0110));
    }

    #[test]
    fn test_comparisons() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(10),
            Instruction::Push(20),
            Instruction::SetLessThan,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&1));

        let program = vec![
            Instruction::Push(20),
            Instruction::Push(10),
            Instruction::SetGreaterThan,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&1));

        let program = vec![
            Instruction::Push(10),
            Instruction::Push(10),
            Instruction::SetEqual,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&1));

        let program = vec![
            Instruction::Push(10),
            Instruction::Push(20),
            Instruction::SetNotEqual,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&1));

        let program = vec![
            Instruction::Push(10),
            Instruction::Push(10),
            Instruction::SetLessThanOrEqual,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&1));

        let program = vec![
            Instruction::Push(20),
            Instruction::Push(10),
            Instruction::SetGreaterThanOrEqual,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&1));
    }

    #[test]
    fn test_shift() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(0b0001),
            Instruction::Push(2),
            Instruction::ShiftLeftLogical,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&0b0100));

        let program = vec![
            Instruction::Push(0b0100),
            Instruction::Push(2),
            Instruction::ShiftRightLogical,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&0b0001));

        let program = vec![
            Instruction::Push(-8), // 0b11111111111111111111111111111000 in 32-bit
            Instruction::Push(2),
            Instruction::ShiftRightArithmetic,
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&-2)); // 0b
    }

    #[test]
    fn math_with_read() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(10),
            Instruction::Push(50),
            Instruction::Push(70),
            Instruction::Add, // 50 + 70 = 120
            Instruction::Read(0),
            Instruction::Div, // 120 / 10 = 12
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&12));
    }

    #[test]
    fn test_run_until() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(10),
            Instruction::Push(20),
            Instruction::Add,
            Instruction::Push(5),
            Instruction::Mul,
        ];
        let last = machine.run_until(&program, 3).unwrap();
        assert_eq!(last, Some(&30)); // After first 3 instructions: 10 + 20 = 30
        let last = machine.run_until(&program, 5).unwrap();
        assert_eq!(last, Some(&150)); // After all instructions: 30 * 5 = 150
    }
}
