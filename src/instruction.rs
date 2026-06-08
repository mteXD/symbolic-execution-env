use std::rc::Rc;

use crate::types::{CellIndex, Immediate};

#[derive(Debug, Clone)]
pub enum NullaryOp {
    Nop,
    Rebase,
}

#[derive(Debug, Clone)]
pub enum UnaryOpCell {
    Not,
    Read,
    ReadReverse,
    Pop,
    // Tail, // Tail call not needed right now.
}

#[derive(Debug, Clone)]
pub enum UnaryOpImm {
    Push,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum FunctionOp {
    FunctionDefine,
    FunctionCall,
}

#[derive(Debug, Clone)]
pub enum IntrinsicOp {
    Print,
    Input,
    FileRead,
    FileWrite,
}

#[derive(Debug, Clone)]
pub enum IntrinsicArg {
    Cell(CellIndex),
    Str(String),
}

#[derive(Debug, Clone)]
pub enum Instruction {
    AluNullary(NullaryOp),
    AluUnaryImm(UnaryOpImm, Immediate),
    AluUnaryCell(UnaryOpCell, CellIndex),
    AluBinary(BinaryOp, CellIndex, CellIndex),
    Block(Rc<[Instruction]>),
    IfElse(CellIndex, Rc<Instruction>, Rc<Instruction>),
    AluFunction(FunctionOp, String),
    AluIntrinsic(IntrinsicOp, IntrinsicArg),
}
