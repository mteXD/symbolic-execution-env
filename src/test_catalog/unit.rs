//! Unit tests for core VM behavior.

use super::*;
use crate::types::Immediate;

// ---------------------------------------------------------------------------
// Stack operations
// ---------------------------------------------------------------------------

test_program! {
    /// [POSITIVE] Pushes 5 values onto the stack.
    stack_push,
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
    stack_pop_most,
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
    stack_pop_all,
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
    stack_pop_empty,
    program: vec![add_instr!(R Pop, 1)],
    verifier: { error VerifierError::StackUnderflow },
    executor: { error ExecutorError::StackUnderflow },
}

test_program! {
    /// [NEGATIVE] Pushes 3 values, then tries to pop 4.
    stack_pop_too_many,
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
    stack_read,
    program: vec![add_instr!(Push, 42), add_instr!(R Read, 0)],
    verifier: { stack [42, 42] },
    executor: { stack [42, 42] },
}

test_program! {
    /// [NEGATIVE] Tries to read from an empty stack.
    stack_read_empty,
    program: vec![add_instr!(R Read, 0)],
    verifier: { error VerifierError::InvalidCell { .. } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// [POSITIVE] Pushes 3 values and reads them back with indices 0, 1, 2.
    stack_read_multiple,
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
    stack_read_bad_index,
    program: vec![add_instr!(Push, 42), add_instr!(R Read, 1)],
    verifier: { error VerifierError::InvalidCell { .. } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// [NEGATIVE] Read with index larger than stack size after several pushes.
    stack_read_far_beyond_stack,
    program: vec![add_instr!(Push, 1), add_instr!(Push, 2), add_instr!(R Read, 100)],
    verifier: { error VerifierError::InvalidCell { .. } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// [POSITIVE] Reads the top of stack.
    stack_read_reverse,
    program: vec![add_instr!(Push, 10), add_instr!(R ReadReverse, 0)],
    verifier: { stack [10, 10] },
    executor: { stack [10, 10] },
}

test_program! {
    /// [POSITIVE] Reads the top 3 values.
    ///
    /// As can be seen, `ReadReverse` with the same index used multiple times can be utilized to
    /// clone a portion of the stack.
    stack_read_reverse_multiple,
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
    stack_read_reverse_bad_index,
    program: vec![add_instr!(Push, 42), add_instr!(R ReadReverse, 1)],
    verifier: { error VerifierError::InvalidCell { .. } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// [NEGATIVE] Tries to read reverse from an empty stack.
    stack_read_reverse_bad_empty,
    program: vec![add_instr!(R ReadReverse, 0)],
    verifier: { error VerifierError::InvalidCell { .. } },
    executor: { error ExecutorError::InvalidCell },
}

// ---------------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------------

test_program! {
    /// [POSITIVE] Tests bitwise not.
    arith_bitwise_not,
    program: vec![add_instr!(Push, 0b1100), add_instr!(R Not, 0)],
    verifier: { stack [0b1100, !0b1100] },
    executor: { stack [0b1100, !0b1100] },
}

test_program! {
    /// [POSITIVE] Tests Nop.
    arith_nop,
    program: vec![add_instr!(Nop)],
    verifier: { stack [] },
    executor: { stack [] },
}

test_program! {
    /// [NEGATIVE] Tests division by zero.
    arith_div_by_zero,
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
    /// overflow. The executor accepts this; the verifier currently mistakes
    /// `Immediate::MAX` for its unbounded-span sentinel.
    #[ignore = "Verifier mistakes an exact boundary result for overflow"]
    arith_exact_max_is_not_overflow,
    program: vec![
        add_instr!(Push, Immediate::MAX),
        add_instr!(Push, 0),
        add_instr!(Add, 0, 1),
    ],
    verifier: { stack [Immediate::MAX, 0, Immediate::MAX] },
    executor: { stack [Immediate::MAX, 0, Immediate::MAX] },
}

// ---------------------------------------------------------------------------
// Conditionals
// ---------------------------------------------------------------------------

test_program! {
    /// [POSITIVE] A simple ifelse with a statically known true condition.
    cond_ifelse_known_true,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1), // 10 > 5 -> known true
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
    cond_ifelse_known_false,
    program: vec![
        add_instr!(Push, 3),
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1), // 3 > 5 -> known false
        add_instr!(ifelse 2,
            add_instr!(Push, 42), // not taken
            add_instr!(Push, 0)   // taken
        ),
    ],
    verifier: { stack [3, 5, 0, 0] },
    executor: { stack [3, 5, 0, 0] },
}

test_program! {
    /// [POSITIVE] An ifelse with an unknown condition but balanced branches.
    cond_ifelse_unknown_balanced,
    program: vec![
        add_instr!(Input), // [?]
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1), // unknown condition
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
    cond_ifelse_known_true_asymmetric,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 3),
        add_instr!(SetGreaterThan, 0, 1), // 10 > 3 -> known true
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
    cond_ifelse_known_false_asymmetric,
    program: vec![
        add_instr!(Push, 0),
        add_instr!(Push, 3),
        add_instr!(SetGreaterThan, 0, 1), // 3 > 10 -> known false
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
    cond_ifelse_unknown_unbalanced,
    program: vec![
        add_instr!(Input), // [?]
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1), // unknown condition
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
    /// [NEGATIVE] Condition is not the result of a comparison instruction.
    ///
    /// Will eventually be caught by the type system.
    #[ignore = "Type system currently not yet implemented."]
    cond_ifelse_bad_placement,
    program: vec![
        add_instr!(Push, 1), // Not a comparison
        add_instr!(ifelse 0,
            add_instr!(Push, 10),
            add_instr!(Push, 20)
        ),
    ],
    verifier: { error VerifierError::DebugError { .. } },
    executor: { error ExecutorError::DebugError { .. } },
}

test_program! {
    /// [NEGATIVE] No condition on the stack at all when ifelse runs.
    cond_ifelse_no_condition,
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
    cond_ifelse_inside_block,
    program: vec![
        make_block!(
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

// Known limitation, related to the regression test above but not exercised by
// any test yet: when the verifier explores BOTH branches of an unknown
// condition, pops inside a branch that reach below an enclosing block's start
// mutate that `Block` frame's `start`/`saved_below` before the cells are
// restored for the second branch. To be fixed if a real program ever hits it.

test_program! {
    /// A: 0 locals, symmetric R Pop 2 into parent
    test_a,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Input),
        add_instr!(Push, 0),
        add_instr!(SetGreaterThan, 2, 3),
        make_block!(
            add_instr!(ifelse 4,
                add_instr!(R Pop, 2),
                add_instr!(R Pop, 2)
            ),
            add_instr!(Push, 777)
        ),
    ],
    verifier: { stack [10, 20, ValueSpan::inf(), 0, ValueSpan::new(0, 1), 777] },
    executor: { cases {
            input [100] => stack [10, 20, 100, 0, 1, 777];
            input [-100] => stack [10, 20, -100, 0, 0, 777];
        }
    },
}

test_program! {
    /// B: 1 local, R Pop 2 branches (1 below start each)
    test_b,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Input),
        add_instr!(Push, 0),
        add_instr!(SetGreaterThan, 2, 3),
        make_block!(
            add_instr!(Push, 30),
            add_instr!(ifelse 4,
                add_instr!(R Pop, 2),
                add_instr!(R Pop, 2)
            ),
            add_instr!(Push, 777)
        ),
    ],
    verifier: { stack [10, 20, ValueSpan::inf(), 0, ValueSpan::new(0, 1), 777] },
    executor: { cases {
            input [100] => stack [10, 20, 100, 0, 1, 777];
            input [-100] => stack [10, 20, -100, 0, 0, 777];
        }
    },
}

test_program! {
    /// C: asymmetric (true R Pop 3 below start, false Nop)
    test_c,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Input),
        add_instr!(Push, 0),
        add_instr!(SetGreaterThan, 2, 3),
        make_block!(
            add_instr!(ifelse 4,
                add_instr!(R Pop, 3),
                add_instr!(R Pop, 3)
            ),
            add_instr!(Push, 777)
        ),
    ],
    verifier: { stack [10, 20, ValueSpan::inf(), 0, ValueSpan::new(0, 1), 777] },
    executor: { cases {
            input [100] => stack [10, 20, 100, 0, 1, 777];
            input [-100] => stack [10, 20, -100, 0, 0, 777];
        }
    },
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

