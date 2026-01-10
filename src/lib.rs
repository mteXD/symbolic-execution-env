// A virtual machine that's a hybrid between a register-based and stack-based architecture.
// Registers are temporarily represented as i64 value. These "registers" are referred to as "cells"
// in the code.
// It has a push instruction - it places a value to the next available cell.
// This is similar to Single Static Assignment (SSA) form in compilers.
// Pop doesn't have to exist for reading purposes, as we can read directly from available cell.
// However, pop can be used to free up cells when needed.

use std::fmt::Debug;

// pub struct Register(pub usize);
// pub struct Immediate(pub i64);

type Register = u16;
type Immediate = i64;


pub enum MachineError {
    StackOverflow,
    StackUnderflow,
    InvalidRegister,
    DivisionByZero,
}

trait Operator {}

pub enum NullaryOp {
    Nop,
}

pub enum UnaryOpReg {
    Not,
    Read,
    Pop,
}

pub enum UnaryOpImm {
    Push,
}

pub enum BinaryOp {
    // Arithmetic instructions
    Add,
    Mul,
    Div,
    // Bitwise instructions
    And,
    Or,
    Xor,
    // Shifting
    ShiftLeftLogical,
    ShiftRightLogical,
    ShiftRightArithmetic,
    // Comparison ; Good enough for early stage of development
    SetEqual,
    SetNotEqual,
    SetLessThan,
    SetLessThanOrEqual,
    SetGreaterThan,
    SetGreaterThanOrEqual,
}

pub enum Instruction {
    AluNullary(NullaryOp),
    AluUnaryImm(UnaryOpImm, Immediate),
    AluUnaryReg(UnaryOpReg, Register),
    AluBinary(BinaryOp, Register, Register),
}

impl UnaryOpImm {
    pub fn eval(&self, machine: &mut Machine, imm: Immediate) -> Result<(), MachineError> {
        match self {
            UnaryOpImm::Push => {
                machine.push(imm)?;
            }
        }
        Ok(())
    }
}

