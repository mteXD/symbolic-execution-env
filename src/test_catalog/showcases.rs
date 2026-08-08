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

test_program! {
    /// Euclidean GCD for nonnegative integer operands.
    euclidean_gcd,
    program: vec![
        add_instr!(fun FunctionDefine, "gcd"),
        make_block!(
            add_instr!(R ReadReverse, 1), // cell 0: current first operand
            add_instr!(R ReadReverse, 1), // cell 1: current second operand
            add_instr!(Rebase),           // cells 0-1: function arguments
            add_instr!(Push, 0),          // cell 2: zero constant
            add_instr!(SetEqual, 1, 2),   // cell 3: second operand == 0
            add_instr!(ifelse 3,
                add_instr!(R Read, 0), // cell 4: return first operand
                make_block!(
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
        make_block!(
            add_instr!(R ReadReverse, 1), // cell 0: base
            add_instr!(R ReadReverse, 1), // cell 1: exponent
            add_instr!(Rebase),           // cells 0-1: function arguments
            add_instr!(Push, 0),          // cell 2: zero constant
            add_instr!(SetEqual, 1, 2),   // cell 3: exponent == 0
            add_instr!(ifelse 3,
                add_instr!(Push, 1), // cell 4: base-case result
                make_block!(
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
        make_block!(
            add_instr!(R ReadReverse, 0), // cell 0: remaining bits
            add_instr!(Rebase),           // cell 0: function argument
            add_instr!(Push, 0),          // cell 1: zero constant
            add_instr!(SetEqual, 0, 1),   // cell 2: remaining bits == 0
            add_instr!(ifelse 2,
                add_instr!(Push, 0), // cell 3: base-case count
                make_block!(
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
        make_block!(
            add_instr!(R ReadReverse, 1), // cell 0: number
            add_instr!(R ReadReverse, 1), // cell 1: candidate divisor
            add_instr!(Rebase),           // cells 0-1: function arguments
            add_instr!(Div, 0, 1),        // cell 2: number / divisor
            add_instr!(SetGreaterThan, 1, 2), // cell 3: divisor > quotient
            add_instr!(ifelse 3,
                add_instr!(Push, 1), // cell 4: no divisor remains
                make_block!(
                    add_instr!(Mul, 2, 1), // cell 4: quotient * divisor
                    add_instr!(Push, -1), // cell 5: negation constant
                    add_instr!(Mul, 4, 5), // cell 6: negated product
                    add_instr!(Add, 0, 6), // cell 7: derived remainder
                    add_instr!(Push, 0), // cell 8: zero constant
                    add_instr!(SetEqual, 7, 8), // cell 9: divisible
                    add_instr!(ifelse 9,
                        add_instr!(Push, 0), // cell 10: composite result
                        make_block!(
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
        make_block!(
            add_instr!(R ReadReverse, 0), // cell 0: number
            add_instr!(Rebase),           // cell 0: function argument
            add_instr!(Push, 2),          // cell 1: minimum prime/divisor
            add_instr!(SetLessThan, 0, 1), // cell 2: number < 2
            add_instr!(ifelse 2,
                add_instr!(Push, 0), // cell 3: below-two result
                make_block!(
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
