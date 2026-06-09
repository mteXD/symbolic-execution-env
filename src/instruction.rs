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

/// An operation that takes one immediate value encoded in the instruction.
///
/// The tag parameter is only used by [`UnaryOpImm::TaggedPush`]. It defaults
/// to `()`, so ordinary unmonitored programs keep using the simple
/// `UnaryOpImm` type.
#[derive(Debug, Clone)]
pub enum UnaryOpImm<Tag = ()> {
    Push,
    TaggedPush(Tag),
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
pub enum Instruction<Tag = ()> {
    AluNullary(NullaryOp),
    AluUnaryImm(UnaryOpImm<Tag>, Immediate),
    AluUnaryCell(UnaryOpCell, CellIndex),
    AluBinary(BinaryOp, CellIndex, CellIndex),
    Block(Rc<[Instruction<Tag>]>),
    IfElse(CellIndex, Rc<Instruction<Tag>>, Rc<Instruction<Tag>>),
    AluFunction(FunctionOp, String),
    AluIntrinsic(IntrinsicOp, IntrinsicArg),
}