impl Operator for NullaryOp {}
impl Operator for UnaryOpReg {}
impl Operator for UnaryOpImm {}
impl Operator for BinaryOp {}

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
        Machine { cells: Vec::new() }
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

    fn multi_pop(&mut self, n: Register) -> Result<(), MachineError> {
        for _ in 0..n {
            self.pop()?;
        }
        Ok(())
    }

    fn read(&self, reg: Register) -> Result<&i64, MachineError> {
        match self.cells.get::<usize>(reg.into()) {
            Some(value) => Ok(value),
            None => Err(MachineError::InvalidRegister),
        }
    }

    fn evaluate_instruction(&mut self, instruction: &Instruction) -> Result<(), MachineError> {
        use Instruction::*;

        match instruction {
            AluNullary(nullop) => match nullop {
                NullaryOp::Nop => {}
            },
            AluUnaryImm(unop_imm, imm) => match unop_imm {
                UnaryOpImm::Push => {
                    self.push(*imm)?;
                }
            },
            AluUnaryReg(unop_reg, reg) => match unop_reg {
                UnaryOpReg::Not => {
                    let val = self.read(*reg)?;
                    self.push(!*val)?;
                }
                UnaryOpReg::Read => {
                    let val = self.read(*reg)?;
                    self.push(*val)?;
                }
                UnaryOpReg::Pop => {
                    self.multi_pop(*reg)?;
                }
            },
            AluBinary(binop, reg1, reg2) => match binop {
                BinaryOp::Add => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(a + b)?;
                }
                BinaryOp::Mul => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(a * b)?;
                }
                BinaryOp::Div => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    let div = a.checked_div(*b).ok_or(MachineError::DivisionByZero)?;
                    self.push(div)?;
                }
                BinaryOp::And => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(a & b)?;
                }
                BinaryOp::Or => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(a | b)?;
                }
                BinaryOp::Xor => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(a ^ b)?;
                }
                BinaryOp::ShiftLeftLogical => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(a << b)?;
                }
                BinaryOp::ShiftRightLogical => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(((*a as u64) >> b) as i64)?;
                }
                BinaryOp::ShiftRightArithmetic => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(a >> b)?;
                }
                BinaryOp::SetEqual => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(bool_to_i64(a == b))?;
                }
                BinaryOp::SetNotEqual => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(bool_to_i64(a != b))?;
                }
                BinaryOp::SetLessThan => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(bool_to_i64(a < b))?;
                }
                BinaryOp::SetLessThanOrEqual => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(bool_to_i64(a <= b))?;
                }
                BinaryOp::SetGreaterThan => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(bool_to_i64(a > b))?;
                }
                BinaryOp::SetGreaterThanOrEqual => {
                    let a = self.read(*reg1)?;
                    let b = self.read(*reg2)?;
                    self.push(bool_to_i64(a >= b))?;
                }
            },
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
    use Instruction::{AluUnaryImm, AluUnaryReg, AluBinary};
    use UnaryOpImm::Push;

    macro_rules! test_binop {
        ($name:ident, $a:expr, $b:expr, $op:ident => $expected:expr) => {
            #[test]
            fn $name() {
                let mut machine = Machine::default();
                let program = vec![
                    AluUnaryImm(Push, $a),
                    AluUnaryImm(Push, $b),
                    AluBinary(BinaryOp::$op, 0, 1),
                ];
                let last = machine.run(&program).unwrap();
                assert_eq!(last, Some(&$expected));
            }
        };
    }

    #[test]
    fn test_push_pop() {
        use UnaryOpReg::Pop;

        let mut machine = Machine::default();
        let prog = vec![
            AluUnaryImm(Push, 1),
            AluUnaryImm(Push, 2),
            AluUnaryImm(Push, 3),
        ];
        machine.run(&prog).unwrap();
        assert_eq!(machine.cells[0], 1);
        assert_eq!(machine.cells[1], 2);
        assert_eq!(machine.cells[2], 3);

        let prog = vec![AluUnaryReg(Pop, 1)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, Some(&2));

        let prog = vec![AluUnaryReg(Pop, 2)];
        let val = machine.run(&prog).unwrap();
        assert_eq!(val, None);

        let prog = vec![AluUnaryReg(Pop, 1)];
        let result = machine.run(&prog);
        assert!(matches!(result, Err(MachineError::StackUnderflow)));
    }

    #[test]
    fn test_read() {
        let mut machine = Machine::default();
        let program = vec![
            AluUnaryImm(Push, 100),
            AluUnaryImm(Push, 200),
            AluUnaryReg(UnaryOpReg::Read, 0),
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
            AluUnaryImm(Push, 10),
            AluUnaryImm(Push, 0),
            AluBinary(BinaryOp::Div, 0, 1),
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
        let program = vec![
            AluUnaryImm(Push, 0b1100),
            AluUnaryReg(UnaryOpReg::Not, 0),
        ];
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
            AluUnaryImm(Push, 50),
            AluUnaryImm(Push, 70),
            AluUnaryImm(Push, 10),
            AluBinary(BinaryOp::Add, 0, 1), // 50 + 70 = 120
            AluBinary(BinaryOp::Div, 3, 2), // 120 / 10 = 12
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&12));
    }

    #[test]
    fn test_run_until() {
        let mut machine = Machine::default();
        let program = vec![
            AluUnaryImm(Push, 10),
            AluUnaryImm(Push, 20),
            AluBinary(BinaryOp::Add, 0, 1),
            AluUnaryImm(Push, 5),
            AluBinary(BinaryOp::Mul, 2, 3),
        ];
        let last = machine.run_until(&program, 3).unwrap();
        assert_eq!(last, Some(&30)); // After first 3 instructions: 10 + 20 = 30

        let mut machine = Machine::default();
        let last = machine.run_until(&program, 5).unwrap();
        assert_eq!(last, Some(&150)); // After all instructions: 30 * 5 = 150
    }
}