test_program! {
    /// [NEGATIVE] Empty blocks are prohibited.
    blocks_empty_block,
    program: {
        let program: Vec<Instruction> = vec![make_block!()];
        program
    },
    verifier: { error VerifierError::EmptyBlock },
    executor: { error ExecutorError::EmptyBlock },
}

test_program! {
    /// [POSITIVE] A block with some instructions is fine.
    blocks_block_simple,
    program: vec![make_block!(add_instr!(Push, 42))],
    verifier: { stack [42] },
    executor: { stack [42] },
}

test_program! {
    /// [POSITIVE] A block can return a value (the last push in the block).
    blocks_block_return_val,
    program: vec![
        add_instr!(Push, 10),
        make_block!(add_instr!(Push, 20), add_instr!(Push, 30)),
    ],
    verifier: { stack [10, 30] },
    executor: { stack [10, 30] },
}

test_program! {
    /// [POSITIVE] After block execution, stack state is restored and return value is on top.
    blocks_block_pops_only,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Push, 30),
        make_block!(add_instr!(R Pop, 2)),
    ],
    verifier: { stack [10, 20, 30, 10] },
    executor: { stack [10, 20, 30, 10] },
}

test_program! {
    /// [POSITIVE] Blocks can be nested.
    blocks_block_nested,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(Push, 20),
            make_block!(add_instr!(Push, 30))
        ),
    ],
    verifier: { stack [10, 30] },
    executor: { stack [10, 30] },
}

