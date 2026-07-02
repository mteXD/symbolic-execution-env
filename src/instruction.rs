//! This module defines the instruction set for the virtual machine.

use std::rc::Rc;

use crate::types::{CellIndex, Immediate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NullaryOp {
    Nop,
    Rebase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOpCell {
    Not,
    Read,
    ReadReverse,
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
pub enum FunctionOp {
    FunctionDefine,
    FunctionCall,
    /// Defines the body of a trusted downgrader (its connection and budget live
    /// in the policy, keyed by name). Distinct from `FunctionDefine` so a
    /// downgrade gate is never confused with an ordinary function.
    Downgrader,
    /// Invokes a downgrader, applying its implicit retag and per-value budget.
    /// Distinct from `FunctionCall` to make every downgrade site explicit.
    Downgrade,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntrinsicOp {
    Print,
    Input,
    FileRead,
    FileWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntrinsicArg {
    /// For intrinsics that take no argument (e.g. `Input`).
    None,
    Cell(CellIndex),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    (R $op:ident, $a:expr) => {
        // for register
        AluUnaryCell(UnaryOpCell::$op, $a)
    };
    ($op:ident, $a:expr, $b:expr) => {
        AluBinary(BinaryOp::$op, $a, $b)
    };
    (fun $op:ident, $name:expr) => {
        AluFunction(FunctionOp::$op, String::from($name))
    };
    (io $op:ident) => {
        AluIntrinsic(
            IntrinsicOp::$op,
            $crate::instruction::IntrinsicArg::None,
        )
    };
    (io $op:ident, $a:expr) => {
        AluIntrinsic(
            IntrinsicOp::$op,
            $crate::instruction::IntrinsicArg::Cell($a),
        )
    };
    (io_str $op:ident, $a:expr) => {
        AluIntrinsic(
            IntrinsicOp::$op,
            $crate::instruction::IntrinsicArg::Str(String::from($a)),
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
