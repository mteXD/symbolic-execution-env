//! This module defines the instruction set for the virtual machine.

use std::rc::Rc;

use crate::types::{CellIndex, Immediate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NullaryOp {
    Nop,
    Rebase,
    Input,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpCell {
    Not,
    Read,
    ReadReverse,
    Print,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpCellAmnt {
    Pop,
}

/// An operation that takes one immediate value encoded in the instruction.
///
/// The tag parameter is only used by [`UnaryOpImm::TaggedPush`]. It defaults
/// to `()`, so ordinary unmonitored programs keep using the simple
/// `UnaryOpImm` type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpImm<Tag = ()> {
    Push,
    TaggedPush(Tag),
}

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
/// The unqualified arms expect the relevant [`Instruction`] variants and op
/// enums (e.g. `Push`, `NullaryOp`, ...) to be in scope at the call site.
#[macro_export]
macro_rules! add_instr {
    ($op:ident) => {
        AluNullary(NullaryOp::$op)
    };
    ($op:ident, $a:expr) => {
        // for immediate
        AluUnaryImm(UnaryOpImm::$op, $a)
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
        AluUnaryCellAmnt(UnaryOpCellAmnt::Pop, $a)
    };
    (R $op:ident, $a:expr) => {
        // for register
        AluUnaryCell(UnaryOpCell::$op, $a)
    };
    (strarg $op:ident, $name:expr) => {
        // for an io-path string (FileRead, FileWrite)
        AluUnaryString(UnaryOpString::$op, String::from($name))
    };
    ($op:ident, $a:expr, $b:expr) => {
        AluBinary(BinaryOp::$op, $a, $b)
    };
    (fun $op:ident, $name:expr) => {
        // for a function-name string (FunctionDefine, FunctionCall, Downgrader,
        // Downgrade)
        AluUnaryString(UnaryOpString::$op, String::from($name))
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
