//! Integration tests for core VM behavior.

use virtual_machine::{
    block, instr,
    instruction::Instruction,
    machine::{
        CoreError,
        executor::ExecutorError,
        verifier::{ValueSpan, VerifierError},
    },
    test_program,
    types::{FunctionDataError, Immediate},
};

mod stack {
    use super::*;

    test_program! {
        /// [POSITIVE] Pushes 5 values onto the stack.
        push,
        program: vec![
            instr!(Push, 1),
            instr!(Push, 2),
            instr!(Push, 3),
            instr!(Push, 4),
            instr!(Push, 5),
        ],
        verifier: { stack [1, 2, 3, 4, 5] },
        executor: { stack [1, 2, 3, 4, 5] },
    }

    test_program! {
        /// [POSITIVE] Pushes 3 values, then pops 2, leaving 1 on the stack.
        pop_most,
        program: vec![
            instr!(Push, 1),
            instr!(Push, 2),
            instr!(Push, 3),
            instr!(R Pop, 2),
        ],
        verifier: { stack [1] },
        executor: { stack [1] },
    }

    test_program! {
        /// [POSITIVE] Pushes 3 values, then pops all 3, leaving the stack empty.
        pop_all,
        program: vec![
            instr!(Push, 1),
            instr!(Push, 2),
            instr!(Push, 3),
            instr!(R Pop, 3),
        ],
        verifier: { stack [] },
        executor: { stack [] },
    }

    test_program! {
        /// [NEGATIVE] Tries to pop from an empty stack.
        pop_empty,
        program: vec![instr!(R Pop, 1)],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [NEGATIVE] Pushes 3 values, then tries to pop 4.
        pop_too_many,
        program: vec![
            instr!(Push, 1),
            instr!(Push, 2),
            instr!(Push, 3),
            instr!(R Pop, 4),
        ],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [POSITIVE] Pushes a value and reads it back with index 0.
        read,
        program: vec![instr!(Push, 42), instr!(R Read, 0)],
        verifier: { stack [42, 42] },
        executor: { stack [42, 42] },
    }

