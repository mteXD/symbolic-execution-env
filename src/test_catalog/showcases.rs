//! Showcase tests: larger, more realistic programs.
//!
//! Both sides use `custom` bodies: the verifier asserts the program is
//! accepted, and the executor runs it on a concrete input and checks the
//! top-of-stack result against a reference implementation.

use super::*;

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

test_program! {
    /// Recursive factorial.
    factorial,
    program: vec![
        add_instr!(fun FunctionDefine, "factorial"),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 1
            add_instr!(SetGreaterThan, 0, 1), // n > 1
            add_instr!(ifelse 2, // if n <= 1, skip to return
                make_block!(
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
    /// Recursive factorial, alternative argument-passing style.
    factorial_alt,
    program: vec![
        add_instr!(fun FunctionDefine, "factorial"),
        make_block!(
            add_instr!(R Read, 1),
            add_instr!(Rebase),
            make_block!(
                // argument of next function call
                add_instr!(Push, -1), // Push -1
                add_instr!(Add, 0, 1) // n - 1
            ),
            add_instr!(Push, 1),
            add_instr!(SetGreaterThan, 0, 2),
            add_instr!(ifelse 3, // if n <= 1, skip to return
                make_block!(
                    add_instr!(fun FunctionCall, "factorial"), // else, factorial(n - 1)
                    add_instr!(Mul, 0, 4)                      // n * factorial(n - 1)
                ),
                add_instr!(Push, 1)
            )
        ),
        add_instr!(Push, -1),
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
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 2
            add_instr!(SetGreaterThan, 0, 1), // n > 2
            add_instr!(ifelse 2, // if n <= 1, skip to return
                make_block!(
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
        make_block!(
            add_instr!(R ReadReverse, 1), // remaining
            add_instr!(R ReadReverse, 1), // reversed
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 2), // remaining == 0
            add_instr!(ifelse 3,
                add_instr!(R Read, 1), // return reversed
                make_block!(
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
        make_block!(
            add_instr!(R ReadReverse, 0), // x
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetLessThan, 0, 1), // x < 0
            add_instr!(ifelse 2,
                add_instr!(Push, 0),
                make_block!(
                    add_instr!(R Read, 0), // remaining
                    add_instr!(Push, 0),   // reversed
                    add_instr!(fun FunctionCall, "reverse_digits"),
                    add_instr!(SetEqual, 0, 5)
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
