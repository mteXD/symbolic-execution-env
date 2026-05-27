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
        AluIntrinsic(IntrinsicOp::$op, $a)
    };
}

#[macro_export]
macro_rules! make_block {
    ($($instr:expr),+) => { // Variadic arguments, at least one
        Block(vec![ $( $instr ),* ],)
    };
}

macro_rules! new_programs {
    (
        $(
            $name:ident {
                $( $instr:expr ),* $(,)?
            }
        ),* $(,)?
    ) => {
        $(
            // #[allow(dead_code)] // TODO: Remove this
            pub fn $name() -> Vec<Instruction> {
                vec![ $( $instr ),* ]
            }
        )*
    };
}

const FUNC_NAME: &str = "generic_function_name";
const COUNTDOWN: &str = "countdown";
const INNER: &str = "inner";
const OUTER: &str = "outer";

new_programs! {
    push5 {
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(Push, 3),
        add_instr!(Push, 4),
        add_instr!(Push, 5),
    },

    pop_multiple_bad {
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(Push, 3),
        add_instr!(R Pop, 4), // 3 elements available, but trying to pop 4
    },

    read {
        add_instr!(Push, 100),
        add_instr!(Push, 200),
        add_instr!(R Read, 0), // Read from cell 0
    },

    read_bad_index {
        add_instr!(Push, 100),
        add_instr!(R Read, 1)
    },

    read_reverse {
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Push, 30),
        add_instr!(R ReadReverse, 1), // Should read 20
    },

    read_reverse_bad_empty_1 {
        add_instr!(R ReadReverse, 0)
    },

    read_reverse_bad_empty_2 {
        add_instr!(R ReadReverse, 42)
    },

    read_reverse_bad_index_1 {
        add_instr!(Push, 10),
        add_instr!(R ReadReverse, 1)
    },

    read_reverse_bad_index_2 {
        add_instr!(Push, 10),
        add_instr!(R ReadReverse, 42)
    },

    bitwise_not {
        add_instr!(Push, 0b1100),
        add_instr!(R Not, 0)
    },

    nop {
        add_instr!(Nop),
    },

    math_with_read {
        add_instr!(Push, 50),
        add_instr!(Push, 70),
        add_instr!(Push, 10),
        add_instr!(Add, 0, 1), // 50 + 70 = 120
        add_instr!(Div, 3, 2), // 120 / 10 = 12
    },

    div_by_0 {
        add_instr!(Push, 10),
        add_instr!(Push, 0),
        add_instr!(Div, 0, 1),
    },

    basic_block {
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Add, 0, 1),
        make_block!(
            add_instr!(Push, 2),   // This push should be deleted after block ends
            add_instr!(Mul, 2, 3) // This is the last push, the result, and it should remain after block ends
        ),
        add_instr!(Add, 2, 3),
    },

    nested_block {
        add_instr!(Push, 3),
        make_block!(
            add_instr!(Push, 4),
            make_block!(
                add_instr!(Push, 5),
                add_instr!(Mul, 1, 2) // 4 * 5 = 20
            ),
            add_instr!(Add, 0, 2) // 3 + 20 = 23
        ),
    },

    block_with_pop {
        add_instr!(Push, 3),
        add_instr!(Push, 5),
        make_block!(
            add_instr!(R Pop, 2), // Pop the 20, leaving only 30
            add_instr!(Push, 0) // Required, at least something must be on stack
        ),
        add_instr!(Mul, 0, 1), // 3 * 5 = 15
    },

    block_nested_rebase_1 {
        add_instr!(Push, 2),
        make_block!(
            add_instr!(Push, 3),
            add_instr!(Rebase),
            make_block!(
                add_instr!(Push, 4),
                add_instr!(Mul, 0, 1) // 3 * 4 = 12
            ),
            add_instr!(Add, 0, 1) // 3 + 12 = 14
        ),
    },


    block_nested_rebase_2 {
        add_instr!(Push, 2),
        make_block!(
            add_instr!(Push, 3),
            add_instr!(Rebase),
            make_block!(
                add_instr!(R ReadReverse, 0),
                add_instr!(Push, 4),
                add_instr!(Rebase),
                add_instr!(Mul, 0, 1) // 3 * 4 = 12
            ),
            add_instr!(Add, 0, 1) // 3 + 12 = 14
        ),
    },

    square_add_42 {
        add_instr!(Push, 5), // Argument
        make_block!(
            add_instr!(R ReadReverse, 0), // Read x . . . r0 <- x
            add_instr!(Rebase),
            add_instr!(Mul, 0, 0), // x ^ 2 . . . r1 <- r0 ^ 2
            add_instr!(Push, 42),  // r2 <- 42
            add_instr!(Mul, 0, 2), // x * 42 . . . r3 <- r0 * r2
            add_instr!(Add, 1, 3)  // x^2 + 42x . . . r4 <- r1 + r3
        ),
    },

    conditional {
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(SetGreaterThan, 0, 1), // 10 > 20 = 0
        add_instr!(Cond),                 // Skip block
        add_instr!(Push, 999),            // This should be skipped
        add_instr!(Push, 42),             // This should be the last instruction executed
    },

    conditional_problem {
        add_instr!(io Input, 0),
        add_instr!(Push, 20),
        add_instr!(SetGreaterThan, 0, 1), // ? > 20 = 0
        add_instr!(Cond),                 // Skip block
        add_instr!(Push, 999),            // This should be skipped
        add_instr!(Push, 42),             // This should be the last instruction executed
        add_instr!(Add, 3, 4),
    },

    simple_function {
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Mul, 0, 0) // Multiply input by 2
        ),
        add_instr!(Push, 3),
        add_instr!(fun FunctionCall, FUNC_NAME),
    },

    sequential_fn_defs {
        add_instr!(fun FunctionDefine, "push2_1"),
        add_instr!(fun FunctionDefine, "push2_2"),
        add_instr!(fun FunctionDefine, "push2_3"),
        add_instr!(Push, 2),
        add_instr!(fun FunctionCall, "push2_1"),
        add_instr!(fun FunctionCall, "push2_2"),
    },

    sequential_fn_defs_loop {
        add_instr!(fun FunctionDefine, "push2_1"),
        add_instr!(fun FunctionDefine, "push2_2"),
        add_instr!(fun FunctionDefine, "push2_3"),
        add_instr!(fun FunctionCall, "push2_1"),
        add_instr!(fun FunctionCall, "push2_1"),
    },

    smaller_recursion {
        add_instr!(fun FunctionDefine, COUNTDOWN),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, -1),
            add_instr!(Push, 0),
            add_instr!(Add, 0, 1),
            add_instr!(SetGreaterThan, 3, 2), // Add > 0
            add_instr!(Cond),
            add_instr!(fun FunctionCall, COUNTDOWN)
        ),
    },

    small_recursion {
        add_instr!(fun FunctionDefine, COUNTDOWN),
        make_block!(
            add_instr!(R ReadReverse, 0),     // Read the argument n
            add_instr!(Rebase),               // Rebase to make n the only argument
            add_instr!(Push, 0),              // This is the bound
            add_instr!(SetGreaterThan, 0, 1), // 0 -> arg, 1 -> bound.
            add_instr!(Cond),                 // if n <= 0, skip to return
            make_block!(
                add_instr!(Push, -1),  // Push 1 as the base case result
                add_instr!(Add, 0, 2), // n - 1
                add_instr!(fun FunctionCall, COUNTDOWN) // else, calculate countdown(n - 1)
            )
        ),
        add_instr!(Push, 5),
        add_instr!(fun FunctionCall, COUNTDOWN),
    },

    small_recursion_bad {
        add_instr!(fun FunctionDefine, COUNTDOWN),
        make_block!(
            add_instr!(R ReadReverse, 0),     // Read the argument n
            add_instr!(Rebase),               // Rebase to make n the only argument
            add_instr!(Push, 0),              // This is the bound
            add_instr!(SetGreaterThan, 0, 1), // 0 -> critical value, 1 -> bound.
            add_instr!(Cond),                 // if n <= 0, skip to return
            make_block!(
                // Here, we forget to decrease the critical value.
                add_instr!(fun FunctionCall, COUNTDOWN) // else, calculate countdown(n - 1)
            )
        ),
        add_instr!(Push, 5),
        add_instr!(fun FunctionCall, COUNTDOWN),
    },

    recursion_nested_fn_def {
        add_instr!(fun FunctionDefine, OUTER),
        make_block!(
            add_instr!(fun FunctionDefine, INNER),
            make_block!(add_instr!(Push, 42)),
            add_instr!(fun FunctionCall, OUTER)
        ),
        add_instr!(fun FunctionCall, OUTER),
    },

    nested_functions {
        add_instr!(fun FunctionDefine, OUTER),
        make_block!(
            add_instr!(fun FunctionDefine, INNER),
            make_block!(add_instr!(Push, 42)),
            add_instr!(fun FunctionCall, INNER)
        ),
        add_instr!(fun FunctionCall, OUTER),
    },

    nested_functions_bad {
        add_instr!(fun FunctionDefine, OUTER),
        make_block!(
            add_instr!(fun FunctionDefine, INNER),
            make_block!(add_instr!(Push, 42)),
            add_instr!(fun FunctionCall, INNER)
        ),
        add_instr!(fun FunctionCall, INNER),
        add_instr!(fun FunctionCall, INNER), // This should fail
    },

    function_multi_args {
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(R ReadReverse, 1),
            add_instr!(R ReadReverse, 2),
            add_instr!(Rebase),
            add_instr!(Add, 0, 1), // a + b
            add_instr!(Add, 3, 2)  // (a + b) + c
        ),
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Push, 30),
        add_instr!(fun FunctionCall, FUNC_NAME),
    },

    input {
       add_instr!(io Input, 0)
    },

    // This function is a void function.
    void_print_block {
        add_instr!(Push, 42),
        make_block!(
            add_instr!(io Print, 0)
        ),
        add_instr!(R Pop, 1), 
        add_instr!(R Read, 0)
    },

    block_with_pops_only {
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        make_block!(
            add_instr!(R Pop, 2)
        ),
        add_instr!(Add, 0, 1) // This should still work, block has no effect on the outer code
    }
}