    test_program! {
        /// [NEGATIVE] Tries to read from an empty stack.
        read_empty,
        program: vec![instr!(R Read, 0)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [POSITIVE] Pushes 3 values and reads them back with indices 0, 1, 2.
        read_multiple,
        program: vec![
            instr!(Push, 10),
            instr!(Push, 20),
            instr!(Push, 30),
            instr!(R Read, 0),
            instr!(R Read, 1),
            instr!(R Read, 2),
        ],
        verifier: { stack [10, 20, 30, 10, 20, 30] },
        executor: { stack [10, 20, 30, 10, 20, 30] },
    }

    test_program! {
        /// [NEGATIVE] Pushes a value and tries to read from index 1.
        read_bad_index,
        program: vec![instr!(Push, 42), instr!(R Read, 1)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [NEGATIVE] Read with index larger than stack size after several pushes.
        read_far_beyond_stack,
        program: vec![instr!(Push, 1), instr!(Push, 2), instr!(R Read, 100)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [POSITIVE] Reads the top of stack.
        read_reverse,
        program: vec![instr!(Push, 10), instr!(R ReadReverse, 0)],
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
            instr!(Push, 10),
            instr!(Push, 20),
            instr!(Push, 30),
            instr!(R ReadReverse, 2),
            instr!(R ReadReverse, 2),
            instr!(R ReadReverse, 2),
        ],
        verifier: { stack [10, 20, 30, 10, 20, 30] },
        executor: { stack [10, 20, 30, 10, 20, 30] },
    }

    test_program! {
        /// [NEGATIVE] Pushes a value and tries to read reverse from index 1.
        read_reverse_bad_index,
        program: vec![instr!(Push, 42), instr!(R ReadReverse, 1)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }

    test_program! {
        /// [NEGATIVE] Tries to read reverse from an empty stack.
        read_reverse_bad_empty,
        program: vec![instr!(R ReadReverse, 0)],
        verifier: { error VerifierError::InvalidCell { .. } },
        executor: { error ExecutorError::InvalidCell },
    }
}

mod arith {
    use super::*;

    test_program! {
        /// [POSITIVE] Mathematical negation appends the negated cell.
        neg,
        program: vec![instr!(Push, 6), instr!(R Neg, 0)],
        verifier: { stack [6, -6] },
        executor: { stack [6, -6] },
    }

    test_program! {
        /// [POSITIVE] Subtraction appends cell1 - cell2 in operand order.
        sub,
        program: vec![
            instr!(Push, 10),
            instr!(Push, 3),
            instr!(Sub, 0, 1),
        ],
        verifier: { stack [10, 3, 7] },
        executor: { stack [10, 3, 7] },
    }

    test_program! {
        /// [POSITIVE] Tests bitwise not.
        bitwise_not,
        program: vec![instr!(Push, 0b1100), instr!(R Not, 0)],
        verifier: { stack [0b1100, !0b1100] },
        executor: { stack [0b1100, !0b1100] },
    }

    test_program! {
        /// [POSITIVE] Bitwise not maps the merged interval `[-2, 4]` exactly to
        /// `[!4, !-2] == [-5, 1]`.
        bitwise_not_interval,
        program: vec![
            instr!(Input),
            instr!(Push, 0),
            instr!(CmpGreaterThan, 0, 1),
            instr!(IfElse 2,
                [
                    instr!(Push, -2),
                ],
                [
                    instr!(Push, 4),
                ],
            ),
            instr!(R Not, 3),
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
        program: vec![instr!(Input), instr!(R Not, 0)],
        verifier: { stack [ValueSpan::inf(), ValueSpan::inf()] },
        executor: { cases {
            input [Immediate::MIN] => stack [Immediate::MIN, Immediate::MAX];
            input [Immediate::MAX] => stack [Immediate::MAX, Immediate::MIN]
        } },
    }

    test_program! {
        /// [POSITIVE] Tests Nop.
        nop,
        program: vec![instr!(Nop)],
        verifier: { stack [] },
        executor: { stack [] },
    }

    test_program! {
        /// [NEGATIVE] Tests division by zero.
        div_by_zero,
        program: vec![
            instr!(Push, 10),
            instr!(Push, 0),
            instr!(Div, 0, 1),
        ],
        verifier: { error VerifierError::DivisionByZero },
        executor: { error ExecutorError::DivisionByZero },
    }

    test_program! {
        /// An exact arithmetic result at the representable boundary is not an
        /// overflow.
        overflow_boundaries_add_exact,
        program: vec![
            instr!(Push, Immediate::MAX),
            instr!(Push, 0),
            instr!(Add, 0, 1),
        ],
        verifier: { stack [Immediate::MAX, 0, Immediate::MAX] },
        executor: { stack [Immediate::MAX, 0, Immediate::MAX] },
    }

    test_program! {
        /// An exact addition result at the lower representable boundary is not
        /// an overflow.
        overflow_boundaries_add,
        program: vec![
            instr!(Push, Immediate::MIN),
            instr!(Push, 0),
            instr!(Add, 0, 1),
        ],
        verifier: { stack [Immediate::MIN, 0, Immediate::MIN] },
        executor: { stack [Immediate::MIN, 0, Immediate::MIN] },
    }

    test_program! {
        /// Exact multiplication may produce either representable boundary.
        overflow_boundaries_mul,
        program: vec![
            instr!(Push, Immediate::MAX),
            instr!(Push, 1),
            instr!(Mul, 0, 1),
            instr!(Push, Immediate::MIN),
            instr!(Mul, 3, 1),
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
            instr!(Push, Immediate::MAX),
            instr!(Push, 1),
            instr!(Add, 0, 1),
        ],
        verifier: { error VerifierError::ArithmeticOverflow },
        executor: { error ExecutorError::ArithmeticOverflow },
    }

    test_program! {
        /// Checked multiplication still rejects a genuinely unrepresentable result.
        exact_mul_overflow,
        program: vec![
            instr!(Push, Immediate::MAX),
            instr!(Push, 2),
            instr!(Mul, 0, 1),
        ],
        verifier: { error VerifierError::ArithmeticOverflow },
        executor: { error ExecutorError::ArithmeticOverflow },
    }
}

mod ifelse {
    use super::*;

    test_program! {
        /// [POSITIVE] A simple ifelse with a statically known true condition.
        known_true,
        program: vec![
            instr!(Push, 10),
            instr!(Push, 5),
            instr!(CmpGreaterThan, 0, 1), // 10 > 5 -> known true
            instr!(IfElse 2,
                [
                    instr!(Push, 42), // taken
                ],
                [
                    instr!(Push, 0), // not taken
                ],
            ),
        ],
        verifier: { stack [10, 5, 1, 42] },
        executor: { stack [10, 5, 1, 42] },
    }

    test_program! {
        /// [POSITIVE] A simple ifelse with a statically known false condition.
        known_false,
        program: vec![
            instr!(Push, 3),
            instr!(Push, 5),
            instr!(CmpGreaterThan, 0, 1), // 3 > 5 -> known false
            instr!(IfElse 2,
                [
                    instr!(Push, 42), // not taken
                ],
                [
                    instr!(Push, 0), // taken
                ],
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
            instr!(Push, 1),
            instr!(IfElse 0,
                [
                    instr!(Push, 10),
                    instr!(Push, 20),
                    instr!(R Pop, 1),
                    block!(1,
                        instr!(Push, 5),
                        instr!(Add, 0, 1)
                    ),
                ],
                [],
            ),
            instr!(Push, 0),
            instr!(IfElse 3,
                [],
                [
                    instr!(Push, 30),
                    instr!(Push, 40),
                    instr!(R Pop, 1),
                    block!(1,
                        instr!(Push, 7),
                        instr!(Add, 0, 1)
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
            instr!(Input),
            instr!(IfElse 0,
                [
                    instr!(Push, 10),
                    instr!(Push, 11),
                ],
                [
                    instr!(Push, 20),
                    instr!(Push, 21),
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
            instr!(Push, 1),
            instr!(IfElse 0, [], [],),
            instr!(Push, 2),
        ],
        verifier: { stack [1, 2] },
        executor: { stack [1, 2] },
    }

    test_program! {
        /// [POSITIVE] An ifelse with an unknown condition but balanced branches.
        unknown_balanced,
        program: vec![
            instr!(Input), // [?]
            instr!(Push, 5),
            instr!(CmpGreaterThan, 0, 1), // unknown condition
            instr!(IfElse 2,
                [
                    instr!(Push, 42), // +1 cell if taken
                ],
                [
                    instr!(Push, 0), // +1 cell if not taken (balanced)
                ],
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
            instr!(Push, 10),
            instr!(Push, 3),
            instr!(CmpGreaterThan, 0, 1), // 10 > 3 -> known true
            instr!(IfElse 2,
                [
                    instr!(Push, 42), // taken: +1 cell
                ],
                [
                    instr!(R Pop, 2), // dead:  would be -2 cells
                ],
            ),
        ],
        verifier: { stack [10, 3, 1, 42] },
        executor: { stack [10, 3, 1, 42] },
    }

    test_program! {
        /// [POSITIVE] Statically-known false condition: only the false branch runs.
        known_false_asymmetric,
        program: vec![
            instr!(Push, 0),
            instr!(Push, 3),
            instr!(CmpGreaterThan, 0, 1), // 3 > 10 -> known false
            instr!(IfElse 2,
                [
                    instr!(Push, 42), // dead
                ],
                [
                    instr!(R Pop, 2), // taken: +1 cell
                ],
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
            instr!(Input), // [?]
            instr!(Push, 5),
            instr!(CmpGreaterThan, 0, 1), // unknown condition
            instr!(IfElse 2,
                [
                    instr!(Push, 42), // +1 cell if taken
                ],
                [
                    instr!(R Pop, 1), // -1 cell if not taken (unbalanced)
                ],
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
            instr!(IfElse 0,
                [
                    instr!(Push, 1),
                ],
                [
                    instr!(Push, 2),
                ],
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
            block!(0,
                instr!(Push, 1),
                instr!(IfElse 0,
                    [
                        instr!(Push, 2),
                    ],
                    [
                        instr!(Push, 3),
                    ],
                )
            )
        ],
        verifier: { stack [2] },
        executor: { stack [2] },
    }
}

mod blocks {
    use super::*;

    test_program! {
        /// [NEGATIVE] Empty blocks are prohibited.
        empty_block,
        program: {
            let program: Vec<Instruction> = vec![block!(0)];
            program
        },
        verifier: { error VerifierError::EmptyBlock },
        executor: { error ExecutorError::EmptyBlock },
    }

    test_program! {
        /// [POSITIVE] A block with some instructions is fine.
        simple,
        program: vec![
            block!(0,
                instr!(Push, 42)
            )
        ],
        verifier: { stack [42] },
        executor: { stack [42] },
    }

    test_program! {
        /// [POSITIVE] A block can return a value (the last push in the block).
        return_val,
        program: vec![
            instr!(Push, 10),
            block!(0,
                instr!(Push, 20),
                instr!(Push, 30)
            ),
        ],
        verifier: { stack [10, 30] },
        executor: { stack [10, 30] },
    }

    test_program! {
        /// [POSITIVE] Blocks can be nested.
        nested,
        program: vec![
            instr!(Push, 10),
            block!(0,
                instr!(Push, 20),
                block!(0,
                    instr!(Push, 30)
                )
            ),
        ],
        verifier: { stack [10, 30] },
        executor: { stack [10, 30] },
    }

    test_program! {
        /// [NEGATIVE] A block that pops more than it pushes.
        stack_underflow,
        program: vec![block!(0,
            instr!(Push, 10),
            instr!(R Pop, 2) // Error: tries to pop 2 but only 1 on block stack
        )],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [NEGATIVE] A block must return a value.
        no_return_val,
        program: vec![
            instr!(Push, 10),
            block!(1,
                instr!(R Pop, 1) // Pops the only value, leaving block with no return value
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
            instr!(Push, 10),
            block!(0,
                instr!(Push, 20),
                instr!(Add, 0, 0) // 20 + 20 = 40
            ),
        ],
        verifier: { stack [10, 40] },
        executor: { stack [10, 40] },
    }

    test_program! {
        /// [POSITIVE] A zero-argument block starts with an empty local stack.
        isolated_zero_arguments,
        program: vec![block!(0,
            instr!(Push, 20),
            instr!(Add, 0, 0) // 20 + 20 = 40
        )],
        verifier: { stack [40] },
        executor: { stack [40] },
    }

    test_program! {
        /// [POSITIVE] Nested blocks each establish an isolated local stack.
        isolated_nested,
        program: vec![
            instr!(Push, 10),
            block!(0,
                instr!(Push, 20),
                block!(0,
                    instr!(Push, 30),
                    instr!(Add, 0, 0) // 30 + 30 = 60
                ),
                instr!(Add, 0, 1) // 20 + 60 = 80
            ),
        ],
        verifier: { stack [10, 80] },
        executor: { stack [10, 80] },
    }

    test_program! {
        /// [POSITIVE] A nested block can clone an ordered suffix of parent locals.
        isolated_nested_arguments,
        program: vec![
            instr!(Push, 10),
            block!(0,
                instr!(Push, 20),
                instr!(Push, 30),
                block!(2,
                    instr!(Push, 40),
                    instr!(Add, 0, 2) // 20 + 40 = 60
                ),
                instr!(Add, 1, 2) // 30 + 60 = 90
            ),
        ],
        verifier: { stack [10, 90] },
        executor: { stack [10, 90] },
    }

    test_program! {
        /// [NEGATIVE] A block cannot request more arguments than the caller has.
        not_enough_arguments,
        program: vec![
            instr!(Push, 10),
            block!(2,
                instr!(Nop)
            ),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { required: 2, available: 1 }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { required: 2, available: 1 }) },
    }

    test_program! {
        /// [NEGATIVE] `Pop` cannot cross an isolated block boundary.
        isolated_pop_underflow,
        program: vec![
            instr!(Push, 10),
            block!(0,
                instr!(R Pop, 1), // Error
            ),
        ],
        verifier: { error VerifierError::StackUnderflow },
        executor: { error ExecutorError::StackUnderflow },
    }

    test_program! {
        /// [POSITIVE] An `IfElse` branch may itself be an isolated block.
        isolated_ifelse_branch,
        program: vec![
            instr!(Push, 10),
            instr!(Push, 5),
            instr!(CmpGreaterThan, 0, 1),
            instr!(IfElse 2,
                [
                    block!(0,
                        instr!(Push, 10),
                        instr!(Add, 0, 0) // 10 + 10 = 20
                    ),
                ],
                [
                    block!(0,
                        instr!(Push, 20),
                        instr!(Add, 0, 0) // 20 + 20 = 40
                    ),
                ],
            ),
        ],
        verifier: { stack [10, 5, 1, 20] },
        executor: { stack [10, 5, 1, 20] },
    }

    test_program! {
        /// [POSITIVE] A nested isolated block can run after an earlier `IfElse`.
        isolated_after_ifelse,
        program: vec![
            instr!(Push, 10),
            block!(1,
                instr!(Push, 1),
                instr!(IfElse 1,
                    [
                        instr!(Push, 2),
                    ],
                    [
                        instr!(Push, 3),
                    ],
                ),
                block!(2,
                    instr!(Add, 0, 1) // 1 + 2 = 3
                )
            ),
        ],
        verifier: { stack [10, 3] },
        executor: { stack [10, 3] },
    }
}

mod functions {
    use super::*;

    const FUNC_NAME: &str = "generic_function_name";
    const INNER: &str = "inner";
    const OUTER: &str = "outer";

    test_program! {
        /// [NEGATIVE] A non-block instruction is not a function body.
        direct,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME),
            instr!(Push, 3),

            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody(name))) if name == FUNC_NAME },
        executor: { error ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody(name))) if name == FUNC_NAME },
    }

    test_program! {
        /// [POSITIVE] A simple function that doubles its single argument.
        simple,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME),
            block!(1,
                instr!(Mul, 0, 0) // Multiply input by 2
            ),
            instr!(Push, 3),
            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { stack [3, 9] },
        executor: { stack [3, 9] },
    }

    test_program! {
        /// [NEGATIVE] Function call with no argument provided.
        no_args,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME),
            block!(1,
                instr!(Mul, 0, 0) // Multiply input by 2
            ),
            // Here, we forget to push an argument.
            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { required: 1, available: 0 }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { required: 1, available: 0 }) },
    }

    test_program! {
        /// [NEGATIVE] A function definition cannot be another definition's body.
        sequential_defs,
        program: vec![
            instr!(fun FunctionDefine, "push2_1"),
            instr!(fun FunctionDefine, "push2_2"),
            instr!(fun FunctionDefine, "push2_3"),
            instr!(Push, 2),
            instr!(fun FunctionCall, "push2_1"),
            instr!(fun FunctionCall, "push2_2"),
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
            instr!(fun FunctionDefine, "f"),
            block!(0,
                instr!(Push, 10),
                instr!(Push, 5),
                instr!(CmpGreaterThan, 0, 1),
                instr!(IfElse 2,
                    [
                        instr!(fun FunctionCall, "f"),
                    ],
                    [
                        instr!(Push, 0),
                    ],
                )
            ),
            instr!(fun FunctionCall, "f"),
        ],
        verifier_only: { stack [ValueSpan::inf()] },
    }

    test_program! {
        /// A recursive call reached inside a function body is summarized rather
        /// than traversed. The uncertain branch merge remains conservative.
        recursion_unknown_branch,
        program: vec![
            instr!(fun FunctionDefine, "f"),
            block!(0,
                instr!(Input),
                instr!(Push, 0),
                instr!(CmpGreaterThan, 0, 1),
                instr!(IfElse 2,
                    [
                        instr!(fun FunctionCall, "f"),
                    ],
                    [
                        instr!(Push, 0),
                    ],
                )
            ),
            instr!(fun FunctionCall, "f"),
        ],
        verifier_only: { stack [ValueSpan::inf()] },
    }

    test_program! {
        /// [NEGATIVE] Nested function definitions are prohibited.
        /// Note: Executor cannot catch this; nested function will be executed.
        nested_defs,
        program: vec![
            instr!(fun FunctionDefine, OUTER),
            block!(0,
                instr!(fun FunctionDefine, INNER),
                block!(0,
                    instr!(Push, 42)
                ),
                instr!(fun FunctionCall, INNER)
            ),
            instr!(fun FunctionCall, OUTER),
        ],
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
            instr!(fun FunctionDefine, FUNC_NAME),
            block!(3,
                instr!(Add, 0, 1), // a + b
                instr!(Add, 3, 2)  // (a + b) + c
            ),
            instr!(Push, 10),
            instr!(Push, 20),
            instr!(Push, 30),
            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { stack [10, 20, 30, 60] },
        executor: { stack [10, 20, 30, 60] },
    }

    test_program! {
        /// [NEGATIVE] A multi-argument function, but not all arguments are provided.
        multi_args_missing,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME),
            block!(3,
                instr!(Add, 0, 1), // a + b
                instr!(Add, 3, 2)  // (a + b) + c
            ),
            instr!(Push, 10),
            instr!(Push, 20),
            // Missing third argument
            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { required: 3, available: 2 }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { required: 3, available: 2 }) },
    }

    test_program! {
        /// [POSITIVE] A function that takes 3 arguments (alternative argument-passing style).
        multi_args_alt,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME),
            block!(3,
                instr!(Add, 0, 1), // a + b
                instr!(Add, 3, 2)  // (a + b) + c
            ),
            // Main program starts here
            block!(0,
                instr!(Push, 10),
                instr!(Push, 20),
                instr!(Push, 30),
                instr!(fun FunctionCall, FUNC_NAME),
            ),
            block!(0,
                instr!(Push, 40),
                instr!(Push, 50),
                instr!(Push, 60),
                instr!(fun FunctionCall, FUNC_NAME),
            ),
            block!(0,
                instr!(Push, 70),
                instr!(Push, 80),
                instr!(Push, 90),
                instr!(fun FunctionCall, FUNC_NAME),
            ),
            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { stack [60, 150, 240, 450] },
        executor: { stack [60, 150, 240, 450] },
    }

