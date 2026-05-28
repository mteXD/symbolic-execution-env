use std::{cell::RefCell, rc::Rc};

use crate::{
    add_instr,
    instruction::{BinaryOp, FunctionOp, Instruction::*, NullaryOp, UnaryOpCell, UnaryOpImm},
    machine::verifier::{ValueSpan, Verifier, VerifierError},
    make_block, programs,
    types::{self, Immediate},
};

use VerifierError::*;

macro_rules! assert_last {
    ($prog:expr) => {
        let mut verifier = Verifier::new(&$prog);
        let result = verifier.verify();
        assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    };
}

macro_rules! assert_last_err {
    ($prog:expr, $err:pat) => {
        let mut verifier = Verifier::new(&$prog);
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

            let mut verifier = Verifier::new(&program);
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

#[test]
fn test_push_pop() {
    let mut program = programs::push5();
    let mut verifier = Verifier::new(&program);
    let result = verifier.verify();
    assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    let len = verifier.cells.len();
    assert_eq!(len, 5);
    for i in 0..len {
        assert_eq!(
            verifier.cells[i],
            ValueSpan {
                min: (i + 1) as i64,
                max: (i + 1) as i64
            }
        );
    }

    program.extend(vec![
        add_instr!(R Pop, 4),  // Pops 4
        add_instr!(R Read, 0), // Reads remaining one
    ]);
    let mut verifier = Verifier::new(&program);
    let result = verifier.verify();
    assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    let len = verifier.cells.len();
    assert_eq!(len, 2);
    for i in 0..len {
        assert_eq!(
            verifier.cells[i],
            ValueSpan {
                min: (1) as i64,
                max: (1) as i64
            }
        );
    }

    program.extend(vec![
        add_instr!(R Pop, 2),  // Should pop 3 and 2
        add_instr!(R Read, 0), // Should fail
    ]);

    let mut verifier = Verifier::new(&program);
    let result = verifier.verify();
    if let Err(VerifierError::InvalidCell {
        instr: _,
        cell_index: _,
        cells: _,
        prog: _,
        location: _,
    }) = result
    {
        // Test passed
    } else {
        panic!("Expected InvalidCell error, but got {:?}", result);
    }
}

#[test]
fn pop_multiple_bad() {
    let program = programs::pop_multiple_bad();
    assert_last_err!(program, StackUnderflow);
}

#[test]
fn read_bad_index() {
    let program = programs::read_bad_index();
    assert_last_err!(program, _InvalidCell);
}

create_test!(read_reverse);

#[test]
fn test_read_reverse_bad_empty() {
    // PART 1
    let program = programs::read_reverse_bad_empty_1();
    assert_last_err!(program, _InvalidCell);

    // PART 2
    let program = programs::read_reverse_bad_empty_2();
    assert_last_err!(program, _InvalidCell);
}

#[test]
fn test_read_reverse_bad_index() {
    // PART 1
    let program = programs::read_reverse_bad_index_1();
    assert_last_err!(program, _InvalidCell);

    // PART 2
    let program = programs::read_reverse_bad_index_2();
    assert_last_err!(program, _InvalidCell);
}

test_binop!(test_add, Add);
test_binop!(test_add_neg, Add);
test_binop!(test_mul, Mul);
test_binop!(test_div, Div);
test_binop!(test_and, And);
test_binop!(test_or, Or);
test_binop!(test_xor, Xor);
test_binop!(test_slt, SetLessThan);
test_binop!(test_sgt, SetGreaterThan);
test_binop!(test_seq, SetEqual);
test_binop!(test_sne, SetNotEqual);
test_binop!(test_sle, SetLessThanOrEqual);
test_binop!(test_sge, SetGreaterThanOrEqual);
test_binop!(test_sll, ShiftLeftLogical);
test_binop!(test_srl, ShiftRightLogical);
test_binop!(test_sra, ShiftRightArithmetic);

#[test]
fn test_div_bad() {
    let program = vec![
        add_instr!(Push, 10),
        add_instr!(Push, 0),
        add_instr!(Div, 0, 1),
    ];
    assert_last_err!(program, DivisionByZero);
}

create_test!(nop);
create_test!(bitwise_not);

#[test]
fn test_not_bad() {
    let program = vec![add_instr!(R Not, 0)];
    assert_last_err!(program, _InvalidCell);
}

create_test!(math_with_read);

#[test]
fn conditional() {
    let program = programs::conditional();
    let mut verifier = Verifier::new(&program);
    let result = verifier.verify();
    assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    let result = result.unwrap();
    assert_eq!(result, Some(&ValueSpan { min: 4, max: 6 }));
}

#[test]
#[ignore]
fn conditional_problem() {
    let program = programs::conditional_problem();
    let mut verifier = Verifier::new(&program);

    let new_input: Rc<RefCell<Vec<Immediate>>> = Rc::new(RefCell::new(vec![10]));
    verifier.redirect_input(types::Input::Buffer(new_input.clone()));

    let result = verifier.verify();
    // assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    // let result = result.unwrap();
    match result {
        Err(VerifierError::CondInvalidCell {
            instr: _,
            cell_index: _,
            cells: _,
            prog: _,
            location: _,
        }) => (),
        _ => panic!("Expected CondInvalidCell error, but got {:?}", result),
    }
}

mod blocks {
    use super::*;

    create_test!(basic_block);
    create_test!(nested_block);
    create_test!(block_with_pop);
    create_test!(block_nested_rebase_1);
    create_test!(block_nested_rebase_2);
    create_test!(square_add_42);

    #[test]
    fn void_print_block() {
        let program = programs::void_print_block();

        let mut verifier = Verifier::new(&program);
        let result = verifier.verify();
        assert!(result.is_ok(), "Verification failed: {:?}", result.err());

        assert_eq!(verifier.cells.len(), 2);
    }

    #[test]
    fn block_with_pops_only() {
        let program = programs::block_with_pops_only();

        let mut verifier = Verifier::new(&program);
        let result = verifier.verify();
        match result {
            Err(BlockHasEmptyStack) => (),
            _ => panic!("Expected BlockHasEmptyStack error, but got {:?}", result),
        }
    }
}

mod intrinsics {
    use super::*;
    use crate::instruction::{
        // self,
        IntrinsicOp,
    };

    // print probably doesn't need to be tested
    #[test]
    #[ignore]
    fn print() {
        todo!()
    }

    #[test]
    fn input() {
        let program = vec![add_instr!(io Input, 0)];

        let mut verifier = Verifier::new(&program);
        let result = verifier.verify();
        assert!(result.is_ok(), "Verification failed: {:?}", result.err());
        let result = result.unwrap();
        assert_eq!(result, Some(&ValueSpan::inf()));
    }
}

mod whole_programs {
    use super::*;

    #[test]
    fn test_factorial() {
        let program = programs::prog_factorial(10);
        assert_last!(program);
    }

    #[test]
    fn test_fibonacci() {
        let program = programs::prog_fibonacci(10);
        // panic!("Termination.");
        assert_last!(program);
    }
}
