use super::*;

use crate::{
    add_instr,
    information_flow::{FlowError, SecurityPolicy},
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{
            self, AluBinary, AluFunction, AluIntrinsic, AluNullary, AluUnaryCell, AluUnaryImm,
        },
        NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{CoreError, executor::Executor},
    make_block,
    programs::testable::{
        arithmetic,
        blocks::{self, rebasing},
        conditional, functions, intrinsics, stack,
    },
    types::{FunctionDataError, IoBuffer},
};
use std::{cell::RefCell, fmt::Debug, rc::Rc};

// macro_rules! assert_eq_last {
//     ($prog:expr, $value:expr) => {
//         let mut topval = Executor::new($prog.clone()).exec()?.top;
//         assert_eq!(val, $value);
//     };
// }
//
// macro_rules! assert_eq_last_Int {
//     ($prog:expr, $value:expr) => {
//         assert_eq_last!($prog, Some(&Cell::Integer($value)));
//     };
// }
//
// macro_rules! test_binop {
//     ($name:ident, $a:expr, $b:expr, $op:ident => $expected:expr) => {
//         #[test]
//         fn $name() {
//             let program = vec![
//                 add_instr!(Push, $a),
//                 add_instr!(Push, $b),
//                 add_instr!($op, 0, 1),
//             ];
//             assert_eq_last_Int!(program, $expected);
//         }
//     };
// }

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

fn assert_stack(executor: Executor, expected: Vec<i64>) {
    assert_eq!(executor.cells.len(), expected.len());
    for (i, &value) in expected.iter().enumerate() {
        assert_eq!(executor.cells[i], Cell::Integer(value));
    }
}

// fn check_pos(
//     program: impl FnOnce() -> crate::programs::Snippet,
//     expected: Vec<i64>,
// ) -> Result<(), ExecutorError> {
//     let executor: Executor = Executor::new(program()).exec()?.into();
//     assert_stack(executor, expected);
//     Ok(())
// }
//
// fn check_neg(
//     program: impl FnOnce() -> crate::programs::Snippet,
//     expected: impl std::error::Error,
// ) -> Result<(), ExecutorError> {
//     let executor: Executor = Executor::new(program()).exec()?.into();
//     assert_err!(executor, expected);
//     Ok(())
// }

/// Check that numbers 1-5 are pushed correctly
#[test]
fn stack_push() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::push()).exec()?.into();
    let expected = vec![1, 2, 3, 4, 5];

    assert_stack(executor, expected);

    Ok(())
}

#[test]
fn stack_pop_most() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::pop_most()).exec()?.into();

    let expected = vec![1];
    assert_stack(executor, expected);

    Ok(())
}

#[test]
fn stack_pop_all() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::pop_all()).exec()?.into();

    let expected = vec![];
    assert_stack(executor, expected);

    Ok(())
}

#[test]
fn stack_pop_empty() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::pop_empty()).exec();

    assert_err!(executor, Err(ExecutorError::StackUnderflow));

    Ok(())
}

#[test]
fn stack_pop_too_many() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::pop_too_many()).exec();

    assert_err!(executor, Err(ExecutorError::StackUnderflow));

    Ok(())
}

#[test]
fn stack_read() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read()).exec()?.into();

    let expected = vec![42, 42];
    assert_stack(executor, expected);

    Ok(())
}

#[test]
fn stack_read_empty() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_empty()).exec();

    assert_err!(executor, Err(ExecutorError::InvalidCell));

    Ok(())
}

#[test]
fn stack_read_multiple() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_multiple()).exec()?.into();

    let expected = vec![10, 20, 30, 10, 20, 30];
    assert_stack(executor, expected);

    Ok(())
}

#[test]
/// [NEGATIVE] Pushes a value and tries to read from index 1
fn stack_read_bad_index() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_bad_index()).exec();

    assert_err!(executor, Err(ExecutorError::InvalidCell));

    Ok(())
}

// [NEGATIVE] Read with index larger than stack size after several pushes.
#[test]
fn stack_read_far_beyond_stack() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_far_beyond_stack()).exec();

    assert_err!(executor, Err(ExecutorError::InvalidCell));

    Ok(())
}

