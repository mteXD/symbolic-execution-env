use crate::{
    machine::Machine,
    types::{Immediate, MachineError, Register},
};

#[allow(dead_code)]
pub enum NullaryOp {
    #[allow(dead_code)]
    Nop,
    Return,
}

#[allow(dead_code)]
pub enum UnaryOpReg {
    Not,
    Read,
    Pop,
    Call,
}

#[allow(dead_code)]
pub enum UnaryOpImm {
    Push,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
