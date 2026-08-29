//! Integration tests for core VM behavior.

use virtual_machine::{
    add_instr,
    instruction::Instruction,
    machine::{
        CoreError,
        executor::ExecutorError,
        verifier::{ValueSpan, VerifierError},
    },
    make_block, test_program,
    types::{FunctionDataError, Immediate},
};

// ---------------------------------------------------------------------------
// Stack operations
// ---------------------------------------------------------------------------

mod stack {
    use super::*;

    test_program! {
        /// [POSITIVE] Pushes 5 values onto the stack.
        push,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(Push, 4),
            add_instr!(Push, 5),
        ],
        verifier: { stack [1, 2, 3, 4, 5] },
        executor: { stack [1, 2, 3, 4, 5] },
    }

    test_program! {
        /// [POSITIVE] Pushes 3 values, then pops 2, leaving 1 on the stack.
        pop_most,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(R Pop, 2),
        ],
        verifier: { stack [1] },
        executor: { stack [1] },
    }

    test_program! {
        /// [POSITIVE] Pushes 3 values, then pops all 3, leaving the stack empty.
        pop_all,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(R Pop, 3),
        ],
        verifier: { stack [] },
        executor: { stack [] },
    }

    test_program! {
        /// [NEGATIVE] Tries to pop from an empty stack.
        pop_empty,
        program: vec![add_instr!(R Pop, 1)],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [NEGATIVE] Pushes 3 values, then tries to pop 4.
        pop_too_many,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(R Pop, 4),
        ],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [POSITIVE] Pushes a value and reads it back with index 0.
        read,
        program: vec![add_instr!(Push, 42), add_instr!(R Read, 0)],
        verifier: { stack [42, 42] },
        executor: { stack [42, 42] },
    }

    test_program! {
        /// [NEGATIVE] Tries to read from an empty stack.
        read_empty,
        program: vec![add_instr!(R Read, 0)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [POSITIVE] Pushes 3 values and reads them back with indices 0, 1, 2.
        read_multiple,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            add_instr!(R Read, 0),
            add_instr!(R Read, 1),
            add_instr!(R Read, 2),
        ],
        verifier: { stack [10, 20, 30, 10, 20, 30] },
        executor: { stack [10, 20, 30, 10, 20, 30] },
    }

    test_program! {
        /// [NEGATIVE] Pushes a value and tries to read from index 1.
        read_bad_index,
        program: vec![add_instr!(Push, 42), add_instr!(R Read, 1)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [NEGATIVE] Read with index larger than stack size after several pushes.
        read_far_beyond_stack,
        program: vec![add_instr!(Push, 1), add_instr!(Push, 2), add_instr!(R Read, 100)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [POSITIVE] Reads the top of stack.
        read_reverse,
        program: vec![add_instr!(Push, 10), add_instr!(R ReadReverse, 0)],
        verifier: { stack [10, 10] },
        executor: { stack [10, 10] },
    }

    test_program! {
        /// [POSITIVE] Reads the top 3 values.
        ///
        /// As can be seen, `ReadReverse` with the same index used multiple times can be utilized to
        /// clone a portion of the stack.
        read_reverse_multiple,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2),
        ],
        verifier: { stack [10, 20, 30, 10, 20, 30] },
        executor: { stack [10, 20, 30, 10, 20, 30] },
    }

    test_program! {
        /// [NEGATIVE] Pushes a value and tries to read reverse from index 1.
        read_reverse_bad_index,
        program: vec![add_instr!(Push, 42), add_instr!(R ReadReverse, 1)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [NEGATIVE] Tries to read reverse from an empty stack.
        read_reverse_bad_empty,
        program: vec![add_instr!(R ReadReverse, 0)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }
}

// ---------------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------------

mod arith {
    use super::*;

    test_program! {
        /// [POSITIVE] Mathematical negation appends the negated cell.
        neg,
        program: vec![add_instr!(Push, 6), add_instr!(R Neg, 0)],
        verifier: { stack [6, -6] },
        executor: { stack [6, -6] },
    }

    test_program! {
        /// [POSITIVE] Subtraction appends cell1 - cell2 in operand order.
        sub,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(Push, 3),
            add_instr!(Sub, 0, 1),
        ],
        verifier: { stack [10, 3, 7] },
        executor: { stack [10, 3, 7] },
    }

    test_program! {
        /// [POSITIVE] Tests bitwise not.
        bitwise_not,
        program: vec![add_instr!(Push, 0b1100), add_instr!(R Not, 0)],
        verifier: { stack [0b1100, !0b1100] },
        executor: { stack [0b1100, !0b1100] },
    }

    test_program! {
        /// [POSITIVE] Bitwise not maps the merged interval `[-2, 4]` exactly to
        /// `[!4, !-2] == [-5, 1]`.
        bitwise_not_interval,
        program: vec![
            add_instr!(Input),
            add_instr!(Push, 0),
            add_instr!(CmpGreaterThan, 0, 1),
            add_instr!(ifelse 2,
                add_instr!(Push, -2),
                add_instr!(Push, 4)
            ),
            add_instr!(R Not, 3),
        ],
        verifier: { stack [
            ValueSpan::inf(),
            0,
            ValueSpan::new(0, 1),
            ValueSpan::new(-2, 4),
            ValueSpan::new(-5, 1),
        ] },
        executor: { cases {
            input [1] => stack [1, 0, 1, -2, 1];
            input [0] => stack [0, 0, 0, 4, -5]
        } },
    }

    test_program! {
        /// [POSITIVE] Bitwise not maps the full signed 64-bit input range back to
        /// itself and exchanges its endpoints.
        bitwise_not_full_range,
        program: vec![add_instr!(Input), add_instr!(R Not, 0)],
        verifier: { stack [ValueSpan::inf(), ValueSpan::inf()] },
        executor: { cases {
            input [Immediate::MIN] => stack [Immediate::MIN, Immediate::MAX];
            input [Immediate::MAX] => stack [Immediate::MAX, Immediate::MIN]
        } },
    }

    test_program! {
        /// [POSITIVE] Tests Nop.
        nop,
        program: vec![add_instr!(Nop)],
        verifier: { stack [] },
        executor: { stack [] },
    }

    test_program! {
        /// [NEGATIVE] Tests division by zero.
        div_by_zero,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(Push, 0),
            add_instr!(Div, 0, 1),
        ],
        verifier: { error VerifierError::DivisionByZero },
        executor: { error ExecutorError::DivisionByZero },
    }

    test_program! {
        /// An exact arithmetic result at the representable boundary is not an
        /// overflow.
        overflow_boundaries_add_exact,
        program: vec![
            add_instr!(Push, Immediate::MAX),
            add_instr!(Push, 0),
            add_instr!(Add, 0, 1),
        ],
        verifier: { stack [Immediate::MAX, 0, Immediate::MAX] },
        executor: { stack [Immediate::MAX, 0, Immediate::MAX] },
    }

    test_program! {
        /// An exact addition result at the lower representable boundary is not
        /// an overflow.
        overflow_boundaries_add,
        program: vec![
            add_instr!(Push, Immediate::MIN),
            add_instr!(Push, 0),
            add_instr!(Add, 0, 1),
        ],
        verifier: { stack [Immediate::MIN, 0, Immediate::MIN] },
        executor: { stack [Immediate::MIN, 0, Immediate::MIN] },
    }

    test_program! {
        /// Exact multiplication may produce either representable boundary.
        overflow_boundaries_mul,
        program: vec![
            add_instr!(Push, Immediate::MAX),
            add_instr!(Push, 1),
            add_instr!(Mul, 0, 1),
            add_instr!(Push, Immediate::MIN),
            add_instr!(Mul, 3, 1),
        ],
        verifier: { stack [
            Immediate::MAX,
            1,
            Immediate::MAX,
            Immediate::MIN,
            Immediate::MIN,
        ] },
        executor: { stack [
            Immediate::MAX,
            1,
            Immediate::MAX,
            Immediate::MIN,
            Immediate::MIN,
        ] },
    }

    test_program! {
        /// Checked addition still rejects a genuinely unrepresentable result.
        exact_add_overflow,
        program: vec![
            add_instr!(Push, Immediate::MAX),
            add_instr!(Push, 1),
            add_instr!(Add, 0, 1),
        ],
        verifier: { error VerifierError::ArithmeticOverflow },
        executor: { error ExecutorError::ArithmeticOverflow },
    }

    test_program! {
        /// Checked multiplication still rejects a genuinely unrepresentable result.
        exact_mul_overflow,
        program: vec![
            add_instr!(Push, Immediate::MAX),
            add_instr!(Push, 2),
            add_instr!(Mul, 0, 1),
        ],
        verifier: { error VerifierError::ArithmeticOverflow },
        executor: { error ExecutorError::ArithmeticOverflow },
    }
}

