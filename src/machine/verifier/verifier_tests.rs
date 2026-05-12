use crate::{
    add_instr,
    instruction::{BinaryOp, FunctionOp, Instruction::*, NullaryOp, UnaryOpCell, UnaryOpImm},
    machine::verifier::Verifier,
    make_block,
};

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
        }
    };
}

#[test]
fn test_push() {
    let prog = vec![
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(Push, 3),
        add_instr!(Push, 4),
        add_instr!(Push, 5),
    ];
    let mut verifier = Verifier::new(&prog);
    assert!(verifier.verify().is_ok());
}

#[test]
fn test_pop_good() {
    let prog = vec![add_instr!(Push, 1), add_instr!(R Pop, 1)];

    let mut verifier = Verifier::new(&prog);
    assert!(verifier.verify().is_ok());
}

#[test]
fn test_pop_multiple_good() {
    let prog = vec![
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(Push, 3),
        add_instr!(Push, 4),
        add_instr!(R Pop, 1),
        add_instr!(Push, 4),
        add_instr!(R Pop, 4),
    ];

    let mut verifier = Verifier::new(&prog);
    assert!(verifier.verify().is_ok());
}

#[test]
fn test_pop_bad() {
    let prog = vec![
        add_instr!(R Pop, 1), // Trying to pop from empty cells
    ];

    let mut verifier = Verifier::new(&prog);
    assert!(verifier.verify().is_err());
}

#[test]
fn test_pop_multiple_bad() {
    let prog = vec![
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(Push, 3),
        add_instr!(R Pop, 4), // 3 elements available, but trying to pop 4
    ];

    let mut verifier = Verifier::new(&prog);
    assert!(verifier.verify().is_err());
}

#[test]
fn test_read() {
    let program = vec![
        add_instr!(Push, 100),
        add_instr!(Push, 200),
        add_instr!(R Read, 0), // Read from cell 0
    ];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_ok());
}

#[test]
fn test_read_bad_empty() {
    let program = vec![add_instr!(R Read, 0)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());
}

#[test]
fn test_read_bad_index() {
    let program = vec![add_instr!(Push, 100), add_instr!(R Read, 1)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());
}

#[test]
fn test_read_reverse() {
    let program = vec![
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Push, 30),
        add_instr!(R ReadReverse, 1), // Should read 20
    ];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_ok());
}

#[test]
fn test_read_reverse_bad_empty() {
    // PART 1

    let program = vec![add_instr!(R ReadReverse, 0)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());

    // PART 2

    let program = vec![add_instr!(R ReadReverse, 42)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());
}

#[test]
fn test_read_reverse_bad_index() {
    // PART 1

    let program = vec![add_instr!(Push, 10), add_instr!(R ReadReverse, 1)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());

    // PART 2

    let program = vec![add_instr!(Push, 10), add_instr!(R ReadReverse, 42)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());
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
#[ignore]
fn test_div_bad() {
    let program = vec![
        add_instr!(Push, 10),
        add_instr!(Push, 0),
        add_instr!(Div, 0, 1),
    ];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());
}

#[test]
fn test_not() {
    let program = vec![add_instr!(Push, 0b1100), add_instr!(R Not, 0)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_ok());
}

#[test]
fn test_not_bad() {
    let program = vec![add_instr!(R Not, 0)];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_err());
}

#[test]
fn nop() {
    let program = vec![add_instr!(Nop)];
    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_ok());
}

#[test]
fn math_with_read() {
    let program = vec![
        add_instr!(Push, 50),
        add_instr!(Push, 70),
        add_instr!(Push, 10),
        add_instr!(Add, 0, 1), // 50 + 70 = 120
        add_instr!(Div, 3, 2), // 120 / 10 = 12
    ];

    let mut verifier = Verifier::new(&program);
    assert!(verifier.verify().is_ok());
}

mod blocks {
    use super::*;

