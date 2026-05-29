use std::rc::Rc;

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
    (ifelse $when_true:expr, $when_false:expr) => {
        IfElse(Rc::new($when_true), Rc::new($when_false))
    };
}

#[macro_export]
macro_rules! make_block {
    ($($instr:expr),+) => { // Variadic arguments, at least one
        Block(std::rc::Rc::<[Instruction]>::from(vec![ $( $instr ),* ]))
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
            pub fn $name() -> std::rc::Rc<[Instruction]> {
                std::rc::Rc::<[Instruction]>::from(vec![ $( $instr ),* ])
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
        add_instr!(io Input, 0),
        add_instr!(Push, 20),
        add_instr!(SetGreaterThan, 0, 1), // ? > 20 = 0
        add_instr!(ifelse
            add_instr!(Push, 1),
            add_instr!(Push, 2)
        ),
        add_instr!(SetGreaterThan, 0, 1), // ? > 20 = 0
        add_instr!(ifelse
            add_instr!(Push, 3),
            add_instr!(Push, 4)
        ),
        add_instr!(Add, 3, 5),
    },

    conditional_problem {
        add_instr!(io Input, 0),
        add_instr!(Push, 20),
        add_instr!(SetGreaterThan, 0, 1), // ? > 20 = 0
        add_instr!(ifelse
            make_block!(
                add_instr!(Push, 999),
                add_instr!(Push, 999)
            ),
            add_instr!(Push, 42)
        ),
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
            add_instr!(ifelse
                add_instr!(fun FunctionCall, COUNTDOWN),
                add_instr!(Push, 1)
            )
        ),
    },

    small_recursion {
        add_instr!(fun FunctionDefine, COUNTDOWN),
        make_block!(
            add_instr!(R ReadReverse, 0),     // Read the argument n
            add_instr!(Rebase),               // Rebase to make n the only argument
            add_instr!(Push, 0),              // This is the bound
            add_instr!(SetGreaterThan, 0, 1), // 0 -> arg, 1 -> bound.
            add_instr!(ifelse
                make_block!(
                    add_instr!(Push, -1),  // Push 1 as the base case result
                    add_instr!(Add, 0, 2), // n - 1
                    add_instr!(fun FunctionCall, COUNTDOWN) // else, calculate countdown(n - 1)
                ),
                add_instr!(Push, 0)
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
            add_instr!(ifelse
                make_block!(
                    // Here, we forget to decrease the critical value.
                    add_instr!(fun FunctionCall, COUNTDOWN) // else, calculate countdown(n - 1)
                ),
                add_instr!(Push, 0)
            )                 // if n <= 0, skip to return
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
    },

    // =========================================================================
    // IfElse coverage: with the new inline semantics, both branches mutate the
    // parent stack directly. The verifier checks that, when the condition is
    // not statically known, both branches end with the same `cells.len()`.
    // =========================================================================

    // [POSITIVE] Both branches push exactly one cell -> matching final length.
    ifelse_balanced_push {
        add_instr!(io Input, 0),                  // [?]
        add_instr!(Push, 10),                     // [?, 10]
        add_instr!(SetGreaterThan, 0, 1),         // [?, 10, ?>10]   (unknown)
        add_instr!(ifelse
            add_instr!(Push, 100),                // true:  +1 cell
            add_instr!(Push, 200)                 // false: +1 cell
        )
    },

    // [POSITIVE] Both branches use a Block; each block contributes exactly one
    // cell to the parent (the block's last cell), so the final length matches.
    ifelse_balanced_blocks {
        add_instr!(io Input, 0),
        add_instr!(Push, 5),
        add_instr!(SetLessThan, 0, 1),            // unknown condition
        add_instr!(ifelse
            make_block!(
                add_instr!(Push, 1),
                add_instr!(Push, 2),
                add_instr!(Add, 0, 1)             // block produces 1 result
            ),
            make_block!(
                add_instr!(Push, 7),
                add_instr!(Push, 8),
                add_instr!(Mul, 0, 1)             // block produces 1 result
            )
        )
    },

    // [POSITIVE] Statically-known condition: only the taken branch runs, and the
    // verifier does not need to compare branch sizes (asymmetric branches are
    // fine here because the untaken branch is dead code).
    ifelse_known_true_asymmetric {
        add_instr!(Push, 10),
        add_instr!(Push, 3),
        add_instr!(SetGreaterThan, 0, 1),         // 10 > 3 -> known true
        add_instr!(ifelse
            add_instr!(Push, 42),                 // taken: +1 cell
            add_instr!(R Pop, 2)                  // dead:  would be -2 cells
        )
    },

    // [POSITIVE] Statically-known false condition: only the false branch runs.
    ifelse_known_false_asymmetric {
        add_instr!(Push, 3),
        add_instr!(Push, 10),
        add_instr!(SetGreaterThan, 0, 1),         // 3 > 10 -> known false
        add_instr!(ifelse
            add_instr!(R Pop, 2),                 // dead
            add_instr!(Push, 42)                  // taken: +1 cell
        )
    },

    // [POSITIVE] A Block inside an ifelse branch may use `Rebase` normally,
    // because the topmost frame is the inner Block (not the IfElseBranch
    // marker). Both branches still produce a single cell.
    ifelse_block_in_branch_can_rebase {
        add_instr!(Push, 7),
        add_instr!(io Input, 0),
        add_instr!(Push, 0),
        add_instr!(SetNotEqual, 0, 1),
        add_instr!(ifelse
            make_block!(
                add_instr!(R ReadReverse, 0),     // read top of parent stack
                add_instr!(Rebase),               // OK: rebases the inner block
                add_instr!(Push, 2),
                add_instr!(Mul, 0, 1)
            ),
            add_instr!(Push, 99)
        )
    },

    // [POSITIVE] Nested ifelse: every branch is balanced w.r.t. its sibling.
    ifelse_nested_balanced {
        add_instr!(io Input, 0),                  // outer condition source
        add_instr!(Push, 0),
        add_instr!(SetGreaterThan, 0, 1),
        add_instr!(ifelse
            make_block!(                           // outer-true: ends with 1 cell
                add_instr!(io Input, 0),
                add_instr!(Push, 5),
                add_instr!(SetLessThan, 0, 1),
                add_instr!(ifelse
                    add_instr!(Push, 1),
                    add_instr!(Push, 2)
                )
            ),
            add_instr!(Push, 999)                  // outer-false: 1 cell
        )
    },

    // =========================================================================
    // IfElse negative cases (verifier should reject before execution).
    // =========================================================================

    // [NEGATIVE] Branches differ in final stack size: true does Pop, false does
    // Push. Verifier expected error: `CondUnequalStackSizes`.
    ifelse_unequal_branches_pop_vs_push {
        add_instr!(Push, 100),                    // [100]
        add_instr!(Push, 200),                    // [100, 200]
        add_instr!(io Input, 0),                  // [100, 200, ?]
        add_instr!(Push, 0),
        add_instr!(SetGreaterThan, 0, 1),         // unknown condition
        add_instr!(ifelse
            add_instr!(R Pop, 1),                 // -1 cell
            add_instr!(Push, 7)                   // +1 cell  (mismatch by 2)
        )
    },

    // [NEGATIVE] Same direction, different magnitude: true pops 2, false pops 1.
    ifelse_unequal_branches_pop_amounts {
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(Push, 3),
        add_instr!(Push, 4),
        add_instr!(io Input, 0),
        add_instr!(Push, 0),
        add_instr!(SetNotEqual, 0, 1),
        add_instr!(ifelse
            add_instr!(R Pop, 2),
            add_instr!(R Pop, 1)
        )
    },

    // [NEGATIVE] `Rebase` directly inside an ifelse branch: the topmost frame
    // is the IfElseBranch marker, which forbids rebase. Expected: `RebaseError`.
    ifelse_rebase_in_branch_forbidden {
        add_instr!(io Input, 0),
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1),
        add_instr!(ifelse
            add_instr!(Rebase),                   // forbidden directly in branch
            add_instr!(Push, 0)
        )
    },

    // [NEGATIVE] Even when the ifelse is nested inside a block, a `Rebase`
    // directly in the branch (no inner block) is still forbidden.
    ifelse_rebase_in_branch_inside_block {
        add_instr!(Push, 1),
        make_block!(
            add_instr!(Push, 2),
            add_instr!(io Input, 0),
            add_instr!(Push, 0),
            add_instr!(SetGreaterThan, 0, 1),
            add_instr!(ifelse
                add_instr!(Rebase),               // still forbidden
                add_instr!(Push, 0)
            )
        )
    },

    // [NEGATIVE] One branch performs `Div, 0`, which becomes a verifier-time
    // `DivisionByZero` (the divisor is statically known to be 0).
    ifelse_div_by_zero_in_branch {
        add_instr!(Push, 100),
        add_instr!(Push, 0),                      // divisor 0
        add_instr!(io Input, 0),
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1),
        add_instr!(ifelse
            add_instr!(Div, 0, 1),                // -> DivisionByZero
            add_instr!(Push, 1)
        )
    },

    // [NEGATIVE] One branch reads a non-existent cell (after popping too far).
    // Expected: `InvalidCell` raised inside the branch.
    ifelse_invalid_cell_in_branch {
        add_instr!(Push, 7),
        add_instr!(io Input, 0),
        add_instr!(Push, 0),
        add_instr!(SetNotEqual, 0, 1),
        add_instr!(ifelse
            make_block!(
                add_instr!(R Pop, 4),             // pops more than exists
                add_instr!(Push, 0)
            ),
            add_instr!(Push, 1)
        )
    },

    // [NEGATIVE] Condition is not the result of a comparison instruction.
    // Expected: `UnsafeCondPlacement`.
    ifelse_bad_placement {
        add_instr!(Push, 1),                      // condition cell, but
        add_instr!(Push, 1),                      // last instr is Push, not cmp
        add_instr!(ifelse
            add_instr!(Push, 10),
            add_instr!(Push, 20)
        )
    },

    // [NEGATIVE] No condition on the stack at all when ifelse runs.
    // Expected: `StackUnderflow`.
    ifelse_no_condition {
        add_instr!(ifelse
            add_instr!(Push, 1),
            add_instr!(Push, 2)
        )
    },

    // [NEGATIVE] An ifelse whose branches use the (now-invalid) old "block-like"
    // shape: true branch is a 2-push block (=> 1 cell on parent), false branch
    // is a Pop (=> -1 cell). Expected: `CondUnequalStackSizes`.
    ifelse_unequal_block_vs_pop {
        add_instr!(Push, 5),
        add_instr!(Push, 6),
        add_instr!(io Input, 0),
        add_instr!(Push, 0),
        add_instr!(SetGreaterThan, 0, 1),
        add_instr!(ifelse
            make_block!(
                add_instr!(Push, 1),
                add_instr!(Push, 2)
            ),                                    // +1 cell
            add_instr!(R Pop, 1)                  // -1 cell
        )
    },

    // =========================================================================
    // Function definition and call edge cases.
    // =========================================================================

    // [NEGATIVE] Calling a function with no arguments on the stack when the
    // function expects one. Expected: `NotEnoughArguments`.
    function_call_missing_args {
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R ReadReverse, 0),         // requires 1 argument
            add_instr!(Rebase),
            add_instr!(Push, 2),
            add_instr!(Mul, 0, 1)
        ),
        add_instr!(fun FunctionCall, FUNC_NAME)   // no args pushed first
    },

    // [POSITIVE] Function with two args, both consumed via ReadReverse + Rebase.
    function_two_args_ok {
        add_instr!(fun FunctionDefine, "add2"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(R ReadReverse, 1),
            add_instr!(Rebase),
            add_instr!(Add, 0, 1)
        ),
        add_instr!(Push, 11),
        add_instr!(Push, 31),
        add_instr!(fun FunctionCall, "add2")
    },

    // =========================================================================
    // Block / stack edge cases.
    // =========================================================================

    // [NEGATIVE] Block ends with an empty local stack. Expected: `BlockHasEmptyStack`.
    block_pops_everything {
        add_instr!(Push, 9),
        make_block!(
            add_instr!(Push, 1),
            add_instr!(R Pop, 1)                  // body ends with no body-local cell
        )
    },

    // [NEGATIVE] Plain `Pop` underflow at top level.
    pop_underflow_top_level {
        add_instr!(R Pop, 1)
    },

    // [NEGATIVE] Read with index larger than stack size after several pushes.
    read_far_beyond_stack {
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(R Read, 100)
    },

    // [POSITIVE] A long arithmetic chain with no errors.
    long_arithmetic_chain {
        add_instr!(Push, 2),
        add_instr!(Push, 3),
        add_instr!(Add, 0, 1),                    // 5
        add_instr!(Push, 4),
        add_instr!(Mul, 2, 3),                    // 20
        add_instr!(Push, 2),
        add_instr!(Div, 4, 5)                     // 10
    }
}

