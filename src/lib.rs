// A virtual machine that's a hybrid between a register-based and stack-based architecture.
// Registers are temporarily represented as i64 value. These "registers" are referred to as "cells"
// in the code.
// It has a push instruction - it places a value to the next available cell.
// This is similar to Single Static Assignment (SSA) form in compilers.
// Pop doesn't have to exist for reading purposes, as we can read directly from available cell.
// However, pop can be used to free up cells when needed.

use std::fmt::Debug;

type Register = usize;
// type Immediate = i64;

pub enum Instruction {
    // Basic instructions = memory instructions
    Push(i64),   // Push a new value into the next available cell
    Pop(usize),  // Pop n cells to free them up
    Read(usize), // Read from a specific cell; cell must exist
    Nop,
    // Arithmetic instructions
    Add(Register, Register),
    Mul(Register, Register),
    Div(Register, Register),
    // Bitwise instructions
    And(Register, Register),
    Or(Register, Register),
    Not(Register),
    Xor(Register, Register),
    // Shifting
    ShiftLeftLogical(Register, Register),
    ShiftRightLogical(Register, Register),
    ShiftRightArithmetic(Register, Register),
    // Comparison (set); Good enough for early stage of development
    SetEqual(Register, Register),
    SetNotEqual(Register, Register),
    SetLessThan(Register, Register),
    SetLessThanOrEqual(Register, Register),
    SetGreaterThan(Register, Register),
    SetGreaterThanOrEqual(Register, Register),
}

pub enum MachineError {
    StackOverflow,
    StackUnderflow,
    InvalidRegister,
    DivisionByZero,
}

impl Debug for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            MachineError::StackOverflow => "Stack Overflow",
            MachineError::StackUnderflow => "Stack Underflow",
            MachineError::InvalidRegister => "Invalid Register",
            MachineError::DivisionByZero => "Division By Zero",
        };
        write!(f, "{}", text)
    }
}

pub struct Machine {
    cells: Vec<i64>,
}

impl Machine {
    pub fn new() -> Self {
        Machine {
            cells: Vec::new(),
        }
    }

    fn push(&mut self, value: i64) -> Result<(), MachineError> {
        self.cells.push(value);
        Ok(())
    }

    fn pop(&mut self) -> Result<i64, MachineError> {
        if let Some(value) = self.cells.pop() {
            Ok(value)
        } else {
            Err(MachineError::StackUnderflow)
        }
    }

    fn multi_pop(&mut self, n: usize) -> Result<(), MachineError> {
        for _ in 0..n {
            self.pop()?;
        }
        Ok(())
    }

    fn read(&self, reg: Register) -> Result<&i64, MachineError> {
        match self.cells.get(reg) {
            Some(value) => Ok(value),
            None => Err(MachineError::InvalidRegister),
        }
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
                let val = self.read(*reg)?;
                self.push(*val)?;
            }
            Instruction::Add(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(a + b)?;
            }
            Instruction::Mul(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(a * b)?;
            }
            Instruction::Div(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                let div = a.checked_div(*b).ok_or(MachineError::DivisionByZero)?;
                self.push(div)?;
            }
            Instruction::And(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(a & b)?;
            }
            Instruction::Or(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(a | b)?;
            }
            Instruction::Not(r) => {
                let a = self.read(*r)?;
                self.push(!a)?;
            }
            Instruction::Xor(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(a ^ b)?;
            }
            Instruction::ShiftLeftLogical(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(a << b)?;
            }
            Instruction::ShiftRightLogical(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(((*a as u64) >> b) as i64)?;
            }
            Instruction::ShiftRightArithmetic(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(a >> b)?;
            }
            Instruction::SetEqual(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(bool_to_i64(a == b))?;
            }
            Instruction::SetNotEqual(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(bool_to_i64(a != b))?;
            }
            Instruction::SetLessThan(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(bool_to_i64(a < b))?;
            }
            Instruction::SetLessThanOrEqual(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(bool_to_i64(a <= b))?;
            }
            Instruction::SetGreaterThan(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
                self.push(bool_to_i64(a > b))?;
            }
            Instruction::SetGreaterThanOrEqual(r1, r2) => {
                let a = self.read(*r1)?;
                let b = self.read(*r2)?;
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
        Machine::new() // Default to 64 cells
    }
}

fn bool_to_i64(value: bool) -> i64 {
    i64::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_binop {
        ($name:ident, $a:expr, $b:expr, $op:ident => $expected:expr) => {
            #[test]
            fn $name() {
                let mut machine = Machine::default();
                let program = vec![
                    Instruction::Push($a),
                    Instruction::Push($b),
                    Instruction::$op(0, 1),
                ];
                let last = machine.run(&program).unwrap();
                assert_eq!(last, Some(&$expected));
            }
        };
    }

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

        let prog = vec![Instruction::Pop(1)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, Some(&2));

        let prog = vec![Instruction::Pop(2)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, None);

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
        assert_eq!(machine.cells[0], 100);
        assert_eq!(machine.cells[1], 200);
    }

    test_binop!(test_add, 10, 20, Add => 30);
    test_binop!(test_add_neg, 10, -30, Add => -20);
    test_binop!(test_mul, 10, 20, Mul => 200);
    test_binop!(test_div, 20, 5, Div => 4);

    #[test]
    fn test_div_bad() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(10),
            Instruction::Push(0),
            Instruction::Div(0, 1),
        ];
        let result = machine.run(&program);
        assert!(matches!(result, Err(MachineError::DivisionByZero)));
    }

    test_binop!(test_and, 0b1100, 0b1010, And => 0b1000);
    test_binop!(test_or, 0b1100, 0b1010, Or => 0b1110);
    test_binop!(test_xor, 0b1100, 0b1010, Xor => 0b0110);

    #[test]
    fn test_not() {
        let mut machine = Machine::default();
        let program = vec![Instruction::Push(0b1100), Instruction::Not(0)];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&(!0b1100)));
    }

    test_binop!(test_slt, 10, 20, SetLessThan => 1);
    test_binop!(test_sgt, 20, 10, SetGreaterThan => 1);
    test_binop!(test_seq, 10, 10, SetEqual => 1);
    test_binop!(test_sne, 10, 20, SetNotEqual => 1);
    test_binop!(test_sle, 10, 10, SetLessThanOrEqual => 1);
    test_binop!(test_sge, 20, 10, SetGreaterThanOrEqual => 1);

    test_binop!(test_sll, 0b0001, 2, ShiftLeftLogical => 0b0100);
    test_binop!(test_srl, 0b0100, 2, ShiftRightLogical => 0b0001);
    test_binop!(test_sra, -8, 2, ShiftRightArithmetic => -2);

    #[test]
    fn math_with_read() {
        let mut machine = Machine::default();
        let program = vec![
            Instruction::Push(50),
            Instruction::Push(70),
            Instruction::Push(10),
            Instruction::Add(0, 1), // 50 + 70 = 120
            Instruction::Div(3, 2), // 120 / 10 = 12
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
            Instruction::Add(0, 1),
            Instruction::Push(5),
            Instruction::Mul(2, 3),
        ];
        let last = machine.run_until(&program, 3).unwrap();
        assert_eq!(last, Some(&30)); // After first 3 instructions: 10 + 20 = 30

        let mut machine = Machine::default();
        let last = machine.run_until(&program, 5).unwrap();
        assert_eq!(last, Some(&150)); // After all instructions: 30 * 5 = 150
    }
}
