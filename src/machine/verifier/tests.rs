//! Unit tests for the verifier without DIFTAM
//!
//! For DIFTAM tests, see `diftam_tests.rs`.

use crate::{
    add_instr,
    instruction::{
        BinaryOp,
        Instruction::{self, *},
        UnaryOpCell, UnaryOpImm,
    },
    machine::{
        CoreError, Evaluate,
        verifier::{ValueSpan, Verifier, VerifierError},
    },
    programs::{
        INNER, OUTER,
        testable::{
            arithmetic,
            blocks::{self, rebasing},
            conditional, functions, intrinsics, stack,
        },
    },
    types::{FunctionDataError, IoBuffer},
};

use VerifierError::*;

#[derive(Debug, Clone, Copy)]
enum TestTag {
    Public,
}

macro_rules! assert_last {
    ($prog:expr) => {
        let mut verifier = Verifier::new($prog.clone());
        let result = verifier.verify();
        assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    };
}

macro_rules! assert_last_err {
    ($prog:expr, $err:pat) => {
        let mut verifier = Verifier::new($prog.clone());
        let result = verifier.verify();
        if let Err($err) = result {
            // Test passed
        } else {
            panic!(
                "Expected error {:?}, but got {:?}",
                stringify!($err),
                result
            );
        }
    };
}

macro_rules! assert_last_Int {
    ($prog:expr) => {
        assert_last!($prog);
    };
}

macro_rules! test_binop {
    ($name:ident, $op:ident) => {
        #[test]
        fn $name() {
            let program = vec![
                add_instr!(Push, 20),
                add_instr!(Push, 5),
                add_instr!($op, 0, 1),
            ];

            let mut verifier = Verifier::new(program.clone());
            assert!(verifier.verify().is_ok());

            assert_last_Int!(program);
        }
    };
}

macro_rules! create_test {
    ($name:ident) => {
        #[test]
        fn $name() {
            let program = programs::$name();
            assert_last!(program);
        }
    };
}

macro_rules! assert_err {
    ($result: ident, $expected: pat) => {
        match $result {
            $expected => (),
            x => panic!(
                "\nEXPECTED\n{:#?},\nACTUAL\n{:#?}",
                stringify!($expected),
                x
            ),
        }
    };
}

fn assert_stack(verifier: Verifier, expected: Vec<ValueSpan>) {
    assert_eq!(verifier.stack.len(), expected.len());
    for (i, &exp_val) in expected.iter().enumerate() {
        let tru_val = *verifier.stack.get(i).unwrap();
        assert_eq!(tru_val, exp_val)
    }
}

/// Check that numbers 1-5 are pushed correctly
#[test]
fn stack_push() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::push()).verify()?.into();

    let expected = ValueSpan::from_list([1, 2, 3, 4, 5]);
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn stack_pop_most() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::pop_most()).verify()?.into();

    let expected = ValueSpan::from_list([1]);
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn stack_pop_all() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::pop_all()).verify()?.into();

    let expected = ValueSpan::from_list([]);
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn stack_pop_empty() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::pop_empty()).verify();

    assert_err!(verifier, Err(VerifierError::StackUnderflow));

    Ok(())
}

#[test]
fn stack_pop_too_many() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::pop_too_many()).verify();

    assert_err!(verifier, Err(VerifierError::StackUnderflow));

    Ok(())
}

#[test]
fn stack_read() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read()).verify()?.into();

    let expected = ValueSpan::from_list([42, 42]);
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn stack_read_empty() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_empty()).verify();

    assert_err!(verifier, Err(VerifierError::InvalidCell { .. }));

    Ok(())
}

#[test]
fn stack_read_multiple() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_multiple()).verify()?.into();

    let expected = ValueSpan::from_list([10, 20, 30, 10, 20, 30]);
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
/// [NEGATIVE] Pushes a value and tries to read from index 1
fn stack_read_bad_index() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_bad_index()).verify();

    assert_err!(verifier, Err(VerifierError::InvalidCell { .. }));

    Ok(())
}

// [NEGATIVE] Read with index larger than stack size after several pushes.
#[test]
fn stack_read_far_beyond_stack() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_far_beyond_stack()).verify();

    assert_err!(verifier, Err(VerifierError::InvalidCell { .. }));

    Ok(())
}

/// [POSITIVE] Reads the top of stack
///
/// Expected final stack state: [10, 10]
#[test]
fn stack_read_reverse() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_reverse()).verify()?.into();

    let expected = ValueSpan::from_list([10, 10]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] Reads top 3 values