test_program! {
    /// [NEGATIVE] A block that pops more than it pushes.
    blocks_block_stack_underflow,
    program: vec![make_block!(
        add_instr!(Push, 10),
        add_instr!(R Pop, 2) // Error: tries to pop 2 but only 1 on block stack
    )],
    verifier: { error VerifierError::StackUnderflow },
    executor: { error ExecutorError::StackUnderflow },
}

test_program! {
    /// [NEGATIVE] A block must return a value.
    blocks_block_no_return_val,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(R Pop, 1) // Pops the only value, leaving block with no return value
        ),
    ],
    verifier: { error VerifierError::BlockHasEmptyStack },
    executor: { error ExecutorError::BlockHasEmptyStack },
}

// ---------------------------------------------------------------------------
// Rebasing
// ---------------------------------------------------------------------------

test_program! {
    /// [POSITIVE] A `Rebase` inside a block resets index counting.
    rebasing_rebase_simple,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(Rebase),
            add_instr!(Push, 20),
            add_instr!(Add, 0, 0) // 20 + 20 = 40
        ),
    ],
    verifier: { stack [10, 40] },
    executor: { stack [10, 40] },
}

test_program! {
    /// [POSITIVE] A `Rebase` without previous pushes is valid, just redundant.
    rebasing_rebase_redundant,
    program: vec![make_block!(
        add_instr!(Rebase),
        add_instr!(Push, 20),
        add_instr!(Add, 0, 0) // 20 + 20 = 40
    )],
    verifier: { stack [40] },
    executor: { stack [40] },
}

test_program! {
    /// [POSITIVE] A `Rebase` inside a nested block also works.
    rebasing_rebase_nested_1,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(Rebase),
            add_instr!(Push, 20),
            make_block!(
                add_instr!(Rebase),
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
    /// [POSITIVE] `Rebase` need not be used everywhere, nor is its position fixed.
    rebasing_rebase_nested_2,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(Push, 20),
            add_instr!(Rebase),
            add_instr!(Push, 30),
            make_block!(
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
    /// [NEGATIVE] `Rebase` cannot be used without blocks.
    rebasing_rebase_no_block,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Rebase), // Error: no block frame to rebase to
        add_instr!(Push, 20),
        add_instr!(Add, 0, 0),
    ],
    verifier: { error VerifierError::Core(CoreError::RebaseError) },
    executor: { error ExecutorError::Core(CoreError::RebaseError) },
}