// ---------------------------------------------------------------------------
// Conditionals
// ---------------------------------------------------------------------------

mod ifelse {
    use super::*;

    test_program! {
        /// [POSITIVE] A simple ifelse with a statically known true condition.
        known_true,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(Push, 5),
            add_instr!(CmpGreaterThan, 0, 1), // 10 > 5 -> known true
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // taken
                add_instr!(Push, 0)   // not taken
            ),
        ],
        verifier: { stack [10, 5, 1, 42] },
        executor: { stack [10, 5, 1, 42] },
    }

    test_program! {
        /// [POSITIVE] A simple ifelse with a statically known false condition.
        known_false,
        program: vec![
            add_instr!(Push, 3),
            add_instr!(Push, 5),
            add_instr!(CmpGreaterThan, 0, 1), // 3 > 5 -> known false
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // not taken
                add_instr!(Push, 0)   // taken
            ),
        ],
        verifier: { stack [3, 5, 0, 0] },
        executor: { stack [3, 5, 0, 0] },
    }

    test_program! {
        /// [POSITIVE] Both statically selected branches accept ordered instruction
        /// sequences. Direct pushes and pops remain visible on the surrounding
        /// stack, while nested blocks retain isolated result-collapse behavior.
        multiple_instructions_known_branches,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(ifelse 0,
                [
                    add_instr!(Push, 10),
                    add_instr!(Push, 20),
                    add_instr!(R Pop, 1),
                    make_block!(1,
                        add_instr!(Push, 5),
                        add_instr!(Add, 0, 1)
                    ),
                ],
                [],
            ),
            add_instr!(Push, 0),
            add_instr!(ifelse 3,
                [],
                [
                    add_instr!(Push, 30),
                    add_instr!(Push, 40),
                    add_instr!(R Pop, 1),
                    make_block!(1,
                        add_instr!(Push, 7),
                        add_instr!(Add, 0, 1)
                    ),
                ],
            ),
        ],
        verifier: { stack [1, 10, 15, 0, 30, 37] },
        executor: { stack [1, 10, 15, 0, 30, 37] },
    }

    test_program! {
        /// [POSITIVE] An unknown condition explores every instruction in both
        /// balanced sequences before merging their resulting cells.
        multiple_instructions_unknown_balanced,
        program: vec![
            add_instr!(Input),
            add_instr!(ifelse 0,
                [
                    add_instr!(Push, 10),
                    add_instr!(Push, 11),
                ],
                [
                    add_instr!(Push, 20),
                    add_instr!(Push, 21),
                ],
            ),
        ],
        verifier: { stack [ValueSpan::inf(), ValueSpan::new(10, 20), ValueSpan::new(11, 21)] },
        executor: { cases {
            input [1] => stack [1, 10, 11];
            input [0] => stack [0, 20, 21]
        } },
    }

    test_program! {
        /// [POSITIVE] Empty branch lists are no-ops. This also exercises empty
        /// list inference and trailing commas in the bracket-list macro form.
        empty_branches_are_noops,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(ifelse 0, [], [],),
            add_instr!(Push, 2),
        ],
        verifier: { stack [1, 2] },
        executor: { stack [1, 2] },
    }

    test_program! {
        /// [POSITIVE] An ifelse with an unknown condition but balanced branches.
        unknown_balanced,
        program: vec![
            add_instr!(Input), // [?]
            add_instr!(Push, 5),
            add_instr!(CmpGreaterThan, 0, 1), // unknown condition
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // +1 cell if taken
                add_instr!(Push, 0)   // +1 cell if not taken (balanced)
            ),
        ],
        verifier: { stack [ValueSpan::inf(), 5, ValueSpan::new(0, 1), ValueSpan::new(0, 42)] },
        executor: { cases {
            input [100] => stack [100, 5, 1, 42];
            input [-100] => stack [-100, 5, 0, 0]
        } },
    }

    test_program! {
        /// [POSITIVE] Statically-known true condition: only the taken branch runs.
        known_true_asymmetric,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(Push, 3),
            add_instr!(CmpGreaterThan, 0, 1), // 10 > 3 -> known true
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // taken: +1 cell
                add_instr!(R Pop, 2)  // dead:  would be -2 cells
            ),
        ],
        verifier: { stack [10, 3, 1, 42] },
        executor: { stack [10, 3, 1, 42] },
    }

    test_program! {
        /// [POSITIVE] Statically-known false condition: only the false branch runs.
        known_false_asymmetric,
        program: vec![
            add_instr!(Push, 0),
            add_instr!(Push, 3),
            add_instr!(CmpGreaterThan, 0, 1), // 3 > 10 -> known false
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // dead
                add_instr!(R Pop, 2)  // taken: +1 cell
            ),
        ],
        verifier: { stack [0] },
        executor: { stack [0] },
    }

    test_program! {
        /// [NEGATIVE] An ifelse with an unknown condition and unbalanced branches.
        ///
        /// The verifier rejects the program; the executor cannot detect this and
        /// produces differently-sized stacks depending on the input.
        unknown_unbalanced,
        program: vec![
            add_instr!(Input), // [?]
            add_instr!(Push, 5),
            add_instr!(CmpGreaterThan, 0, 1), // unknown condition
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // +1 cell if taken
                add_instr!(R Pop, 1)  // -1 cell if not taken (unbalanced)
            ),
        ],
        verifier: { error VerifierError::CondUnequalStackSizes {
            true_branch_cells: 4,
            false_branch_cells: 2
            }
        },
        executor: { cases {
            input [100] => stack [100, 5, 1, 42];
            input [-100] => stack [-100, 5]
            }
        },
    }

    test_program! {
        /// [NEGATIVE] No condition on the stack at all when ifelse runs.
        no_condition,
        program: vec![
            add_instr!(ifelse 0,
                add_instr!(Push, 1),
                add_instr!(Push, 2)
            )
        ],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [POSITIVE] An `IfElse` inside a block: the branch's marker frame is
        /// popped when the branch ends, so the enclosing block can exit normally.
        ///
        /// Regression test: the marker frame used to be leaked, and the block's
        /// later `exit_block` panicked on it in both runners.
        inside_block,
        program: vec![
            make_block!(0,
                add_instr!(Push, 1),
                add_instr!(ifelse 0,
                    add_instr!(Push, 2),
                    add_instr!(Push, 3)
                )
            )
        ],
        verifier: { stack [2] },
        executor: { stack [2] },
    }
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

