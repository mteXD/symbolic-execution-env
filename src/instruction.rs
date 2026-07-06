//! This module defines the instruction set for the virtual machine.

use std::rc::Rc;

use crate::types::{CellIndex, Immediate};

/// Operations with 0 arguments
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NullaryOp {
    Nop,
    Rebase,
    Input,
}

/// Operations with 1 Cell Index argument
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpCell {
    Not,
    Read,
    ReadReverse,
    Print,
}

/// Operations with 1 Cell Amount argument
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpCellAmnt {
    Pop,
}

/// Operations with 1 Cell Amount argument.
///
/// [`TaggedPush`] also takes a Tag argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpImm<Tag = ()> {
    Push,
    /// TaggedPush pushes a value with the provided tag.
    TaggedPush(Tag),
}

/// Operations with 1 String argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpString {
    FunctionDefine,
    FunctionCall,
    //// Define a new downgrader
    Downgrader,
    /// Invokes a downgrader, applying its implicit retag and per-value budget.
    Downgrade,
    FileRead,
    FileWrite,
}

/// Operations with 2 Cell Index arguments
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Instruction type for the virtual machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction<Tag = ()> {
    AluNullary(NullaryOp),
    AluUnaryImm(UnaryOpImm<Tag>, Immediate),
    AluUnaryCell(UnaryOpCell, CellIndex),
    AluUnaryCellAmnt(UnaryOpCellAmnt, CellIndex),
    AluUnaryString(UnaryOpString, String),
    AluBinary(BinaryOp, CellIndex, CellIndex),
    Block(Rc<[Instruction<Tag>]>),
    IfElse(CellIndex, Rc<Instruction<Tag>>, Rc<Instruction<Tag>>),
}

/// Convenience constructor for a single [`Instruction`].
///
/// Every arm expands to fully-qualified paths, so the macro is self-contained:
/// callers need only `use crate::add_instr;` (no [`Instruction`] variant or op
/// enum imports required).
#[macro_export]
macro_rules! add_instr {
    ($op:ident) => {
        $crate::instruction::Instruction::AluNullary($crate::instruction::NullaryOp::$op)
    };
    ($op:ident, $a:expr) => {
        // for immediate
        $crate::instruction::Instruction::AluUnaryImm($crate::instruction::UnaryOpImm::$op, $a)
    };
    (tag Push, $value:expr, $tag:expr) => {
        $crate::instruction::Instruction::AluUnaryImm(
            $crate::instruction::UnaryOpImm::TaggedPush($tag),
            $value,
        )
    };
    (R Pop, $a:expr) => {
        // for a cell amount (e.g. how many cells to pop); must precede the
        // generic `R` arm below since `Pop` also matches `$op:ident`.
        $crate::instruction::Instruction::AluUnaryCellAmnt(
            $crate::instruction::UnaryOpCellAmnt::Pop,
            $a,
        )
    };
    (R $op:ident, $a:expr) => {
        // for register
        $crate::instruction::Instruction::AluUnaryCell($crate::instruction::UnaryOpCell::$op, $a)
    };
    (strarg $op:ident, $name:expr) => {
        // for an io-path string (FileRead, FileWrite)
        $crate::instruction::Instruction::AluUnaryString(
            $crate::instruction::UnaryOpString::$op,
            String::from($name),
        )
    };
    ($op:ident, $a:expr, $b:expr) => {
        $crate::instruction::Instruction::AluBinary($crate::instruction::BinaryOp::$op, $a, $b)
    };
    (fun $op:ident, $name:expr) => {
        // for a function-name string (FunctionDefine, FunctionCall, Downgrader,
        // Downgrade)
        $crate::instruction::Instruction::AluUnaryString(
            $crate::instruction::UnaryOpString::$op,
            String::from($name),
        )
    };
    (ifelse $cond:expr, $when_true:expr, $when_false:expr) => {
        $crate::instruction::Instruction::IfElse(
            $cond,
            std::rc::Rc::new($when_true),
            std::rc::Rc::new($when_false),
        )
    };
}

/// Builds an [`Instruction::Block`] from one or more instruction expressions.
#[macro_export]
macro_rules! make_block {
    ($($instr:expr),*  $(,)?) => { // Variadic arguments, at least one
        $crate::instruction::Instruction::Block(std::rc::Rc::from(vec![ $( $instr ),* ]))
    };
}