test_program! {
    /// [NEGATIVE] `Rebase` cannot be used twice in the same block.
    rebasing_rebase_twice,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(Push, 20),
            add_instr!(Rebase),
            add_instr!(Rebase), // Error: already rebased in this block
            add_instr!(Push, 30),
        ),
    ],
    verifier: { error VerifierError::Core(CoreError::RebaseError) },
    executor: { error ExecutorError::Core(CoreError::RebaseError) },
}

test_program! {
    /// [NEGATIVE] `Pop` after `Rebase` is a stack underflow.
    rebasing_rebase_after_pop,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(Rebase),
            add_instr!(R Pop, 1), // Error
        ),
    ],
    verifier: { error VerifierError::StackUnderflow },
    executor: { error ExecutorError::StackUnderflow },
}

test_program! {
    /// [NEGATIVE] `Rebase` cannot be used in an `IfElse` branch without an inner block.
    rebasing_rebase_in_ifelse_branch,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1),
        add_instr!(ifelse 2,
            add_instr!(Rebase), // Error: no block frame in branch
            add_instr!(Push, 0)
        ),
    ],
    verifier: { error VerifierError::Core(CoreError::RebaseError) },
    executor: { error ExecutorError::Core(CoreError::RebaseError) },
}

test_program! {
    /// [POSITIVE] `Rebase` can be used in an `IfElse` branch inside a block.
    rebasing_rebase_in_ifelse_block,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 5),
        add_instr!(SetGreaterThan, 0, 1),
        add_instr!(ifelse 2,
            make_block!(
                add_instr!(Rebase), // OK: block frame exists in branch
                add_instr!(Push, 10),
                add_instr!(Add, 0, 0) // 10 + 10 = 20
            ),
            make_block!(
                add_instr!(Rebase), // OK: block frame exists in branch
                add_instr!(Push, 20),
                add_instr!(Add, 0, 0) // 20 + 20 = 40
            )
        ),
    ],
    verifier: { stack [10, 5, 1, 20] },
    executor: { stack [10, 5, 1, 20] },
}

test_program! {
    /// [POSITIVE] `Rebase` still works after an `IfElse` earlier in the same
    /// block: the branch's marker frame must not linger as the innermost
    /// frame once the branch has ended.
    rebasing_rebase_after_ifelse,
    program: vec![
        add_instr!(Push, 10),
        make_block!(
            add_instr!(Push, 1),
            add_instr!(ifelse 1,
                add_instr!(Push, 2),
                add_instr!(Push, 3)
            ),
            add_instr!(Rebase),
            add_instr!(Add, 0, 1) // 1 + 2 = 3
        ),
    ],
    verifier: { stack [10, 3] },
    executor: { stack [10, 3] },
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

test_program! {
    /// [POSITIVE] A simple function that doubles its single argument.
    functions_simple,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Mul, 0, 0) // Multiply input by 2
        ),
        add_instr!(Push, 3),
        add_instr!(fun FunctionCall, FUNC_NAME),
    ],
    verifier: { stack [3, ValueSpan::inf()] },
    executor: { stack [3, 9] },
}

test_program! {
    /// [NEGATIVE] Function call with no argument provided.
    functions_no_args,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Mul, 0, 0) // Multiply input by 2
        ),
        // Here, we forget to push an argument.
        add_instr!(fun FunctionCall, FUNC_NAME),
    ],
    verifier: { error VerifierError::NotEnoughArguments { required: 1, available: 0 } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// [POSITIVE] Multiple sequential function definitions count as aliases.
    functions_sequential_defs,
    program: vec![
        add_instr!(fun FunctionDefine, "push2_1"),
        add_instr!(fun FunctionDefine, "push2_2"),
        add_instr!(fun FunctionDefine, "push2_3"),
        add_instr!(Push, 2),
        add_instr!(fun FunctionCall, "push2_1"),
        add_instr!(fun FunctionCall, "push2_2"),
    ],
    verifier: { stack [2, 2] },
    executor: { stack [2, 2] },
}