mod blocks {
    use super::*;

    test_program! {
        /// [NEGATIVE] Empty blocks are prohibited.
        empty_block,
        program: {
            let program: Vec<Instruction> = vec![make_block!(0)];
            program
        },
        verifier: { error VerifierError::EmptyBlock },
        executor: { error ExecutorError::EmptyBlock },
    }

    test_program! {
        /// [POSITIVE] A block with some instructions is fine.
        simple,
        program: vec![
            make_block!(0,
                add_instr!(Push, 42)
            )
        ],
        verifier: { stack [42] },
        executor: { stack [42] },
    }

    test_program! {
        /// [POSITIVE] A block can return a value (the last push in the block).
        return_val,
        program: vec![
            add_instr!(Push, 10),
            make_block!(0,
                add_instr!(Push, 20),
                add_instr!(Push, 30)
            ),
        ],
        verifier: { stack [10, 30] },
        executor: { stack [10, 30] },
    }

    test_program! {
        /// [POSITIVE] Blocks can be nested.
        nested,
        program: vec![
            add_instr!(Push, 10),
            make_block!(0,
                add_instr!(Push, 20),
                make_block!(0,
                    add_instr!(Push, 30)
                )
            ),
        ],
        verifier: { stack [10, 30] },
        executor: { stack [10, 30] },
    }