///
/// Expected final stack state: [10, 20, 30, 10, 20, 30]
#[test]
fn stack_read_reverse_multiple() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_reverse_multiple())
        .verify()?
        .into();

    let expected = ValueSpan::from_list([10, 20, 30, 10, 20, 30]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [NEGATIVE] Pushes a value and tries to read reverse from index 1
#[test]
fn stack_read_reverse_bad_index() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_reverse_bad_index()).verify();

    assert_err!(verifier, Err(VerifierError::InvalidCell { .. }));

    Ok(())
}

/// [NEGATIVE] Tries to read reverse from an empty stack
#[test]
fn stack_read_reverse_bad_empty() -> Result<(), VerifierError> {
    let verifier = Verifier::new(stack::read_reverse_bad_empty()).verify();

    assert_err!(verifier, Err(VerifierError::InvalidCell { .. }));

    Ok(())
}

// -------------- Arithmetic tests --------------

/// [POSITIVE] Tests bitwise not
#[test]
fn arith_bitwise_not() -> Result<(), VerifierError> {
    let verifier = Verifier::new(arithmetic::bitwise_not()).verify()?.into();

    let expected = ValueSpan::from_list([0b1100, !0b1100]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] Tests Nop
#[test]
fn arith_nop() -> Result<(), VerifierError> {
    let verifier = Verifier::new(arithmetic::nop()).verify()?;

    assert_eq!(verifier.cells.len(), 0);

    Ok(())
}

/// [NEGATIVE] Tests division by zero
#[test]
fn arith_div_by_zero() -> Result<(), VerifierError> {
    let verifier = Verifier::new(arithmetic::div_by_zero()).verify();

    assert_err!(verifier, Err(VerifierError::DivisionByZero));

    Ok(())
}

/// [POSITIVE] Tests a simple ifelse with a statically known true condition
///
/// Expected final stack state: [10, 5, 1, 42]
#[test]
fn cond_ifelse_known_true() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_known_true())
        .verify()?
        .into();

    let expected = ValueSpan::from_list([10, 5, 1, 42]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] Tests a simple ifelse with a statically known false condition
///
/// Expected final stack state: [3, 5, 0, 0]
#[test]
fn cond_ifelse_known_false() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_known_false())
        .verify()?
        .into();

    let expected = ValueSpan::from_list([3, 5, 0, 0]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] Tests an ifelse with an unknown condition but balanced branches
///
/// Expected final stack state if input > 5: [100, 5, 1, 42]
/// Expected final stack state if input <= 5: [-100, 5, 0, 0]
#[test]
fn cond_ifelse_unknown_balanced() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_unknown_balanced())
        .verify()?
        .into();

    let expected = vec![
        ValueSpan::inf(),
        5.into(),
        ValueSpan::new(0, 1),
        ValueSpan::new(0, 42),
    ];

    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] Statically-known condition: only the taken branch runs, and the
/// verifier does not need to compare branch sizes (asymmetric branches are
/// fine here because the untaken branch is dead code).
///
/// Expected final stack state: [10, 3, 1, 42]
#[test]
fn cond_ifelse_known_true_asymmetric() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_known_true_asymmetric())
        .verify()?
        .into();

    let expected = ValueSpan::from_list([10, 3, 1, 42]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] Statically-known false condition: only the false branch runs.
///
/// Expected final stack state: [0]
#[test]
fn cond_ifelse_known_false_asymmetric() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_known_false_asymmetric())
        .verify()?
        .into();

    let expected = ValueSpan::from_list([0]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [NEGATIVE] Tests an ifelse with an unknown condition and unbalanced branches
///
/// Expected: No way of detecting this with the verifier.
///
/// Expected final stack state if input > 5: [x, 5, 1, 42]
/// Expected final stack state if input <= 5: [x, 5]
#[test]
fn cond_ifelse_unknown_unbalanced() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_unknown_unbalanced()).verify();

    assert_err!(
        verifier,
        Err(VerifierError::CondUnequalStackSizes {
            true_branch_cells: 4,
            false_branch_cells: 2
        })
    );

    Ok(())
}

/// [NEGATIVE] Condition is not the result of a comparison instruction.
///
/// This program will eventually be used to check the type system.
///
/// Expected: `TypeError`
#[test]
#[ignore = "Type system currently not yet implemented."]
fn cond_ifelse_bad_placement() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_bad_placement()).verify();

    assert_err!(verifier, Err(VerifierError::TypeError { .. })); // TODO: Implement

    Ok(())
}