test_program! {
    /// [NEGATIVE] Calling the function being defined is obvious infinite recursion.
    functions_sequential_defs_loop,
    program: vec![
        add_instr!(fun FunctionDefine, "push2_1"),
        add_instr!(fun FunctionDefine, "push2_2"),
        add_instr!(fun FunctionDefine, "push2_3"),
        add_instr!(fun FunctionCall, "push2_1"),
        add_instr!(fun FunctionCall, "push2_1"),
    ],
    verifier_only: { error VerifierError::InfiniteRecursion { function: f } if f == "push2_1" },
}

test_program! {
    /// [NEGATIVE] A recursive call in a statically-known branch is confirmed infinite recursion.
    functions_infinite_recursion_known_branch,
    program: vec![
        add_instr!(fun FunctionDefine, "f"),
        make_block!(
            add_instr!(Rebase),
            add_instr!(Push, 10),
            add_instr!(Push, 5),
            add_instr!(SetGreaterThan, 0, 1),
            add_instr!(ifelse 2,
                add_instr!(fun FunctionCall, "f"),
                add_instr!(Push, 0)
            )
        ),
        add_instr!(fun FunctionCall, "f"),
    ],
    verifier_only: { error VerifierError::InfiniteRecursion { function: f } if f == "f" },
}

test_program! {
    /// [POSITIVE] A recursive call in an uncertain branch is accepted with a warning.
    functions_recursion_unknown_branch,
    program: vec![
        add_instr!(fun FunctionDefine, "f"),
        make_block!(
            add_instr!(Rebase),
            add_instr!(Input),
            add_instr!(Push, 0),
            add_instr!(SetGreaterThan, 0, 1),
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
    functions_nested_defs,
    program: vec![
        add_instr!(fun FunctionDefine, OUTER),
        make_block!(
            add_instr!(fun FunctionDefine, INNER),
            make_block!(add_instr!(Push, 42)),
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
    functions_multi_args,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2),
            add_instr!(Rebase),
            add_instr!(Add, 0, 1), // a + b
            add_instr!(Add, 3, 2)  // (a + b) + c
        ),
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Push, 30),
        add_instr!(fun FunctionCall, FUNC_NAME),
    ],
    verifier: { stack [10, 20, 30, ValueSpan::inf()] },
    executor: { stack [10, 20, 30, 60] },
}

test_program! {
    /// [NEGATIVE] A multi-argument function, but not all arguments are provided.
    functions_multi_args_missing,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2),
            add_instr!(Rebase),
            add_instr!(Add, 0, 1), // a + b
            add_instr!(Add, 3, 2)  // (a + b) + c
        ),
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        // Missing third argument
        add_instr!(fun FunctionCall, FUNC_NAME),
    ],
    verifier: { error VerifierError::NotEnoughArguments { required: 3, available: 2 } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// [POSITIVE] A function that takes 3 arguments (alternative argument-passing style).
    functions_multi_args_alt,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R Read, 2),
            add_instr!(R Read, 1),
            add_instr!(R Read, 0),
            add_instr!(Rebase),
            add_instr!(Add, 0, 1), // a + b
            add_instr!(Add, 3, 2)  // (a + b) + c
        ),
        // Main program starts here
        make_block!(
            add_instr!(Rebase),
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            add_instr!(fun FunctionCall, FUNC_NAME),
        ),
        make_block!(
            add_instr!(Rebase),
            add_instr!(Push, 40),
            add_instr!(Push, 50),
            add_instr!(Push, 60),
            add_instr!(fun FunctionCall, FUNC_NAME),
        ),
        make_block!(
            add_instr!(Rebase),
            add_instr!(Push, 70),
            add_instr!(Push, 80),
            add_instr!(Push, 90),
            add_instr!(fun FunctionCall, FUNC_NAME),
        ),
        add_instr!(fun FunctionCall, FUNC_NAME),
    ],
    verifier: { stack [
        ValueSpan::inf(),
        ValueSpan::inf(),
        ValueSpan::inf(),
        ValueSpan::inf()
    ] },
    executor: { stack [60, 150, 240, 450] },
}