    test_program! {
        /// [NEGATIVE] A block that pops more than it pushes.
        stack_underflow,
        program: vec![make_block!(0,
            add_instr!(Push, 10),
            add_instr!(R Pop, 2) // Error: tries to pop 2 but only 1 on block stack
        )],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [NEGATIVE] A block must return a value.
        no_return_val,
        program: vec![
            add_instr!(Push, 10),
            make_block!(1,
                add_instr!(R Pop, 1) // Pops the only value, leaving block with no return value
            ),
        ],
        verifier: { error VerifierError::BlockHasEmptyStack },
        executor: { error ExecutorError::BlockHasEmptyStack },
    }

    // ---------------------------------------------------------------------------
    // Isolated block indexing
    // ---------------------------------------------------------------------------

    test_program! {
        /// [POSITIVE] An isolated block starts local index counting at zero.
        isolated_indexing_simple,
        program: vec![
            add_instr!(Push, 10),
            make_block!(0,
                add_instr!(Push, 20),
                add_instr!(Add, 0, 0) // 20 + 20 = 40
            ),
        ],
        verifier: { stack [10, 40] },
        executor: { stack [10, 40] },
    }

    test_program! {
        /// [POSITIVE] A zero-argument block starts with an empty local stack.
        isolated_zero_arguments,
        program: vec![make_block!(0,
            add_instr!(Push, 20),
            add_instr!(Add, 0, 0) // 20 + 20 = 40
        )],
        verifier: { stack [40] },
        executor: { stack [40] },
    }

    test_program! {
        /// [POSITIVE] Nested blocks each establish an isolated local stack.
        isolated_nested,
        program: vec![
            add_instr!(Push, 10),
            make_block!(0,
                add_instr!(Push, 20),
                make_block!(0,
                    add_instr!(Push, 30),
                    add_instr!(Add, 0, 0) // 30 + 30 = 60
                ),
                add_instr!(Add, 0, 1) // 20 + 60 = 80
            ),
        ],
        verifier: { stack [10, 80] },
        executor: { stack [10, 80] },
    }

    test_program! {
        /// [POSITIVE] A nested block can clone an ordered suffix of parent locals.
        isolated_nested_arguments,
        program: vec![
            add_instr!(Push, 10),
            make_block!(0,
                add_instr!(Push, 20),
                add_instr!(Push, 30),
                make_block!(2,
                    add_instr!(Push, 40),
                    add_instr!(Add, 0, 2) // 20 + 40 = 60
                ),
                add_instr!(Add, 1, 2) // 30 + 60 = 90
            ),
        ],
        verifier: { stack [10, 90] },
        executor: { stack [10, 90] },
    }

