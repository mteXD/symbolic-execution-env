use std::fmt::Debug;

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
