use crate::{
    machine::Machine,
    types::{Immediate, MachineError, Register},
};

#[allow(dead_code)]
pub enum NullaryOp {
    #[allow(dead_code)]
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

pub trait Operator {
    type ArgType;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError>;
}

impl Operator for NullaryOp {
    type ArgType = ();

    fn eval(&self, _: &mut Machine, _: ()) -> Result<(), MachineError> {
        match self {
            NullaryOp::Nop => {}
        }
        Ok(())
    }
}
impl Operator for UnaryOpReg {
    type ArgType = Register;

    fn eval(&self, machine: &mut Machine, arg: Register) -> Result<(), MachineError> {
        use UnaryOpReg::*;
        match self {
            Not => {
                let val = machine.read(arg)?;
                machine.push(!*val)?;
            }
            Read => {
                let val = machine.read(arg)?;
                machine.push(*val)?;
            }
            Pop => {
                machine.multi_pop(arg)?;
            }
        }
        Ok(())
    }
}
impl Operator for UnaryOpImm {
    type ArgType = Immediate;

    fn eval(&self, machine: &mut Machine, arg: Immediate) -> Result<(), MachineError> {
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

    fn eval(&self, machine: &mut Machine, arg: (Register, Register)) -> Result<(), MachineError> {
        let (reg1, reg2) = arg;
        use BinaryOp::*;

        match self {
            Add => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(a + b)?;
            }
            Mul => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(a * b)?;
            }
            Div => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                let div = a.checked_div(*b).ok_or(MachineError::DivisionByZero)?;
                machine.push(div)?;
            }
            And => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(a & b)?;
            }
            Or => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(a | b)?;
            }
            Xor => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(a ^ b)?;
            }
            ShiftLeftLogical => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(a << b)?
            }
            ShiftRightLogical => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(((*a as u64) >> b) as i64)?;
            }
            ShiftRightArithmetic => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(a >> b)?;
            }
            SetEqual => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(from_bool(a == b))?;
            }
            SetNotEqual => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(from_bool(a != b))?;
            }
            SetLessThan => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(from_bool(a < b))?;
            }
            SetLessThanOrEqual => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(from_bool(a <= b))?;
            }
            SetGreaterThan => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(from_bool(a > b))?;
            }
            SetGreaterThanOrEqual => {
                let a = machine.read(reg1)?;
                let b = machine.read(reg2)?;
                machine.push(from_bool(a >= b))?;
            }
        }
        Ok(())
    }
}

fn from_bool<T: From<bool>>(value: bool) -> T {
    T::from(value)
}