    test_program! {
        /// [NEGATIVE] A block cannot request more arguments than the caller has.
        not_enough_arguments,
        program: vec![
            add_instr!(Push, 10),
            make_block!(2,
                add_instr!(Nop)
            ),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { required: 2, available: 1 }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { required: 2, available: 1 }) },
    }

    test_program! {
        /// [NEGATIVE] `Pop` cannot cross an isolated block boundary.
        isolated_pop_underflow,
        program: vec![
            add_instr!(Push, 10),
            make_block!(0,
                add_instr!(R Pop, 1), // Error
            ),
        ],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [POSITIVE] An `IfElse` branch may itself be an isolated block.
        isolated_ifelse_branch,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(Push, 5),
            add_instr!(CmpGreaterThan, 0, 1),
            add_instr!(ifelse 2,
                make_block!(0,
                    add_instr!(Push, 10),
                    add_instr!(Add, 0, 0) // 10 + 10 = 20
                ),
                make_block!(0,
                    add_instr!(Push, 20),
                    add_instr!(Add, 0, 0) // 20 + 20 = 40
                )
            ),
        ],
        verifier: { stack [10, 5, 1, 20] },
        executor: { stack [10, 5, 1, 20] },
    }

    test_program! {
        /// [POSITIVE] A nested isolated block can run after an earlier `IfElse`.
        isolated_after_ifelse,
        program: vec![
            add_instr!(Push, 10),
            make_block!(1,
                add_instr!(Push, 1),
                add_instr!(ifelse 1,
                    add_instr!(Push, 2),
                    add_instr!(Push, 3)
                ),
                make_block!(2,
                    add_instr!(Add, 0, 1) // 1 + 2 = 3
                )
            ),
        ],
        verifier: { stack [10, 3] },
        executor: { stack [10, 3] },
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

mod functions {
    use super::*;

    const FUNC_NAME: &str = "generic_function_name";
    const INNER: &str = "inner";
    const OUTER: &str = "outer";

    test_program! {
        /// [NEGATIVE] A non-block instruction is not a function body.
        direct,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            add_instr!(Push, 3),

            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody(name))) if name == FUNC_NAME },
        executor: { error ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody(name))) if name == FUNC_NAME },
    }

    test_program! {
        /// [POSITIVE] A simple function that doubles its single argument.
        simple,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(1,
                add_instr!(Mul, 0, 0) // Multiply input by 2
            ),
            add_instr!(Push, 3),
            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { stack [3, 9] },
        executor: { stack [3, 9] },
    }

    test_program! {
        /// [NEGATIVE] Function call with no argument provided.
        no_args,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(1,
                add_instr!(Mul, 0, 0) // Multiply input by 2
            ),
            // Here, we forget to push an argument.
            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { required: 1, available: 0 }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { required: 1, available: 0 }) },
    }

    test_program! {
        /// [NEGATIVE] A function definition cannot be another definition's body.
        sequential_defs,
        program: vec![
            add_instr!(fun FunctionDefine, "push2_1"),
            add_instr!(fun FunctionDefine, "push2_2"),
            add_instr!(fun FunctionDefine, "push2_3"),
            add_instr!(Push, 2),
            add_instr!(fun FunctionCall, "push2_1"),
            add_instr!(fun FunctionCall, "push2_2"),
        ],
        verifier: { error VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody(name))) if name == "push2_1" },
        executor: { error ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody(name))) if name == "push2_1" },
    }

    test_program! {
        /// A recursive call reached inside a function body is summarized rather
        /// than traversed, even when its branch is statically known. This keeps
        /// verification finite and yields a conservative result.
        infinite_recursion_known_branch,
        program: vec![
            add_instr!(fun FunctionDefine, "f"),
            make_block!(0,
                add_instr!(Push, 10),
                add_instr!(Push, 5),
                add_instr!(CmpGreaterThan, 0, 1),
                add_instr!(ifelse 2,
                    add_instr!(fun FunctionCall, "f"),
                    add_instr!(Push, 0)
                )
            ),
            add_instr!(fun FunctionCall, "f"),
        ],
        verifier_only: { stack [ValueSpan::inf()] },
    }

    test_program! {
        /// A recursive call reached inside a function body is summarized rather
        /// than traversed. The uncertain branch merge remains conservative.
        recursion_unknown_branch,
        program: vec![
            add_instr!(fun FunctionDefine, "f"),
            make_block!(0,
                add_instr!(Input),
                add_instr!(Push, 0),
                add_instr!(CmpGreaterThan, 0, 1),
                add_instr!(ifelse 2,
                    add_instr!(fun FunctionCall, "f"),
                    add_instr!(Push, 0)
                )
            ),
            add_instr!(fun FunctionCall, "f"),
        ],
        verifier_only: { stack [ValueSpan::inf()] },
    }

    test_program! {
        /// [NEGATIVE] Nested function definitions are prohibited.
        /// Note: Executor cannot catch this; nested function will be executed.
        nested_defs,
        program: vec![
            add_instr!(fun FunctionDefine, OUTER),
            make_block!(0,
                add_instr!(fun FunctionDefine, INNER),
                make_block!(0,
                    add_instr!(Push, 42)
                ),
                add_instr!(fun FunctionCall, INNER)
            ),
            add_instr!(fun FunctionCall, OUTER),
        ],
        split,
        verifier: { error VerifierError::NestedFunctionDefinition {
            outer_function: o,
            inner_function: i
        } if o == OUTER && i == INNER },
        executor: { stack [42] },
    }

    test_program! {
        /// [POSITIVE] A function that takes 3 arguments and adds them together.
        multi_args,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(3,
                add_instr!(Add, 0, 1), // a + b
                add_instr!(Add, 3, 2)  // (a + b) + c
            ),
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { stack [10, 20, 30, 60] },
        executor: { stack [10, 20, 30, 60] },
    }

    test_program! {
        /// [NEGATIVE] A multi-argument function, but not all arguments are provided.
        multi_args_missing,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(3,
                add_instr!(Add, 0, 1), // a + b
                add_instr!(Add, 3, 2)  // (a + b) + c
            ),
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            // Missing third argument
            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { required: 3, available: 2 }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { required: 3, available: 2 }) },
    }

    test_program! {
        /// [POSITIVE] A function that takes 3 arguments (alternative argument-passing style).
        multi_args_alt,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(3,
                add_instr!(Add, 0, 1), // a + b
                add_instr!(Add, 3, 2)  // (a + b) + c
            ),
            // Main program starts here
            make_block!(0,
                add_instr!(Push, 10),
                add_instr!(Push, 20),
                add_instr!(Push, 30),
                add_instr!(fun FunctionCall, FUNC_NAME),
            ),
            make_block!(0,
                add_instr!(Push, 40),
                add_instr!(Push, 50),
                add_instr!(Push, 60),
                add_instr!(fun FunctionCall, FUNC_NAME),
            ),
            make_block!(0,
                add_instr!(Push, 70),
                add_instr!(Push, 80),
                add_instr!(Push, 90),
                add_instr!(fun FunctionCall, FUNC_NAME),
            ),
            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { stack [60, 150, 240, 450] },
        executor: { stack [60, 150, 240, 450] },
    }

    test_program! {
        /// [NEGATIVE] Alternative argument-passing style, but not all arguments are provided.
        multi_args_alt_missing,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(3,
                add_instr!(Add, 0, 1), // a + b
                add_instr!(Add, 3, 2)  // (a + b) + c
            ),
            // Main program starts here
            make_block!(0,
                add_instr!(Push, 10),
                add_instr!(Push, 20),
                // Missing third argument
                add_instr!(fun FunctionCall, FUNC_NAME),
            ),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { required: 3, available: 2 }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { required: 3, available: 2 }) },
    }

    test_program! {
        /// A function defined inside an ifelse branch only conditionally exists,
        /// so the verifier rejects the definition directly. The executor runs the
        /// concrete branch, registers the function, and calls it successfully.
        defined_inside_branch,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(ifelse 0,
                make_block!(0,
                    add_instr!(fun FunctionDefine, FUNC_NAME),
                    make_block!(0,
                        add_instr!(Push, 42)
                    ),
                    add_instr!(Push, 7)
                ),
                add_instr!(Push, 7)
            ),
            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        split,
        verifier: { error VerifierError::ConditionalDefinition { .. } },
        executor: { stack [1, 7, 42] },
    }

    test_program! {
        /// [NEGATIVE] Declared block arity is checked at the call site before
        /// interpreting the body, including when the argument is only read in
        /// an ifelse branch.
        arg_read_inside_branch,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(1,
                add_instr!(Push, 1),
                add_instr!(ifelse 0,
                    add_instr!(R Read, 0), // reads the declared argument
                    add_instr!(Nop)
                )
            ),
            // No caller cells provided at all.
            add_instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { .. }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { .. }) },
    }

    test_program! {
        /// [NEGATIVE] Missing function body
        missing_body,
        program: vec![
            add_instr!(fun FunctionDefine, FUNC_NAME)
        ],
        verifier: { error VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody( .. ))) },
        executor: { error ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody( .. ))) },
    }
}