/// [POSITIVE] Reads the top of stack
///
/// Expected final stack state: [10, 10]
#[test]
fn stack_read_reverse() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_reverse()).exec()?.into();

    let expected = vec![10, 10];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] Reads top 3 values
///
/// Expected final stack state: [10, 20, 30, 10, 20, 30]
#[test]
fn stack_read_reverse_multiple() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_reverse_multiple()).exec()?.into();

    let expected = vec![10, 20, 30, 10, 20, 30];
    assert_stack(executor, expected);

    Ok(())
}

/// [NEGATIVE] Pushes a value and tries to read reverse from index 1
#[test]
fn stack_read_reverse_bad_index() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_reverse_bad_index()).exec();

    assert_err!(executor, Err(ExecutorError::InvalidCell));

    Ok(())
}

/// [NEGATIVE] Tries to read reverse from an empty stack
#[test]
fn stack_read_reverse_bad_empty() -> Result<(), ExecutorError> {
    let executor = Executor::new(stack::read_reverse_bad_empty()).exec();

    assert_err!(executor, Err(ExecutorError::InvalidCell));

    Ok(())
}

// -------------- Arithmetic tests --------------

/// [POSITIVE] Tests bitwise not
#[test]
fn arith_bitwise_not() -> Result<(), ExecutorError> {
    let executor = Executor::new(arithmetic::bitwise_not()).exec()?.into();

    let expected = vec![0b1100, !0b1100];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] Tests Nop
#[test]
fn arith_nop() -> Result<(), ExecutorError> {
    let executor = Executor::new(arithmetic::nop()).exec()?;

    assert_eq!(executor.cells.len(), 0);

    Ok(())
}

/// [NEGATIVE] Tests division by zero
#[test]
fn arith_div_by_zero() -> Result<(), ExecutorError> {
    let executor = Executor::new(arithmetic::div_by_zero()).exec();

    assert_err!(executor, Err(ExecutorError::DivisionByZero));

    Ok(())
}

/// [POSITIVE] Tests a simple ifelse with a statically known true condition
///
/// Expected final stack state: [10, 5, 1, 42]
#[test]
fn cond_ifelse_known_true() -> Result<(), ExecutorError> {
    let executor = Executor::new(conditional::ifelse_known_true())
        .exec()?
        .into();

    let expected = vec![10, 5, 1, 42];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] Tests a simple ifelse with a statically known false condition
///
/// Expected final stack state: [3, 5, 0, 0]
#[test]
fn cond_ifelse_known_false() -> Result<(), ExecutorError> {
    let executor = Executor::new(conditional::ifelse_known_false())
        .exec()?
        .into();

    let expected = vec![3, 5, 0, 0];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] Tests an ifelse with an unknown condition but balanced branches
///
/// Expected final stack state if input > 5: [100, 5, 1, 42]
/// Expected final stack state if input <= 5: [-100, 5, 0, 0]
#[test]
fn cond_ifelse_unknown_balanced() -> Result<(), ExecutorError> {
    // First test
    let input = IoBuffer::new(vec![100]);

    let executor = Executor::new(conditional::ifelse_unknown_balanced())
        .redirect_input(input.into())
        .exec()?
        .into();

    let expected = vec![100, 5, 1, 42];
    assert_stack(executor, expected);

    // Second test
    let input = IoBuffer::new(vec![-100]);

    let executor = Executor::new(conditional::ifelse_unknown_balanced())
        .redirect_input(input.into())
        .exec()?
        .into();

    let expected = vec![-100, 5, 0, 0];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] Statically-known condition: only the taken branch runs, and the
/// executor does not need to compare branch sizes (asymmetric branches are
/// fine here because the untaken branch is dead code).
///
/// Expected final stack state: [10, 3, 1, 42]
#[test]
fn cond_ifelse_known_true_asymmetric() -> Result<(), ExecutorError> {
    let executor = Executor::new(conditional::ifelse_known_true_asymmetric())
        .exec()?
        .into();

    let expected = vec![10, 3, 1, 42];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] Statically-known false condition: only the false branch runs.
///
/// Expected final stack state: [0]
#[test]
fn cond_ifelse_known_false_asymmetric() -> Result<(), ExecutorError> {
    let executor = Executor::new(conditional::ifelse_known_false_asymmetric())
        .exec()?
        .into();

    let expected = vec![0];
    assert_stack(executor, expected);

    Ok(())
}

