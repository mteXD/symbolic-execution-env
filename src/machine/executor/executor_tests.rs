use super::*;

use crate::{
    add_instr,
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{
            AluBinary, AluFunction, AluIntrinsic, AluNullary, AluUnaryCell, AluUnaryImm, Block,
        },
        NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{CoreError, executor::Executor},
    make_block, programs,
    types::FunctionDataError,
};

macro_rules! assert_eq_last {
    ($prog:expr, $value:expr) => {
        let mut executor = Executor::new(&$prog);
        let val = executor
            .exec()
            .expect("Program should execute successfully");
        assert_eq!(val, $value);
    };
}

macro_rules! assert_eq_last_Int {
    ($prog:expr, $value:expr) => {
        assert_eq_last!($prog, Some(&Cell::Integer($value)));
    };
}

macro_rules! test_binop {
    ($name:ident, $a:expr, $b:expr, $op:ident => $expected:expr) => {
        #[test]
        fn $name() {
            let program = vec![
                add_instr!(Push, $a),
                add_instr!(Push, $b),
                add_instr!($op, 0, 1),
            ];
            assert_eq_last_Int!(program, $expected);
        }
    };
}

#[test]
fn test_push_pop() {
    fn cell_checker(executor: &Executor, expected_len: usize) {
        let len = executor.cells.len();
        assert_eq!(len, expected_len);
        for i in 0..len {
            assert_eq!(executor.cells[i], Cell::Integer((i + 1) as i64));
        }
    }

    let mut program = programs::push5();
    let mut executor = Executor::new(&program);
    let result = executor.exec();
    assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    cell_checker(&executor, 5);

    program.extend(vec![
        add_instr!(R Pop, 4),  // Pops 4
        add_instr!(R Read, 0), // Reads remaining one
    ]);
    let mut executor = Executor::new(&program);
    let result = executor.exec();
    assert!(result.is_ok(), "Verification failed: {:?}", result.err());
    assert_eq!(executor.cells.len(), 2);

    program.extend(vec![
        add_instr!(R Pop, 2),  // Should pop 3 and 2
        add_instr!(R Read, 0), // Should fail
    ]);

    let mut executor = Executor::new(&program);
    let result = executor.exec();
    if let Err(ExecutorError::InvalidCell) = result {
        // Test passed
    } else {
        panic!("Expected InvalidCell error, but got {:?}", result);
    }
}

#[test]
fn read() {
    let program = programs::read();
    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(100)));
    assert_eq!(machine.cells[0], 100);
    assert_eq!(machine.cells[1], 200);
}

#[test]
fn read_reverse() {
    let program = programs::read_reverse();
    assert_eq_last_Int!(program, 20);
}

test_binop!(test_add, 10, 20, Add => 30);
test_binop!(test_add_neg, 10, -30, Add => -20);
test_binop!(test_mul, 10, 20, Mul => 200);
test_binop!(test_div, 20, 5, Div => 4);

#[test]
fn div_by_0() {
    let program = programs::div_by_0();
    let mut machine = Executor::new(&program);
    let last = machine.exec();
    match last {
        Err(DivisionByZero) => (), // Expected error
        _ => panic!("Expected DivisionByZero error, got {:?}", last),
    }
}

test_binop!(test_and, 0b1100, 0b1010, And => 0b1000);
test_binop!(test_or, 0b1100, 0b1010, Or => 0b1110);
test_binop!(test_xor, 0b1100, 0b1010, Xor => 0b0110);

#[test]
fn test_not() {
    let program = vec![add_instr!(Push, 0b1100), add_instr!(R Not, 0)];
    assert_eq_last_Int!(program, !0b1100);
}

test_binop!(test_slt, 10, 20, SetLessThan => 1);
test_binop!(test_sgt, 20, 10, SetGreaterThan => 1);
test_binop!(test_seq, 10, 10, SetEqual => 1);
test_binop!(test_sne, 10, 20, SetNotEqual => 1);
test_binop!(test_sle, 10, 10, SetLessThanOrEqual => 1);
test_binop!(test_sge, 20, 10, SetGreaterThanOrEqual => 1);

test_binop!(test_sll, 0b0001, 2, ShiftLeftLogical => 0b0100);
test_binop!(test_srl, 0b0100, 2, ShiftRightLogical => 0b0001);
test_binop!(test_sra, -8, 2, ShiftRightArithmetic => -2);