/// [NEGATIVE] No condition on the stack at all when ifelse runs.
///
/// Expected: `InvalidCell`.
#[test]
fn cond_ifelse_no_condition() -> Result<(), VerifierError> {
    let verifier = Verifier::new(conditional::ifelse_no_condition()).verify();

    assert_err!(verifier, Err(VerifierError::InvalidCell { .. }));

    Ok(())
}

/// [POSITIVE] A block with some instructions is fine.
///
/// Expected final stack state: [42]
#[test]
fn blocks_block_simple() -> Result<(), VerifierError> {
    let verifier = Verifier::new(blocks::block_simple()).verify()?.into();

    let expected = ValueSpan::from_list([42]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [NEGATIVE] Empty blocks are prohibited.
// TODO: Implement empty block prohibition
// pub fn empty_block() -> Snippet

/// [POSITIVE] A block can return a value, which is the last push in the block.
///
/// Expected final stack state: [10, 30]
#[test]
fn blocks_block_return_val() -> Result<(), VerifierError> {
    let verifier = Verifier::new(blocks::block_return_val()).verify()?.into();

    let expected = ValueSpan::from_list([10, 30]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] After block execution, stack state is restored and return value is on top.
///
/// Expected final stack state: [10, 20, 30, 10]
#[test]
fn blocks_block_pops_only() -> Result<(), VerifierError> {
    let verifier = Verifier::new(blocks::block_pops_only()).verify()?.into();

    let expected = ValueSpan::from_list([10, 20, 30, 10]);
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn blocks_block_nested() -> Result<(), VerifierError> {
    let verifier = Verifier::new(blocks::block_nested()).verify()?.into();

    let expected = ValueSpan::from_list([10, 30]);
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn blocks_block_stack_underflow() -> Result<(), VerifierError> {
    let verifier = Verifier::new(blocks::block_stack_underflow()).verify();

    assert_err!(verifier, Err(VerifierError::StackUnderflow));

    Ok(())
}

#[test]
fn blocks_block_no_return_val() -> Result<(), VerifierError> {
    let verifier = Verifier::new(blocks::block_no_return_val()).verify();

    assert_err!(verifier, Err(VerifierError::BlockHasEmptyStack));

    Ok(())
}

/// [POSITIVE] A `Rebase` inside of a block resters index counting.
///
/// Expected final stack state: [10, 40]
#[test]
fn rebasing_rebase_simple() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_simple()).verify()?.into();

    let expected = ValueSpan::from_list([10, 40]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] A `Rebase` without previous pushes is still valid, just redundant.
///
/// Expected final stack state: [40]
#[test]
fn rebasing_rebase_redundant() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_redundant()).verify()?.into();

    let expected = ValueSpan::from_list([40]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] A `Rebase` inside a nested block also works as expected.
///
/// Expected final stack state: [10, 80]
#[test]
fn rebasing_rebase_nested_1() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_nested_1()).verify()?.into();

    let expected = ValueSpan::from_list([10, 80]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] A `Rebase` is not necessarily used everywhere, neither is its position fixed
///
/// Expected final stack state: [10, 90]
#[test]
fn rebasing_rebase_nested_2() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_nested_2()).verify()?.into();

    let expected = ValueSpan::from_list([10, 90]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [NEGATIVE] `Rebase` cannot be used without blocks
#[test]
fn rebasing_rebase_no_block() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_no_block()).verify();

    assert_err!(verifier, Err(VerifierError::Core(CoreError::RebaseError)));

    Ok(())
}

/// [NEGATIVE] `Rebase` cannot be used twice in the same block
#[test]
fn rebasing_rebase_twice() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_twice()).verify();

    assert_err!(verifier, Err(VerifierError::Core(CoreError::RebaseError)));

    Ok(())
}

#[test]
fn rebasing_rebase_after_pop() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_after_pop()).verify();

    assert_err!(verifier, Err(VerifierError::StackUnderflow));

    Ok(())
}

/// [NEGATIVE] `Rebase` cannot be used in an `IfElse` branch without an inner block.
///
/// Expected: `RebaseError`
#[test]
fn rebasing_rebase_in_ifelse_branch() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_in_ifelse_branch()).verify();

    assert_err!(verifier, Err(VerifierError::Core(CoreError::RebaseError)));

    Ok(())
}