/// [NEGATIVE] Tests an ifelse with an unknown condition and unbalanced branches
///
/// Expected: No way of detecting this with the executor.
///
/// Expected final stack state if input > 5: [x, 5, 1, 42]
/// Expected final stack state if input <= 5: [x, 5]
#[test]
fn cond_ifelse_unknown_unbalanced() -> Result<(), ExecutorError> {
    // First test
    let input = IoBuffer::new(vec![100]);

    let executor = Executor::new(conditional::ifelse_unknown_unbalanced())
        .redirect_input(input.into())
        .exec()?
        .into();

    let expected = vec![100, 5, 1, 42];
    assert_stack(executor, expected);

    // Second test
    let input = IoBuffer::new(vec![-100]);

    let executor = Executor::new(conditional::ifelse_unknown_unbalanced())
        .redirect_input(input.into())
        .exec()?
        .into();

    let expected = vec![-100, 5];
    assert_stack(executor, expected);

    Ok(())
}

/// [NEGATIVE] Condition is not the result of a comparison instruction.
///
/// This program will eventually be used to check the type system.
///
/// Expected: `TypeError`
#[test]
#[ignore = "Type system currently not yet implemented."]
fn cond_ifelse_bad_placement() -> Result<(), ExecutorError> {
    let executor = Executor::new(conditional::ifelse_bad_placement()).exec();

    assert_err!(executor, Err(ExecutorError::TypeError { .. })); // TODO: Implement

    Ok(())
}

/// [NEGATIVE] No condition on the stack at all when ifelse runs.
///
/// Expected: `InvalidCell`.
#[test]
fn cond_ifelse_no_condition() -> Result<(), ExecutorError> {
    let executor = Executor::new(conditional::ifelse_no_condition()).exec();

    assert_err!(executor, Err(ExecutorError::InvalidCell));

    Ok(())
}

use paste::paste;

macro_rules! make_executor_test_pos {
    (
        $(#[$meta:meta])*
        $category:ident,
        $module:ident::$test:ident,
        $expected:expr
    ) => {
        paste! {
            $(#[$meta])*
            #[test]
            fn [<$category _ $test>]() -> Result<(), ExecutorError> {
                let executor = Executor::new($module::$test()).exec()?.into();

                assert_stack(executor, $expected);

                Ok(())
            }
        }
    };
    (
        $(#[$meta:meta])*
        $module:ident::$test:ident,
        $expected:expr
    ) => {
        make_executor_test_pos!(
            $(#[$meta])*
            $module,
            $module::$test,
            $expected
        );
    };
}

macro_rules! make_executor_test_neg {
    (
        $(#[$meta:meta])*
        $category:ident,
        $module:ident::$test:ident,
        $expected_err:pat
    ) => {
        paste! {
            $(#[$meta])*
            #[test]
            fn [<$category _ $test>]() -> Result<(), ExecutorError> {
                let executor = Executor::new($module::$test()).exec();

                assert_err!(executor, Err($expected_err));

                Ok(())
            }
        }
    };
    (
        $(#[$meta:meta])*
        $module:ident::$test:ident,
        $expected_err:pat
    ) => {
        make_executor_test_neg!(
            $(#[$meta])*
            $module,
            $module::$test,
            $expected_err
        );
    };
}

make_executor_test_pos!(
    /// [POSITIVE] A block with some instructions is fine.
    ///
    /// Expected final stack state: [42]
    blocks, blocks::block_simple, vec![42]);

/// [NEGATIVE] Empty blocks are prohibited.
// TODO: Implement empty block prohibition
// pub fn empty_block() -> Snippet

/// [POSITIVE] A block can return a value, which is the last push in the block.
///
/// Expected final stack state: [10, 30]
#[test]
fn blocks_block_return_val() -> Result<(), ExecutorError> {
    let executor = Executor::new(blocks::block_return_val()).exec()?.into();

    let expected = vec![10, 30];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] After block execution, stack state is restored and return value is on top.
///
/// Expected final stack state: [10, 20, 30, 10]
#[test]
fn blocks_block_pops_only() -> Result<(), ExecutorError> {
    let executor = Executor::new(blocks::block_pops_only()).exec()?.into();

    let expected = vec![10, 20, 30, 10];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] Blocks can be nested
///
/// Expected final stack state: [10, 30]
#[test]
fn blocks_block_nested() -> Result<(), ExecutorError> {
    let executor = Executor::new(blocks::block_nested()).exec()?.into();

    let expected = vec![10, 30];
    assert_stack(executor, expected);

    Ok(())
}

make_executor_test_neg!(
    /// [NEGATIVE] A block that pops more than it pushes should cause an error.
    blocks::block_stack_underflow, ExecutorError::StackUnderflow
);

make_executor_test_neg!(
    /// [NEGATIVE] A block must return a value
    blocks::block_no_return_val, ExecutorError::BlockHasEmptyStack
);

/// [POSITIVE] A `Rebase` inside of a block resters index counting.
///
/// Expected final stack state: [10, 40]
#[test]
fn rebasing_rebase_simple() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_simple()).exec()?.into();

    let expected = vec![10, 40];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] A `Rebase` without previous pushes is still valid, just redundant.
///
/// Expected final stack state: [40]
#[test]
fn rebasing_rebase_redundant() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_redundant()).exec()?.into();

    let expected = vec![40];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] A `Rebase` inside a nested block also works as expected.
///
/// Expected final stack state: [10, 80]
#[test]
fn rebasing_rebase_nested_1() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_nested_1()).exec()?.into();

    let expected = vec![10, 80];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] A `Rebase` is not necessarily used everywhere, neither is its position fixed
///
/// Expected final stack state: [10, 90]
#[test]
fn rebasing_rebase_nested_2() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_nested_2()).exec()?.into();

    let expected = vec![10, 90];
    assert_stack(executor, expected);

    Ok(())
}

