//! This module defines the instruction set for the virtual machine.

use std::rc::Rc;

use crate::types::{CellAmount, CellIndex, Immediate};

/// Operations with 0 arguments
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NullaryOp {
    Nop,
    Input,
}

/// Operations with 1 immediate argument.
///
/// [`Self::TaggedPush`] also carries a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpImm<Tag = ()> {
    Push,
    /// TaggedPush pushes a value with the provided tag.
    TaggedPush(Tag),
}

/// Operations with 1 Cell Index argument
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpCell {
    Neg,
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

/// Operations with 1 String argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpString {
    FunctionDefine,
    FunctionCall,
    /// Defines a new downgrader.
    Downgrader,
    /// Invokes a downgrader
    Downgrade,
}

/// Operations with 2 Cell Index arguments
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic instructions
    Add,
    Sub,
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
    // Comparison; Good enough for early stage of development
    CmpEqual,
    CmpNotEqual,
    CmpLessThan,
    CmpLessThanOrEqual,
    CmpGreaterThan,
    CmpGreaterThanOrEqual,
}

/// Instruction type for the virtual machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction<Tag = ()> {
    Nullary(NullaryOp),
    UnaryImm(UnaryOpImm<Tag>, Immediate),
    UnaryCell(UnaryOpCell, CellIndex),
    UnaryCellAmnt(UnaryOpCellAmnt, CellAmount),
    UnaryString(UnaryOpString, String),
    Binary(BinaryOp, CellIndex, CellIndex),
    /// A structured block and its argument count.
    Block(CellAmount, Rc<[Instruction<Tag>]>),
    /// Conditional branch instruction
    IfElse(CellIndex, Rc<[Instruction<Tag>]>, Rc<[Instruction<Tag>]>),
}

/// Convenience constructor for a single [`Instruction`].
///
/// Every arm expands to fully-qualified paths, so the macro is self-contained:
/// callers need only `use crate::add_instr;` (no [`Instruction`] variant or op
/// enum imports required).
#[macro_export]
macro_rules! add_instr {
    ($op:ident) => {
        $crate::instruction::Instruction::Nullary($crate::instruction::NullaryOp::$op)
    };
    ($op:ident, $a:expr) => {
        // for immediate
        $crate::instruction::Instruction::UnaryImm($crate::instruction::UnaryOpImm::$op, $a)
    };
    (TaggedPush, $value:expr, $tag:expr) => {
        $crate::instruction::Instruction::UnaryImm(
            $crate::instruction::UnaryOpImm::TaggedPush($tag),
            $value,
        )
    };
    (R Pop, $a:expr) => {
        // for a cell amount (e.g. how many cells to pop); must precede the
        // generic `R` arm below since `Pop` also matches `$op:ident`.
        $crate::instruction::Instruction::UnaryCellAmnt(
            $crate::instruction::UnaryOpCellAmnt::Pop,
            $a,
        )
    };
    (R $op:ident, $a:expr) => {
        // for register
        $crate::instruction::Instruction::UnaryCell($crate::instruction::UnaryOpCell::$op, $a)
    };
    ($op:ident, $a:expr, $b:expr) => {
        $crate::instruction::Instruction::Binary($crate::instruction::BinaryOp::$op, $a, $b)
    };
    (fun $op:ident, $name:expr) => {
        // for a function-name string (FunctionDefine, FunctionCall, Downgrader,
        // Downgrade)
        $crate::instruction::Instruction::UnaryString(
            $crate::instruction::UnaryOpString::$op,
            String::from($name),
        )
    };
    (ifelse $cond:expr,
        [ $( $when_true:expr ),* $(,)? ],
        [ $( $when_false:expr ),* $(,)? ]
        $(,)?
    ) => {
        $crate::instruction::Instruction::IfElse(
            $cond,
            std::rc::Rc::from(vec![ $( $when_true ),* ]),
            std::rc::Rc::from(vec![ $( $when_false ),* ]),
        )
    };
}

/// Builds an [`Instruction::Block`] with an explicit argument count.
///
/// The first expression is the number of caller cells cloned into the isolated
/// block. The instruction list may be empty, although both runners reject an
/// empty block when processing it.
#[macro_export]
macro_rules! make_block {
    ($argument_count:expr $(, $instr:expr)* $(,)?) => {
        $crate::instruction::Instruction::Block(
            $argument_count,
            std::rc::Rc::from(vec![ $( $instr ),* ]),
        )
    };
}