/// [POSITIVE] `Rebase` can be used in an `IfElse` branch, as long as it's inside a block.
///
/// Expected final stack state: [10, 5, 1, 20]
#[test]
fn rebasing_rebase_in_ifelse_block() -> Result<(), VerifierError> {
    let verifier = Verifier::new(rebasing::rebase_in_ifelse_block())
        .verify()?
        .into();

    let expected = ValueSpan::from_list([10, 5, 1, 20]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [POSITIVE] A simple function that takes one argument, doubles it, and returns the result.
#[test]
fn functions_simple() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::simple()).verify()?.into();

    let expected = vec![3.into(), ValueSpan::inf()];
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn functions_no_args() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::no_args()).verify();

    assert_err!(
        verifier,
        Err(VerifierError::NotEnoughArguments {
            required: 1,
            available: 0
        })
    );

    Ok(())
}

/// [POSITIVE] Multiple sequential function definitions count as aliases
///
/// Expected final stack state: [2, 2]
#[test]
fn functions_sequential_defs() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::sequential_defs()).verify()?.into();

    let expected = ValueSpan::from_list([2, 2]);
    assert_stack(verifier, expected);

    Ok(())
}

/// [NEGATIVE] Calling the function that's being defined is obvious infinite recursion.
#[ignore = "Solve obvious recursion"]
#[test]
fn functions_sequential_defs_loop() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::sequential_defs_loop()).verify();

    assert_err!(
        verifier,
        Err(VerifierError::Core(CoreError::FunctionDataError(
            FunctionDataError::FunctionRedefinition(_)
        )))
    );

    Ok(())
}

/// [NEGATIVE] Nested function definitions are prohibited
#[test]
fn functions_nested_defs() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::nested_defs()).verify();

    match verifier {
        Err(VerifierError::NestedFunctionDefinition {
            outer_function: outer,
            inner_function: inner,
        }) if outer == OUTER && inner == INNER => (),
        x => panic!(
            "\nEXPECTED\n{:#?},\nACTUAL\n{:#?}",
            Err::<Verifier, VerifierError>(VerifierError::NestedFunctionDefinition {
                outer_function: OUTER.to_string(),
                inner_function: INNER.to_string(),
            }),
            x
        ),
    };

    Ok(())
}

/// [POSITIVE] A function that takes 3 arguments and adds them together.
///
/// This is the standard way of providing arguments to a function and will easily work for any
/// length of arguments, as well as as many repetitions of function calls in the main program
/// body as desired.
///
/// Expected final stack state: [10, 20, 30, 60]
#[test]
fn functions_multi_args() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::multi_args()).verify()?.into();

    let mut expected = ValueSpan::from_list([10, 20, 30]);
    expected.push(ValueSpan::inf());
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn functions_multi_args_missing() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::multi_args_missing()).verify();

    assert_err!(
        verifier,
        Err(VerifierError::NotEnoughArguments {
            required: 3,
            available: 2
        })
    );

    Ok(())
}

/// [POSITIVE] A function that takes 3 arguments and adds them together.
///
/// Expected final stack state: [inf, inf, inf, inf]
#[test]
fn functions_multi_args_alt() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::multi_args_alt()).verify()?.into();

    let expected = vec![ValueSpan::inf(); 4];
    assert_stack(verifier, expected);

    Ok(())
}

#[test]
fn functions_multi_args_alt_missing() -> Result<(), VerifierError> {
    let verifier = Verifier::new(functions::multi_args_alt_missing()).verify();

    assert_err!(
        verifier,
        Err(VerifierError::NotEnoughArguments {
            required: 3,
            available: 2
        })
    );

    Ok(())
}

// TODO: implement
// test_binop!(add, Add);
// test_binop!(add_neg, Add);
// test_binop!(mul, Mul);
// test_binop!(div, Div);
// test_binop!(and, And);
// test_binop!(or, Or);
// test_binop!(xor, Xor);
// test_binop!(slt, SetLessThan);
// test_binop!(sgt, SetGreaterThan);
// test_binop!(seq, SetEqual);
// test_binop!(sne, SetNotEqual);
// test_binop!(sle, SetLessThanOrEqual);
// test_binop!(sge, SetGreaterThanOrEqual);
// test_binop!(sll, ShiftLeftLogical);
// test_binop!(srl, ShiftRightLogical);
// test_binop!(sra, ShiftRightArithmetic);