/// [NEGATIVE] `Rebase` cannot be used without blocks
#[test]
fn rebasing_rebase_no_block() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_no_block()).exec();

    assert_err!(executor, Err(ExecutorError::Core(CoreError::RebaseError)));

    Ok(())
}

/// [NEGATIVE] `Rebase` cannot be used twice in the same block
#[test]
fn rebasing_rebase_twice() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_twice()).exec();

    assert_err!(executor, Err(ExecutorError::Core(CoreError::RebaseError)));

    Ok(())
}

/// [NEGATIVE] `Pop` after `Rebase` is a stack underflow.
#[test]
fn rebasing_rebase_after_pop() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_after_pop()).exec();

    assert_err!(executor, Err(ExecutorError::StackUnderflow));

    Ok(())
}

/// [NEGATIVE] `Rebase` cannot be used in an `IfElse` branch without an inner block.
///
/// Expected: `RebaseError`
#[test]
fn rebasing_rebase_in_ifelse_branch() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_in_ifelse_branch()).exec();

    assert_err!(executor, Err(ExecutorError::Core(CoreError::RebaseError)));

    Ok(())
}

/// [POSITIVE] `Rebase` can be used in an `IfElse` branch, as long as it's inside a block.
///
/// Expected final stack state: [10, 5, 1, 20]
#[test]
fn rebasing_rebase_in_ifelse_block() -> Result<(), ExecutorError> {
    let executor = Executor::new(rebasing::rebase_in_ifelse_block())
        .exec()?
        .into();

    let expected = vec![10, 5, 1, 20];
    assert_stack(executor, expected);

    Ok(())
}

/// [POSITIVE] A simple function that takes one argument, doubles it, and returns the result.
#[test]
fn functions_simple() -> Result<(), ExecutorError> {
    let executor = Executor::new(functions::simple()).exec()?.into();

    let expected = vec![3, 9];
    assert_stack(executor, expected);

    Ok(())
}

make_executor_test_neg!(
    /// [NEGATIVE] Function call with no argument provided.
    functions::no_args, ExecutorError::InvalidCell
);

/// [POSITIVE] Multiple sequential function definitions count as aliases
#[test]
fn functions_sequential_defs() -> Result<(), ExecutorError> {
    let executor = Executor::new(functions::sequential_defs()).exec()?.into();

    let expected = vec![2, 2];
    assert_stack(executor, expected);

    Ok(())
}