    #[test]
    fn test_no_arg() {
        let program = vec![
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Add, 0, 1),
            make_block!(
                add_instr!(Push, 2),   // This push should be deleted after block ends
                add_instr!(Mul, 2, 3) // This is the last push, the result, and it should remain after block ends
            ),
            add_instr!(Add, 2, 3),
        ];

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
    }

    #[test]
    fn test_nested() {
        let program = vec![
            add_instr!(Push, 3),
            make_block!(
                add_instr!(Push, 4),
                make_block!(
                    add_instr!(Push, 5),
                    add_instr!(Mul, 1, 2) // 4 * 5 = 20
                ),
                add_instr!(Add, 0, 2) // 3 + 20 = 23
            ),
        ];

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
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

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
    }

    #[test]
    fn test_with_pop() {
        let block = make_block!(
            add_instr!(R Pop, 2) // Pop the 20, leaving only 30
        );

        let program = vec![
            add_instr!(Push, 3),
            add_instr!(Push, 5),
            block,
            add_instr!(Mul, 0, 1), // 3 * 5 = 15
        ];

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
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

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
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

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
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

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
    }
}

mod functions {
    use super::*;

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

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
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
        ];

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
    }

    #[test]
    fn test_nested_functions() {
        let program = vec![
            add_instr!(fun FunctionDefine, String::from("outer")),
            make_block!(
                add_instr!(fun FunctionDefine, String::from("inner")),
                make_block!(add_instr!(Push, 42)),
                add_instr!(fun FunctionCall, String::from("inner"))
            ),
            add_instr!(fun FunctionCall, String::from("outer")),
        ];

        // Outer function call should work
        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
    }

    #[test]
    fn test_nested_functions_bad() {
        let program = vec![
            add_instr!(fun FunctionDefine, String::from("outer")),
            make_block!(
                add_instr!(fun FunctionDefine, String::from("inner")),
                make_block!(add_instr!(Push, 42)),
                add_instr!(fun FunctionCall, String::from("inner"))
            ),
            add_instr!(fun FunctionCall, String::from("outer")),
            add_instr!(fun FunctionCall, String::from("inner")), // This should fail
        ];

        // Inner function call should fail
        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_err());
    }
}

mod programs {
    use super::*;

    #[test]
    fn test_factorial() {
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

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
    }

    /*
     * Things that can go wrong:
     * - Not enough cells for the recursive calls (should be caught by the rebase checks)
     * - Function not defined
     * - Stack underflow
     * - Infinite recursion (something connected to the Cond instruction)
     * - Integer overflow
     * - Not enough arguments after rebase
     * - Make a data structure that will hold, for each function:
     *   - is it recursive
     *   - if so, is it finite
     *     - FunctionCall must be inside a conditional block
     *     - Cond instruction must immediately follow some comparison executed on a critical value
     *       that decreases in each recursion
     *   - if so, what is permissible data input to make it final, using a predicate system,
     *     something like:
     *     - critical value (e.g. "3", "0", "1"...)
     *     - qualifier (e.g. "Greater Than", "Equal", "Greater or equal"...)
     *     - predicates can be combined, maybe? (e.g. {0, "Greater Than"} OR {0, "Equal"}, which
     *       would result in >= 0)
     *
     *
     * Other things to do:
     * - think about implementing pseudo-instructions (e.g. CondBlock that expands into a Block
     *   preceeded by a Cond)
     * - support for chars and strings
     *   - arithmetic (and others) not allowed, or maybe specific arithmetic instructions
     * - support for arbitrary input:
     *   - user input, file input?
     *   - in this case, if statements need to be verified in a fork-and-join manner. Possible need
     *     for implementing Kildall's algorithm.
     * - better error messages
     * - some src/main.rs that runs some predefined programs
     * - Maximum stack size for verification (and maybe execution)
     * - Maximum recursion depth for verification (and maybe execution)
     *
     * Some differences with other systems:
     * - SSA is already built-in (unavoidable for the programmer), except for the fact that cells
     *   can be dropped with pop()
     * - Jumps are only in form of function calls, and so it is impossible to jump to invalid code
     *
     */
    #[test]
    fn test_fibonacci() {
        let number = 10;

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

        let mut verifier = Verifier::new(&program);
        assert!(verifier.verify().is_ok());
    }
}