mod showcases {
    use virtual_machine::{
        add_instr,
        machine::{executor::Executor, verifier::Verifier},
        make_block, test_program,
        types::{IoBuffer, Value},
    };

    /// Reference factorial used to check executor results.
    const fn factorial_helper(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        n * factorial_helper(n - 1)
    }

    /// Reference Fibonacci used to check executor results.
    const fn fibonacci_helper(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        fibonacci_helper(n - 1) + fibonacci_helper(n - 2)
    }

    /// Reference palindrome-number check used to check executor results.
    const fn is_palindrome_helper(x: i64) -> i64 {
        if x < 0 {
            return 0;
        }

        let mut remaining = x;
        let mut reversed = 0;
        while remaining != 0 {
            reversed = reversed * 10 + remaining % 10;
            remaining /= 10;
        }

        (reversed == x) as i64
    }

    /// Reference Euclidean GCD used to check executor results.
    const fn gcd_helper(mut first: i64, mut second: i64) -> i64 {
        while second != 0 {
            let remainder = first % second;
            first = second;
            second = remainder;
        }
        first
    }

    /// Reference exponentiation by squaring used to check executor results.
    const fn power_helper(base: i64, exponent: i64) -> i64 {
        if exponent == 0 {
            return 1;
        }

        let half_power = power_helper(base, exponent / 2);
        let square = half_power * half_power;
        if exponent % 2 == 0 {
            square
        } else {
            square * base
        }
    }

    /// Reference population count used to check executor results.
    const fn population_count_helper(mut value: i64) -> i64 {
        let mut count = 0;
        while value != 0 {
            count += value & 1;
            value = ((value as u64) >> 1) as i64;
        }
        count
    }

    /// Reference trial-division primality check used to check executor results.
    const fn is_prime_helper(value: i64) -> i64 {
        if value < 2 {
            return 0;
        }

        let mut divisor = 2;
        while divisor <= value / divisor {
            if value % divisor == 0 {
                return 0;
            }
            divisor += 1;
        }
        1
    }

    test_program! {
        /// Recursive factorial.
        factorial,
        program: vec![
            add_instr!(fun FunctionDefine, "factorial"),
            make_block!(1,
                add_instr!(Push, 1),              // 1
                add_instr!(CmpGreaterThan, 0, 1), // n > 1
                add_instr!(ifelse 2, // if n <= 1, skip to return
                    make_block!(3,
                        add_instr!(Push, -1),
                        add_instr!(Add, 0, 3), // n - 1
                        add_instr!(fun FunctionCall, "factorial"), // else, factorial(n - 1)
                        add_instr!(Mul, 0, 5)                      // n * factorial(n - 1)
                    ),
                    add_instr!(Push, 1)
                )
            ),
            add_instr!(Input),
            add_instr!(fun FunctionCall, "factorial"),
        ],
        verifier: { custom |program| {
            assert!(Verifier::new(program.clone()).verify().is_ok());
        } },
        executor: { custom |program| {
            let n = 5;
            let executor = Executor::new(program.clone())
                .redirect_input(IoBuffer::new(vec![n]).into())
                .exec()
                .unwrap();
            assert_eq!(executor.values().last().copied(), Some(Value::Integer(factorial_helper(n))));
        } },
    }

    test_program! {
        /// Recursive Fibonacci.
        fibonacci,
        program: vec![
            add_instr!(fun FunctionDefine, "fibonacci"),
            make_block!(1,
                add_instr!(Push, 1),              // 2
                add_instr!(CmpGreaterThan, 0, 1), // n > 2
                add_instr!(ifelse 2, // if n <= 1, skip to return
                    make_block!(3,
                        add_instr!(Push, -1),
                        add_instr!(Add, 0, 3), // n - 1
                        add_instr!(fun FunctionCall, "fibonacci"), // fibonacci(n - 1)
                        add_instr!(Add, 4, 3),                     // (n - 1) - 1 = n - 2
                        add_instr!(fun FunctionCall, "fibonacci"), // fibonacci(n - 2)
                        add_instr!(Add, 5, 7) // fibonacci(n - 1) + fibonacci(n - 2)
                    ),
                    add_instr!(Push, 1)
                )
            ),
            add_instr!(Input),
            add_instr!(fun FunctionCall, "fibonacci"),
        ],
        verifier: { custom |program| {
            assert!(Verifier::new(program.clone()).verify().is_ok());
        } },
        executor: { custom |program| {
            let n = 5;
            let executor = Executor::new(program.clone())
                .redirect_input(IoBuffer::new(vec![n]).into())
                .exec()
                .unwrap();
            assert_eq!(executor.values().last().copied(), Some(Value::Integer(fibonacci_helper(n))));
        } },
    }