pub fn prog_factorial(number: i64) -> Rc<[Instruction]> {
    Rc::<[Instruction]>::from(vec![
        add_instr!(fun FunctionDefine, String::from("factorial")),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 1
            add_instr!(SetGreaterThan, 0, 1), // n > 1
            add_instr!(ifelse // if n <= 1, skip to return
                make_block!(
                    add_instr!(Push, -1),  // Push 1 as the base case result
                    add_instr!(Add, 0, 3), // n - 1
                    add_instr!(fun FunctionCall, String::from("factorial")), // else, calculate factorial(n - 1)
                    add_instr!(Mul, 0, 5)                                    // n * factorial(n - 1
                ),
                add_instr!(Push, 1)
            )
        ),
        add_instr!(Push, number),
        add_instr!(fun FunctionCall, String::from("factorial")),
    ])
}

pub fn prog_fibonacci(number: i64) -> Rc<[Instruction]> {
    Rc::<[Instruction]>::from(vec![
        add_instr!(fun FunctionDefine, String::from("fibonacci")),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 2
            add_instr!(SetGreaterThan, 0, 1), // n > 2
            add_instr!(ifelse // if n <= 1, skip to return
                make_block!(
                    add_instr!(Push, -1),
                    add_instr!(Add, 0, 3), // n - 1
                    add_instr!(fun FunctionCall, String::from("fibonacci")), // calculate fibonacci(n - 1)
                    add_instr!(Add, 4, 3),                                   // (n - 1) - 1 = n - 2
                    add_instr!(fun FunctionCall, String::from("fibonacci")), // calculate fibonacci(n - 2)
                    add_instr!(Add, 5, 7) // fibonacci(n - 1) + fibonacci(n - 2)
                ),
                add_instr!(Push, 1)
            )
        ),
        add_instr!(Push, number),
        add_instr!(fun FunctionCall, String::from("fibonacci")),
    ])
}