    test_program! {
        /// [NEGATIVE] Alternative argument-passing style, but not all arguments are provided.
        multi_args_alt_missing,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME),
            block!(3,
                instr!(Add, 0, 1), // a + b
                instr!(Add, 3, 2)  // (a + b) + c
            ),
            // Main program starts here
            block!(0,
                instr!(Push, 10),
                instr!(Push, 20),
                // Missing third argument
                instr!(fun FunctionCall, FUNC_NAME),
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
            instr!(Push, 1),
            instr!(IfElse 0,
                [
                    block!(0,
                        instr!(fun FunctionDefine, FUNC_NAME),
                        block!(0,
                            instr!(Push, 42)
                        ),
                        instr!(Push, 7)
                    ),
                ],
                [
                    instr!(Push, 7),
                ],
            ),
            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::ConditionalDefinition { .. } },
        executor: { stack [1, 7, 42] },
    }

    test_program! {
        /// [NEGATIVE] Declared block arity is checked at the call site before
        /// interpreting the body, including when the argument is only read in
        /// an ifelse branch.
        arg_read_inside_branch,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME),
            block!(1,
                instr!(Push, 1),
                instr!(IfElse 0,
                    [
                        instr!(R Read, 0), // reads the declared argument
                    ],
                    [
                        instr!(Nop),
                    ],
                )
            ),
            // No caller cells provided at all.
            instr!(fun FunctionCall, FUNC_NAME),
        ],
        verifier: { error VerifierError::Core(CoreError::NotEnoughArguments { .. }) },
        executor: { error ExecutorError::Core(CoreError::NotEnoughArguments { .. }) },
    }

    test_program! {
        /// [NEGATIVE] Missing function body
        missing_body,
        program: vec![
            instr!(fun FunctionDefine, FUNC_NAME)
        ],
        verifier: { error VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody( .. ))) },
        executor: { error ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody( .. ))) },
    }
}