#[test]
fn nop() {
    let program = vec![add_instr!(Nop)];
    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, None);
}

#[test]
fn math_with_read() {
    let program = programs::math_with_read();
    assert_eq_last_Int!(program, 12);
}

#[test]
fn basic_block() {
    let program = programs::basic_block();

    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(90))); // (10 + 20) + ((10 + 20) * 2) = 90

    assert_eq!(machine.cells[0], 10);
    assert_eq!(machine.cells[1], 20);
    assert_eq!(machine.cells[2], 30); // Result of first addition
    assert_eq!(machine.cells[3], 60); // Result of multiplication inside block
    assert_eq!(machine.cells[4], 90); // Final result
    assert!(matches!(machine.cells.get(5), None)); // Ensure no extra cells exist
    assert_eq!(machine.cells.len(), 5);
}

#[test]
fn nested_block() {
    let program = programs::nested_block();
    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(23)));
    assert_eq!(machine.cells[0], 3);
    assert_eq!(machine.cells[1], 23);
}

#[test]
fn test_square_fn() {
    let square_block = make_block!(
        add_instr!(R ReadReverse, 0),
        add_instr!(R ReadReverse, 0),
        add_instr!(Rebase),
        add_instr!(Mul, 0, 1) // Multiply input by 2
    );

    let program = vec![
        add_instr!(Push, 2),
        square_block.clone(),
        square_block.clone(),
    ];

    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(16))); // (2^2)^2 = 16
}

#[test]
fn block_with_pop() {
    let program = programs::block_with_pop();
    assert_eq_last_Int!(program, 15);
}

#[test]
fn test_nested_rebase_1() {
    let program = vec![
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
    ];
    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(15)));
    assert_eq!(machine.cells[0], 2);
    assert_eq!(machine.cells[1], 15);
}

#[test]
fn test_nested_rebase_2() {
    let program = vec![
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
    ];
    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(15)));
    assert_eq!(machine.cells[0], 2);
    assert_eq!(machine.cells[1], 15);
}

#[test]
fn test_square_add_42() {
    let program = vec![
        add_instr!(Push, 5), // Argument
        make_block!(
            add_instr!(R ReadReverse, 0), // Read x . . . r0 <- x
            add_instr!(Rebase),
            add_instr!(Mul, 0, 0), // x ^ 2 . . . r1 <- r0 ^ 2
            add_instr!(Push, 42),  // r2 <- 42
            add_instr!(Mul, 0, 2), // x * 42 . . . r3 <- r0 * r2
            add_instr!(Add, 1, 3)  // x^2 + 42x . . . r4 <- r1 + r3
        ),
    ];

    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(235))); // 5^2 + 42*5 = 25 + 210 = 235
    assert_eq!(machine.cells[0], 5);
    assert_eq!(machine.cells[1], 235);
    assert_eq!(machine.cells.len(), 2);
}

#[test]
fn test_simple_function() {
    let program = vec![
        add_instr!(fun FunctionDefine, String::from("square")),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Mul, 0, 0) // Multiply input by 2
        ),
        add_instr!(Push, 3),
        add_instr!(fun FunctionCall, String::from("square")),
    ];

    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(9))); // 3^2 = 9
}

#[test]
fn test_sequential_definitions() {
    let program = vec![
        add_instr!(fun FunctionDefine, String::from("push2_1")),
        add_instr!(fun FunctionDefine, String::from("push2_2")),
        add_instr!(fun FunctionDefine, String::from("push2_3")),
        add_instr!(Push, 2),
        add_instr!(fun FunctionCall, String::from("push2_1")),
        add_instr!(fun FunctionCall, String::from("push2_2")),
        add_instr!(fun FunctionCall, String::from("push2_3")),
    ];

    let mut machine = Executor::new(&program);
    let _ = machine.exec().unwrap();

    assert_eq!(machine.cells[0], 2);
    assert_eq!(machine.cells[1], 2);
    assert_eq!(machine.cells[2], 2);
    assert!(matches!(machine.cells.get(3), None));
}

