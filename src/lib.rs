// A virtual machine that's a hybrid between a register-based and stack-based architecture.
// Registers are temporarily represented as i64 value. These "registers" are referred to as "cells"
// in the code.
// It has a push instruction - it places a value to the next available cell.
// This is similar to Single Static Assignment (SSA) form in compilers.
// Pop doesn't have to exist for reading purposes, as we can read directly from available cell.
// However, pop can be used to free up cells when needed.

// pub struct Register(pub usize);
// pub struct Immediate(pub i64);
use std::fmt::Debug;

mod instructions;

pub type Register = u16;
pub type Immediate = i64;

pub enum MachineError {
    StackUnderflow,
    InvalidRegister,
    DivisionByZero,
}

impl Debug for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            MachineError::StackUnderflow => "Stack Underflow",
            MachineError::InvalidRegister => "Invalid Register",
            MachineError::DivisionByZero => "Division By Zero",
        };
        write!(f, "{}", text)
    }
}

pub enum NullaryOp {
    Nop,
    Return,
}

pub enum UnaryOpReg {
    Not,
    Read,
    Pop,
    Call,
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

pub trait Operator {
    type ArgType;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError>;
}

impl Operator for NullaryOp {
    type ArgType = ();

    fn eval(&self, _: &mut Machine, _: Self::ArgType) -> Result<(), MachineError> {
        match self {
            NullaryOp::Nop => {}
            NullaryOp::Return => {
                todo!()
            }
        }
        Ok(())
    }
}
impl Operator for UnaryOpReg {
    type ArgType = Register;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError> {
        use UnaryOpReg::*;

        match self {
            Not => {
                let val = !*machine.read(arg)?;
                machine.push(val)?;
            }
            Read => {
                let val = *machine.read(arg)?;
                machine.push(val)?;
            }
            Pop => {
                machine.multi_pop(arg)?;
            }
            Call => {
                todo!()
            }
        }
        Ok(())
    }
}
impl Operator for UnaryOpImm {
    type ArgType = Immediate;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError> {
        match self {
            UnaryOpImm::Push => {
                machine.push(arg)?;
            }
        }
        Ok(())
    }
}
impl Operator for BinaryOp {
    type ArgType = (Register, Register);

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError> {
        use BinaryOp::*;
        fn from_bool<T: From<bool>>(value: bool) -> T {
            value.into()
        }

        let (reg1, reg2) = arg;

        let a = machine.read(reg1)?;
        let b = machine.read(reg2)?;

        let calculated_value = match self {
            Add => a + b,
            Mul => a * b,
            Div => a.checked_div(*b).ok_or(MachineError::DivisionByZero)?,
            And => a & b,
            Or => a | b,
            Xor => a ^ b,
            ShiftLeftLogical => a << b,
            ShiftRightLogical => ((*a as u64) >> b) as i64,
            ShiftRightArithmetic => a >> b,
            SetEqual => from_bool(a == b),
            SetNotEqual => from_bool(a != b),
            SetLessThan => from_bool(a < b),
            SetLessThanOrEqual => from_bool(a <= b),
            SetGreaterThan => from_bool(a > b),
            SetGreaterThanOrEqual => from_bool(a >= b),
        };

        machine.push(calculated_value)?;

        Ok(())
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
        use Instruction::{AluBinary, AluNullary, AluUnaryImm, AluUnaryReg};

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
    use super::*;
    use Instruction::{AluBinary, AluNullary, AluUnaryImm, AluUnaryReg};

    #[macro_export]
    macro_rules! add_instr {
        ($op:ident) => {
            AluNullary(NullaryOp::$op)
        };
        ($op:ident, $a:expr) => {
            // for immediate
            AluUnaryImm(UnaryOpImm::$op, $a)
        };
        (R $op:ident, $a:expr) => {
            // for register
            AluUnaryReg(UnaryOpReg::$op, $a)
        };
        ($op:ident, $a:expr, $b:expr) => {
            AluBinary(BinaryOp::$op, $a, $b)
        };
    }

    macro_rules! test_binop {
        ($name:ident, $a:expr, $b:expr, $op:ident => $expected:expr) => {
            #[test]
            fn $name() {
                let mut machine = Machine::default();
                let program = vec![
                    add_instr!(Push, $a),
                    add_instr!(Push, $b),
                    add_instr!($op, 0, 1),
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

    test_binop!(test_add, 10, 20, Add => 30);
    test_binop!(test_add_neg, 10, -30, Add => -20);
    test_binop!(test_mul, 10, 20, Mul => 200);
    test_binop!(test_div, 20, 5, Div => 4);

    #[test]
    fn test_div_bad() {
        let mut machine = Machine::default();
        let program = vec![
            add_instr!(Push, 10),
            add_instr!(Push, 0),
            add_instr!(Div, 0, 1),
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
        let program = vec![add_instr!(Push, 0b1100), add_instr!(R Not, 0)];
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
    fn nop() {
        let mut machine = Machine::default();
        let program = vec![add_instr!(Nop)];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, None);
    }

    #[test]
    fn math_with_read() {
        let mut machine = Machine::default();
        let program = vec![
            add_instr!(Push, 50),
            add_instr!(Push, 70),
            add_instr!(Push, 10),
            add_instr!(Add, 0, 1), // 50 + 70 = 120
            add_instr!(Div, 3, 2), // 120 / 10 = 12
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&12));
    }

    #[test]
    fn test_run_until() {
        let mut machine = Machine::default();
        let program = vec![
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Add, 0, 1),
            add_instr!(Push, 5),
            add_instr!(Mul, 2, 3),
        ];
        let last = machine.run_until(&program, 3).unwrap();
        assert_eq!(last, Some(&30)); // After first 3 instructions: 10 + 20 = 30

        let mut machine = Machine::default();
        let last = machine.run_until(&program, 5).unwrap();
        assert_eq!(last, Some(&150)); // After all instructions: 30 * 5 = 150
    }
}