mod showcases {
    use virtual_machine::{
        block, instr,
        machine::verifier::{ValueSpan},
        test_program,
    };

    /// Helper function to compute factorial recursively.
    const fn factorial_helper(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        n * factorial_helper(n - 1)
    }

    /// Helper function to compute Fibonacci numbers recursively.
    const fn fibonacci_helper(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        fibonacci_helper(n - 1) + fibonacci_helper(n - 2)
    }

    const N: i64 = 5;

    test_program! {
        factorial,
        program: vec![
            instr!(fun FunctionDefine, "factorial"),
            block!(1,
                instr!(Push, 1),              // 1
                instr!(CmpGreaterThan, 0, 1), // n > 1
                instr!(IfElse 2, // if n <= 1, skip to return
                    [
                        block!(3,
                            instr!(Push, -1),
                            instr!(Add, 0, 3), // n - 1
                            instr!(fun FunctionCall, "factorial"), // else, factorial(n - 1)
                            instr!(Mul, 0, 5)                      // n * factorial(n - 1)
                        ),
                    ],
                    [
                        instr!(Push, 1),
                    ],
                )
            ),
            instr!(Input),
            instr!(fun FunctionCall, "factorial"),
        ],
        verifier: { stack [ValueSpan::inf(), ValueSpan::inf()] },
        executor: { cases {
                input [N] => stack [N, factorial_helper(N)];
        } },
    }

    test_program! {
        fibonacci,
        program: vec![
            instr!(fun FunctionDefine, "fibonacci"),
            block!(1,
                instr!(Push, 1),              // 2
                instr!(CmpGreaterThan, 0, 1), // n > 2
                instr!(IfElse 2, // if n <= 1, skip to return
                    [
                        block!(3,
                            instr!(Push, -1),
                            instr!(Add, 0, 3), // n - 1
                            instr!(fun FunctionCall, "fibonacci"), // fibonacci(n - 1)
                            instr!(Add, 4, 3),                     // (n - 1) - 1 = n - 2
                            instr!(fun FunctionCall, "fibonacci"), // fibonacci(n - 2)
                            instr!(Add, 5, 7) // fibonacci(n - 1) + fibonacci(n - 2)
                        ),
                    ],
                    [
                        instr!(Push, 1),
                    ],
                )
            ),
            instr!(Input),
            instr!(fun FunctionCall, "fibonacci"),
        ],
        verifier: { stack [ValueSpan::inf(), ValueSpan::inf()] },
        executor: { cases {
                input [N] => stack [N, fibonacci_helper(N)];
            }
        },
    }
}