#[test]
fn test_nested_functions() {
    let mut program = vec![
        add_instr!(fun FunctionDefine, String::from("outer")),
        make_block!(
            add_instr!(fun FunctionDefine, String::from("inner")),
            make_block!(add_instr!(Push, 42)),
            add_instr!(fun FunctionCall, String::from("inner"))
        ),
        add_instr!(fun FunctionCall, String::from("outer")),
    ];

    // Outer function call should work
    assert_eq_last_Int!(program, 42);

    // Inner function call should fail
    program.push(add_instr!(fun FunctionCall, String::from("inner")));
    let mut machine = Executor::new(&program);
    let last = machine.exec();
    assert!(matches!(
        last,
        Err(Core(CoreError::FunctionDataError(
            FunctionDataError::FunctionUndefined(_)
        )))
    ));
}

#[test]
fn test_print() {
    let program = vec![
        add_instr!(Push, 123),
        add_instr!(io Print, 0), // Should print 123
    ];

    let mut machine = Executor::new(&program);
    let last = machine.exec().unwrap();
    assert_eq!(last, Some(&Cell::Integer(123)));
}

use std::sync::Once;
static INIT: Once = Once::new();

pub fn init() {
    INIT.call_once(|| {
        env_logger::builder()
            .is_test(true)
            .format_timestamp(None)
            .format_target(false)
            .format_file(true)
            .format_line_number(true)
            .format_module_path(false)
            .init();
    });
}

#[test]
fn test_factorial() {
    fn factorial(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        n * factorial(n - 1)
    }

    init();
    let number = 10;

    let program = vec![
        add_instr!(fun FunctionDefine, String::from("factorial")),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 1
            add_instr!(SetGreaterThan, 0, 1), // n > 1
            add_instr!(Cond),                 // if n <= 1, skip to return
            make_block!(
                add_instr!(Push, -1),  // Push 1 as the base case result
                add_instr!(Add, 0, 2), // n - 1
                add_instr!(fun FunctionCall, String::from("factorial")), // else, calculate factorial(n - 1)
                add_instr!(Mul, 0, 4)                                    // n * factorial(n - 1
            )
        ),
        add_instr!(Push, number),
        add_instr!(fun FunctionCall, String::from("factorial")),
    ];

    assert_eq_last_Int!(program, factorial(number));
}

#[test]
fn test_fibonacci() {
    fn fib(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        fib(n - 1) + fib(n - 2)
    }

    init();
    let number = 4;

    let program = vec![
        add_instr!(fun FunctionDefine, String::from("fibonacci")),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 1
            add_instr!(SetGreaterThan, 0, 1), // n > 1
            add_instr!(Cond),                 // if n <= 1, skip to return
            make_block!(
                add_instr!(Push, -1),  // Push 1 as the base case result
                add_instr!(Add, 0, 2), // n - 1
                add_instr!(fun FunctionCall, String::from("fibonacci")), // else, calculate fibonacci(n - 1)
                add_instr!(Add, 3, 2),                                   // (n - 1) - 1 = n - 2
                add_instr!(fun FunctionCall, String::from("fibonacci")), // else, calculate fibonacci(n - 2)
                add_instr!(Add, 4, 6) // fibonacci(n - 1) + fibonacci(n - 2)
            )
        ),
        add_instr!(Push, number),
        add_instr!(fun FunctionCall, String::from("fibonacci")),
    ];

    assert_eq_last_Int!(program, fib(number));
}

#[test]
#[ignore]
fn test_fibonacci_bad() {
    fn fib(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        fib(n - 1) + fib(n - 2)
    }

    init();
    let number = 4;

    let program = vec![
        add_instr!(fun FunctionDefine, String::from("fibonacci")),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 1
            add_instr!(SetGreaterThan, 0, 1), // n > 1
            add_instr!(Cond),                 // if n <= 1, skip to return
            make_block!(
                add_instr!(Push, -1),  // Push 1 as the base case result
                add_instr!(Add, 0, 2), // n - 1
                add_instr!(fun FunctionCall, String::from("fibonacci")), // else, calculate fibonacci(n - 1)
                add_instr!(Add, 3, 2),                                   // (n - 1) - 1 = n - 2
                add_instr!(fun FunctionCall, String::from("fibonacci")), // else, calculate fibonacci(n - 2)
                add_instr!(Add, 4, 6) // fibonacci(n - 1) + fibonacci(n - 2)
            )
        ),
        // add_instr!(Push, number),
        add_instr!(fun FunctionCall, String::from("fibonacci")),
    ];

    assert_eq_last_Int!(program, fib(number));
}