    test_program! {
        /// Palindrome number using recursive decimal-digit reversal.
        palindrome_number,
        program: vec![
            // reverse_digits(remaining, reversed) recursively computes the decimal
            // reverse. `remaining % 10` is expressed as
            // `remaining - (remaining / 10) * 10` because the VM has no remainder
            // instruction.
            add_instr!(fun FunctionDefine, "reverse_digits"),
            make_block!(2,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 2), // remaining == 0
                add_instr!(ifelse 3,
                    add_instr!(R Read, 1), // return reversed
                    make_block!(4,
                        add_instr!(Push, 10),
                        add_instr!(Div, 0, 4), // quotient = remaining / 10
                        add_instr!(Mul, 5, 4), // quotient * 10
                        add_instr!(Push, -1),
                        add_instr!(Mul, 6, 7), // -(quotient * 10)
                        add_instr!(Add, 0, 8), // digit = remaining % 10
                        add_instr!(Mul, 1, 4), // reversed * 10
                        add_instr!(Add, 10, 9), // reversed * 10 + digit
                        add_instr!(R Read, 5), // next remaining (quotient)
                        add_instr!(R Read, 11), // next reversed
                        add_instr!(fun FunctionCall, "reverse_digits")
                    )
                )
            ),
            // is_palindrome(x) rejects negative values, reverses non-negative x,
            // and compares the result with the original input.
            add_instr!(fun FunctionDefine, "is_palindrome"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpLessThan, 0, 1), // x < 0
                add_instr!(ifelse 2,
                    add_instr!(Push, 0),
                    make_block!(3,
                        add_instr!(R Read, 0), // remaining
                        add_instr!(Push, 0),   // reversed
                        add_instr!(fun FunctionCall, "reverse_digits"),
                        add_instr!(CmpEqual, 0, 5)
                    )
                )
            ),
            add_instr!(Input),
            add_instr!(fun FunctionCall, "is_palindrome"),
        ],
        verifier: { custom |program| {
            assert!(Verifier::new(program.clone()).verify().is_ok());
        } },
        executor: { custom |program| {
            for x in [121, -121, 10, 0, 12321, 123] {
                let executor = Executor::new(program.clone())
                    .redirect_input(IoBuffer::new(vec![x]).into())
                    .exec()
                    .unwrap();
                assert_eq!(
                    executor.values().last().copied(),
                    Some(Value::Integer(is_palindrome_helper(x))),
                    "unexpected result for x = {x}"
                );
            }
        } },
    }

    test_program! {
        /// Euclidean GCD for nonnegative integer operands.
        euclidean_gcd,
        program: vec![
            add_instr!(fun FunctionDefine, "gcd"),
            make_block!(2,
                add_instr!(Push, 0),          // cell 2: zero constant
                add_instr!(CmpEqual, 1, 2),   // cell 3: second operand == 0
                add_instr!(ifelse 3,
                    add_instr!(R Read, 0), // cell 4: return first operand
                    make_block!(4,
                        add_instr!(Div, 0, 1), // cell 4: quotient
                        add_instr!(Mul, 4, 1), // cell 5: quotient * divisor
                        add_instr!(Push, -1), // cell 6: negation constant
                        add_instr!(Mul, 5, 6), // cell 7: -(quotient * divisor)
                        add_instr!(Add, 0, 7), // cell 8: remainder
                        add_instr!(R Read, 1), // cell 9: next first argument
                        add_instr!(R Read, 8), // cell 10: next second argument
                        add_instr!(fun FunctionCall, "gcd") // cell 11: recursive result
                    )
                )
            ),
            add_instr!(Input), // cell 0: first operand
            add_instr!(Input), // cell 1: second operand
            add_instr!(fun FunctionCall, "gcd"), // cell 2: GCD
        ],
        verifier: { custom |program| {
            assert!(Verifier::new(program.clone()).verify().is_ok());
        } },
        executor: { custom |program| {
            for (first, second) in [(0, 0), (42, 0), (0, 42), (54, 24), (1071, 462), (13, 17)] {
                let executor = Executor::new(program.clone())
                    .redirect_input(IoBuffer::new(vec![second, first]).into())
                    .exec()
                    .unwrap();
                assert_eq!(
                    executor.values().last().copied(),
                    Some(Value::Integer(gcd_helper(first, second))),
                    "unexpected result for first = {first}, second = {second}"
                );
            }
        } },
    }

    test_program! {
        /// Integer exponentiation by squaring for nonnegative exponents.
        exponentiation_by_squaring,
        program: vec![
            add_instr!(fun FunctionDefine, "power"),
            make_block!(2,
                add_instr!(Push, 0),          // cell 2: zero constant
                add_instr!(CmpEqual, 1, 2),   // cell 3: exponent == 0
                add_instr!(ifelse 3,
                    add_instr!(Push, 1), // cell 4: base-case result
                    make_block!(4,
                        add_instr!(Push, 2), // cell 4: divisor
                        add_instr!(Div, 1, 4), // cell 5: half-exponent
                        add_instr!(R Read, 0), // cell 6: recursive base argument
                        add_instr!(R Read, 5), // cell 7: recursive exponent argument
                        add_instr!(fun FunctionCall, "power"), // cell 8: half-power
                        add_instr!(Mul, 8, 8), // cell 9: squared half-power
                        add_instr!(Mul, 5, 4), // cell 10: twice half-exponent
                        add_instr!(Push, -1), // cell 11: negation constant
                        add_instr!(Mul, 10, 11), // cell 12: negated doubled value
                        add_instr!(Add, 1, 12), // cell 13: oddness (zero or one)
                        add_instr!(ifelse 13,
                            add_instr!(Mul, 9, 0), // cell 14: odd result
                            add_instr!(R Read, 9) // cell 14: even result
                        )
                    )
                )
            ),
            add_instr!(Input), // cell 0: base
            add_instr!(Input), // cell 1: exponent
            add_instr!(fun FunctionCall, "power"), // cell 2: power
        ],
        verifier: { custom |program| {
            assert!(Verifier::new(program.clone()).verify().is_ok());
        } },
        executor: { custom |program| {
            for (base, exponent) in [(2, 0), (0, 0), (0, 5), (2, 10), (-3, 4), (-3, 5), (3, 13)] {
                let executor = Executor::new(program.clone())
                    .redirect_input(IoBuffer::new(vec![exponent, base]).into())
                    .exec()
                    .unwrap();
                assert_eq!(
                    executor.values().last().copied(),
                    Some(Value::Integer(power_helper(base, exponent))),
                    "unexpected result for base = {base}, exponent = {exponent}"
                );
            }
        } },
    }

    test_program! {
        /// Population count for nonnegative integers using logical right shifts.
        population_count,
        program: vec![
            add_instr!(fun FunctionDefine, "popcount"),
            make_block!(1,
                add_instr!(Push, 0),          // cell 1: zero constant
                add_instr!(CmpEqual, 0, 1),   // cell 2: remaining bits == 0
                add_instr!(ifelse 2,
                    add_instr!(Push, 0), // cell 3: base-case count
                    make_block!(3,
                        add_instr!(Push, 1), // cell 3: mask and shift amount
                        add_instr!(And, 0, 3), // cell 4: low bit
                        add_instr!(ShiftRightLogical, 0, 3), // cell 5: remaining bits
                        add_instr!(fun FunctionCall, "popcount"), // cell 6: recursive count
                        add_instr!(Add, 4, 6) // cell 7: population count
                    )
                )
            ),
            add_instr!(Input), // cell 0: input value
            add_instr!(fun FunctionCall, "popcount"), // cell 1: population count
        ],
        verifier: { custom |program| {
            assert!(Verifier::new(program.clone()).verify().is_ok());
        } },
        executor: { custom |program| {
            for value in [0, 1, 2, 7, 45, 240, 1024, 1_099_511_627_775] {
                let executor = Executor::new(program.clone())
                    .redirect_input(IoBuffer::new(vec![value]).into())
                    .exec()
                    .unwrap();
                assert_eq!(
                    executor.values().last().copied(),
                    Some(Value::Integer(population_count_helper(value))),
                    "unexpected result for value = {value}"
                );
            }
        } },
    }

    test_program! {
        /// Trial-division primality test with overflow-safe termination.
        primality_test,
        program: vec![
            add_instr!(fun FunctionDefine, "is_prime_trial"),
            make_block!(2,
                add_instr!(Div, 0, 1),        // cell 2: number / divisor
                add_instr!(CmpGreaterThan, 1, 2), // cell 3: divisor > quotient
                add_instr!(ifelse 3,
                    add_instr!(Push, 1), // cell 4: no divisor remains
                    make_block!(4,
                        add_instr!(Mul, 2, 1), // cell 4: quotient * divisor
                        add_instr!(Push, -1), // cell 5: negation constant
                        add_instr!(Mul, 4, 5), // cell 6: negated product
                        add_instr!(Add, 0, 6), // cell 7: derived remainder
                        add_instr!(Push, 0), // cell 8: zero constant
                        add_instr!(CmpEqual, 7, 8), // cell 9: divisible
                        add_instr!(ifelse 9,
                            add_instr!(Push, 0), // cell 10: composite result
                            make_block!(10,
                                add_instr!(Push, 1), // cell 10: increment constant
                                add_instr!(Add, 1, 10), // cell 11: next divisor
                                add_instr!(R Read, 0), // cell 12: recursive number argument
                                add_instr!(R Read, 11), // cell 13: recursive divisor argument
                                add_instr!(fun FunctionCall, "is_prime_trial") // cell 14: result
                            )
                        )
                    )
                )
            ),
            add_instr!(fun FunctionDefine, "is_prime"),
            make_block!(1,
                add_instr!(Push, 2),          // cell 1: minimum prime/divisor
                add_instr!(CmpLessThan, 0, 1), // cell 2: number < 2
                add_instr!(ifelse 2,
                    add_instr!(Push, 0), // cell 3: below-two result
                    make_block!(3,
                        add_instr!(R Read, 0), // cell 3: trial number argument
                        add_instr!(Push, 2), // cell 4: initial divisor argument
                        add_instr!(fun FunctionCall, "is_prime_trial") // cell 5: result
                    )
                )
            ),
            add_instr!(Input), // cell 0: number
            add_instr!(fun FunctionCall, "is_prime"), // cell 1: primality result
        ],
        verifier: { custom |program| {
            assert!(Verifier::new(program.clone()).verify().is_ok());
        } },
        executor: { custom |program| {
            for value in [-7, 0, 1, 2, 3, 4, 25, 29, 91, 97] {
                let executor = Executor::new(program.clone())
                    .redirect_input(IoBuffer::new(vec![value]).into())
                    .exec()
                    .unwrap();
                assert_eq!(
                    executor.values().last().copied(),
                    Some(Value::Integer(is_prime_helper(value))),
                    "unexpected result for value = {value}"
                );
            }
        } },
    }
}
