use crate::instruction::{
    Instruction::{self, *},
    *,
};

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

#[macro_export]
macro_rules! make_block {
    ($($instr:expr),*  $(,)?) => { // Variadic arguments, at least one
        $crate::instruction::Instruction::Block(std::rc::Rc::from(vec![ $( $instr ),* ]))
    };
}

macro_rules! prog {
    (
        $( $instr:expr ),* $(,)?
    ) => {
        std::rc::Rc::<[Instruction]>::from(vec![ $( $instr ),* ])
    };
}

pub type Snippet = std::rc::Rc<[Instruction]>;
const FUNC_NAME: &str = "generic_function_name";
const INNER: &str = "inner";
const OUTER: &str = "outer";

const fn factorial_helper(n: i64) -> i64 {
    if n <= 1 {
        return 1;
    }
    n * factorial_helper(n - 1)
}

const fn fibonacci_helper(n: i64) -> i64 {
    if n <= 1 {
        return 1;
    }
    fibonacci_helper(n - 1) + fibonacci_helper(n - 2)
}

pub mod testable;
pub mod testable_diftam;
pub mod showcase;
pub mod showcase_diftam;