/// [NEGATIVE] Calling the function that's being defined is obvious infinite recursion.
#[ignore = "Solve obvious recursion"]
#[test]
fn functions_sequential_defs_loop() -> Result<(), ExecutorError> {
    let executor = Executor::new(functions::sequential_defs_loop()).exec();

    assert_err!(
        executor,
        Err(ExecutorError::Core(CoreError::FunctionDataError(
            FunctionDataError::FunctionRedefinition(_)
        )))
    );

    Ok(())
}

/// [NEGATIVE] Nested function definitions are prohibited
///
/// TODO: Executor behavior for this program is still undecided
#[ignore = "Decide on executor behavior for this case"]
#[test]
fn functions_nested_defs() -> Result<(), ExecutorError> {
    let executor = Executor::new(functions::nested_defs()).exec();

    assert_err!(
        executor,
        Err(ExecutorError::Core(CoreError::FunctionDataError(_)))
    );

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
fn functions_multi_args() -> Result<(), ExecutorError> {
    let executor = Executor::new(functions::multi_args()).exec()?.into();

    let expected = vec![10, 20, 30, 60];
    assert_stack(executor, expected);

    Ok(())
}

make_executor_test_neg!(
    /// [NEGATIVE] A function with multiple arguments, but not all are provieded.
    functions::multi_args_missing, ExecutorError::InvalidCell
);

/// [POSITIVE] A function that takes 3 arguments and adds them together.
///
/// This is an alternative way of providing arguments to a function and is more verbose and
/// less elegant to use, but the semantics allow for it. For functions like these, arguments do
/// not need to be padded with extra pushes, as is usually the case with recursive functions,
/// for which it is important where the arguments sit.
///
/// Refer to [`multi_args`] for standard argument-passing style.
///
/// Expected final stack state: [60, 150, 240, 450]
#[test]
fn functions_multi_args_alt() -> Result<(), ExecutorError> {
    let executor = Executor::new(functions::multi_args_alt()).exec()?.into();

    let expected = vec![60, 150, 240, 450];
    assert_stack(executor, expected);

    Ok(())
}

make_executor_test_neg!(
    /// [NEGATIVE] A function with alternative argument passing style and multiple arguments, but
    /// not all are provieded.
    functions::multi_args_alt_missing, ExecutorError::InvalidCell
);

// test_binop!(add, 10, 20, Add => 30);
// test_binop!(add_neg, 10, -30, Add => -20);
// test_binop!(mul, 10, 20, Mul => 200);
// test_binop!(div, 20, 5, Div => 4);
// test_binop!(and, 0b1100, 0b1010, And => 0b1000);
// test_binop!(or, 0b1100, 0b1010, Or => 0b1110);
// test_binop!(xor, 0b1100, 0b1010, Xor => 0b0110);
// test_binop!(slt, 10, 20, SetLessThan => 1);
// test_binop!(sgt, 20, 10, SetGreaterThan => 1);
// test_binop!(seq, 10, 10, SetEqual => 1);
// test_binop!(sne, 10, 20, SetNotEqual => 1);
// test_binop!(sle, 10, 10, SetLessThanOrEqual => 1);
// test_binop!(sge, 20, 10, SetGreaterThanOrEqual => 1);
// test_binop!(sll, 0b0001, 2, ShiftLeftLogical => 0b0100);
// test_binop!(srl, 0b0100, 2, ShiftRightLogical => 0b0001);
// test_binop!(sra, -8, 2, ShiftRightArithmetic => -2);
//
// // TODO: Solve what to do here
// #[test]
// fn nested_functions() {
//     let mut program = vec![
//         add_instr!(fun FunctionDefine, String::from("outer")),
//         make_block!(
//             add_instr!(fun FunctionDefine, String::from("inner")),
//             make_block!(add_instr!(Push, 42)),
//             add_instr!(fun FunctionCall, String::from("inner"))
//         ),
//         add_instr!(fun FunctionCall, String::from("outer")),
//     ];
//
//     // Outerlook nearly identical but generate completely different test types. That's hard to read. function call should work
//     assert_eq_last_Int!(program, 42);
//
//     // Inner function call should still work; this is only prohibited by the verifier.
//     program.push(add_instr!(fun FunctionCall, String::from("inner")));
//     assert_eq_last_Int!(program, 42);
// }
//
// #[test]
// fn print() {
//     let program = vec![
//         add_instr!(Push, 123),
//         add_instr!(io Print, 0), // Should print 123
//     ];
//
//     let mut machine = Executor::new(program);
//     let last = machine.exec().unwrap();
//     assert_eq!(last, Some(&Cell::Integer(123)));
// }
//
// use std::sync::Once;
// static INIT: Once = Once::new();
//
// pub fn init() {
//     INIT.call_once(|| {
//         env_logger::builder()
//             .is_test(true)
//             .format_timestamp(None)
//             .format_target(false)
//             .format_file(true)
//             .format_line_number(true)
//             .format_module_path(false)
//             .init();
//     });
// }
//
// mod diftam {
//     use super::*;
//     use crate::information_flow::Topology;
//
//     // Policy definitions
//
//     #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
//     enum Confidentiality {
//         Public,
//         Confidential,
//         Secret,
//     }
//
//     #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
//     enum Integrity {
//         Low,
//         Medium,
//         High,
//     }
//
//     fn confidentiality_policy() -> SecurityPolicy<Confidentiality> {
//         use Confidentiality::*;
//
//         let graph = Topology::linear([Public, Confidential, Secret])
//             .into_graph()
//             .unwrap();
//         SecurityPolicy::new(graph, Public, Secret, Public).unwrap()
//     }
//
//     fn integrity_policy() -> SecurityPolicy<Integrity> {
//         use Integrity::*;
//
//         let graph = Topology::linear([Low, Medium, High]).into_graph().unwrap();
//         SecurityPolicy::new(graph, Low, Low, High).unwrap()
//     }
//
//     // Tests
//
//     #[test]
//     fn confidentiality_ift() {
//         use Confidentiality::*;
//
//         let program = vec![
//             add_instr!(Push, 10),
//             add_instr!(tag Push, 20, Secret),
//             add_instr!(Add, 0, 1),
//         ];
//         let mut machine = Executor::with_policy(program, confidentiality_policy()).unwrap();
//         let last = machine.exec().unwrap();
//
//         assert_eq!(last, Some(&Cell::Integer(30)));
//         assert_eq!(machine.read_tag(0).unwrap(), Public);
//         assert_eq!(machine.read_tag(1).unwrap(), Secret);
//         assert_eq!(machine.last_tag(), Some(Secret));
//     }
//
//     #[test]
//     fn integrity_ift() {
//         use Integrity::*;
//
//         let program = vec![
//             add_instr!(tag Push, 10, Low),
//             add_instr!(tag Push, 20, High),
//             add_instr!(Add, 0, 1),
//         ];
//         let mut machine = Executor::with_policy(program, integrity_policy()).unwrap();
//         machine.exec().unwrap();
//
//         assert_eq!(machine.last_tag(), Some(High));
//     }
//
//     #[test]
//     fn pg_input() {
//         use Confidentiality::*;
//
//         let input = Rc::new(RefCell::new(vec![42]));
//         let program = vec![add_instr!(io Input, 0)];
//         let mut machine = Executor::with_policy(program, confidentiality_policy()).unwrap();
//         machine.redirect_input(types::Input::Buffer(input));
//         let last = machine.exec().unwrap();
//
//         assert_eq!(last, Some(&Cell::Integer(42)));
//         assert_eq!(machine.last_tag(), Some(Secret));
//     }
//
//     #[test]
//     fn ifelse_condition_taints_values() {
//         use Confidentiality::*;
//
//         let program = vec![
//             add_instr!(tag Push, 1, Secret),
//             add_instr!(ifelse 0,
//                 add_instr!(tag Push, 7, Public),
//                 add_instr!(tag Push, 9, Public)
//             ),
//             add_instr!(tag Push, 11, Public),
//         ];
//         let mut machine = Executor::with_policy(program, confidentiality_policy()).unwrap();
//         machine.exec().unwrap();
//
//         assert_eq!(
//             machine.cells,
//             vec![Cell::Integer(1), Cell::Integer(7), Cell::Integer(11)]
//         );
//         assert_eq!(machine.read_tag(1).unwrap(), Secret);
//         assert_eq!(machine.read_tag(2).unwrap(), Public);
//     }
//
//     #[test]
//     fn tags_remain_aligned_through_function_rebase_and_pop() {
//         use Confidentiality::*;
//
//         let program = vec![
//             add_instr!(fun FunctionDefine, "add_public"),
//             make_block!(
//                 add_instr!(R ReadReverse, 0),
//                 add_instr!(Rebase),
//                 add_instr!(tag Push, 1, Public),
//                 add_instr!(Add, 0, 1)
//             ),
//             add_instr!(tag Push, 41, Secret),
//             add_instr!(fun FunctionCall, "add_public"),
//             add_instr!(Push, 99),
//             add_instr!(R Pop, 1),
//         ];
//         let mut machine = Executor::with_policy(program, confidentiality_policy()).unwrap();
//         machine.exec().unwrap();
//
//         assert_eq!(machine.cells, vec![Cell::Integer(41), Cell::Integer(42)]);
//         assert_eq!(machine.tags(), &[Secret, Secret]);
//     }
//
//     #[test]
//     /// This test ensures that the output perimeter guard correctly rejects attempt to print a
//     /// secret value.
//     fn output_pg_rejects_secret_tag() {
//         use Confidentiality::*;
//         use types::Output;
//
//         let output = Rc::new(RefCell::new(Vec::new()));
//         let program = vec![add_instr!(tag Push, 42, Secret), add_instr!(io Print, 0)];
//         let mut machine = Executor::with_policy(program, confidentiality_policy()).unwrap();
//         machine.redirect_output(Output::Buffer(output.clone()));
//         let result = machine.exec();
//
//         assert!(matches!(
//             result,
//             Err(ExecutorError::Flow(FlowError::InformationFlowViolation {
//                 found: Secret,
//                 guard: Public,
//             }))
//         ));
//         assert!(output.borrow().is_empty());
//     }
//
//     #[test]
//     /// This test fails not because the values printed are themselves secret, but because the
//     /// condition of the `IfElse` statement is secret.
//     fn output_pg_rejects_public_value_under_private_control() {
//         use Confidentiality::*;
//
//         let output = Rc::new(RefCell::new(Vec::new()));
//         let program = vec![
//             add_instr!(tag Push, 1, Secret),
//             add_instr!(tag Push, 42, Public),
//             add_instr!(ifelse 0, add_instr!(io Print, 1), add_instr!(Nop)),
//         ];
//         let mut machine = Executor::with_policy(program, confidentiality_policy()).unwrap();
//         machine.redirect_output(types::Output::Buffer(output.clone()));
//         let result = machine.exec();
//
//         assert!(matches!(
//             result,
//             Err(ExecutorError::Flow(FlowError::InformationFlowViolation {
//                 found: Secret,
//                 guard: Public,
//             }))
//         ));
//         assert!(output.borrow().is_empty());
//     }
//
//     #[test]
//     /// This test executes correctly
//     fn output_perimeter_accepts_public_output() {
//         let output = Rc::new(RefCell::new(Vec::new()));
//         let program = vec![add_instr!(Push, 42), add_instr!(io Print, 0)];
//         let mut machine = Executor::with_policy(program, confidentiality_policy()).unwrap();
//         machine.redirect_output(types::Output::Buffer(output.clone()));
//         machine.exec().unwrap();
//
//         assert_eq!(*output.borrow(), vec![42]);
//     }
// }
//
// fn fact(n: i64) -> i64 {
//     if n <= 1 {
//         return 1;
//     }
//     n * fact(n - 1)
// }
//
// fn fib(n: i64) -> i64 {
//     if n <= 1 {
//         return 1;
//     }
//     fib(n - 1) + fib(n - 2)
// }
//
// #[test]
// fn factorial() {
//     init();
//     let number = 10;
//
//     let program = programs::prog_factorial(number);
//
//     assert_eq_last_Int!(program, fact(number));
// }
//
// #[test]
// fn fibonacci() {
//     init();
//     let number = 10;
//
//     let program = programs::prog_fibonacci(number);
//
//     assert_eq_last_Int!(program, fib(number));
// }
//
// #[test]
// fn factorial_weird() {
//     init();
//
//     let program = programs::special_argument_providing();
//
//     assert_eq_last_Int!(program, fact(5));
// }