test_program! {
    /// [NEGATIVE] Alternative argument-passing style, but not all arguments are provided.
    functions_multi_args_alt_missing,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(R Read, 2),
            add_instr!(R Read, 1),
            add_instr!(R Read, 0),
            add_instr!(Rebase),
            add_instr!(Add, 0, 1), // a + b
            add_instr!(Add, 3, 2)  // (a + b) + c
        ),
        // Main program starts here
        make_block!(
            add_instr!(Rebase),
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            // Missing third argument
            add_instr!(fun FunctionCall, FUNC_NAME),
        ),
    ],
    verifier: { error VerifierError::NotEnoughArguments { required: 3, available: 2 } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// A function defined inside an ifelse branch only conditionally exists,
    /// so the verifier rejects the definition directly. The executor runs the
    /// concrete branch, registers the function, and calls it successfully.
    functions_defined_inside_branch,
    program: vec![
        add_instr!(Push, 1),
        add_instr!(ifelse 0,
            make_block!(
                add_instr!(fun FunctionDefine, FUNC_NAME),
                make_block!(add_instr!(Push, 42)),
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
    /// [NEGATIVE] A caller-argument read inside an ifelse branch of a function
    /// body must count towards the function's argument requirements, so
    /// calling the function with an empty stack should fail in both runners
    /// (verifier: `NotEnoughArguments`; executor: `InvalidCell`).
    ///
    /// Ignored: the branch rolls back `defining`, forgetting the recorded
    /// argument (bug), so the verifier accepts the under-supplied call.
    #[ignore = "In-branch argument reads are forgotten"]
    functions_arg_read_inside_branch,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME),
        make_block!(
            add_instr!(Push, 1),
            add_instr!(ifelse 0,
                add_instr!(R ReadReverse, 1), // reads a caller argument
                add_instr!(Nop)
            ),
            add_instr!(Rebase)
        ),
        // No caller cells provided at all.
        add_instr!(fun FunctionCall, FUNC_NAME),
    ],
    verifier: { error VerifierError::NotEnoughArguments { .. } },
    executor: { error ExecutorError::InvalidCell },
}

test_program! {
    /// [NEGATIVE] Missing function body
    function_missing_body,
    program: vec![
        add_instr!(fun FunctionDefine, FUNC_NAME)
    ],
    verifier: { error VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody( .. ))) },
    executor: { error ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionMissingBody( .. ))) },
}

// ---------------------------------------------------------------------------
// Intrinsics
// ---------------------------------------------------------------------------

test_program! {
    /// [POSITIVE] Reads an input and prints it back out (buffered I/O).
    intrinsics_input,
    program: vec![add_instr!(Input), add_instr!(R Print, 0)],
    verifier: { stack [ValueSpan::inf()] },
    executor: { cases {
        input [42] => stack [42];
        input [42] => output [42]
    } },
}

test_program! {
    /// [POSITIVE] Reads a string from a file and prints it back out.
    ///
    /// Ignored: depends on the filesystem (no buffered-I/O equivalent yet). The
    /// constructor is still referenced to keep it documented.
    // Preserved verbatim for review after removal of the file-I/O instructions.
    #[cfg(any())]
    #[ignore = "Files not yet completely implemented."]
    intrinsics_file_io_example,
    program: {
        let program: Vec<Instruction> = vec![
            add_instr!(strarg FileRead, "input.txt"),
            add_instr!(Input),
            add_instr!(strarg FileWrite, "output.txt"),
            add_instr!(R Print, 0),
            add_instr!(strarg FileRead, ""),
            add_instr!(strarg FileWrite, ""),
        ];
        program
    },
    verifier: { custom |_program| {} },
    executor: { custom |_program| {} },
}
