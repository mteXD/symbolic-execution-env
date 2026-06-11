//! These programs are more complex and meant to demonstrate the capabilities of this project.
//!
//! They are also used in tests in order to ensure that the verifier and executor can handle more
//! realistic programs.
//!
//! All programs here:
//! - Are positive tests (should not cause errors).
//! - Do not use information flow tracking; refer to [`showcase_diftam`] for examples of that.

use super::*;

pub fn factorial() -> Snippet {
    prog!(
        add_instr!(fun FunctionDefine, String::from("factorial")),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 1
            add_instr!(SetGreaterThan, 0, 1), // n > 1
            add_instr!(ifelse 2, // if n <= 1, skip to return
                make_block!(
                    add_instr!(Push, -1),
                    add_instr!(Add, 0, 3), // n - 1
                    add_instr!(fun FunctionCall, String::from("factorial")), // else, calculate factorial(n - 1)
                    add_instr!(Mul, 0, 5)                                    // n * factorial(n - 1
                ),
                add_instr!(Push, 1)
            )
        ),
        add_instr!(io Input, 0),
        add_instr!(fun FunctionCall, String::from("factorial")),
    )
}

pub fn factorial_alt() -> Snippet {
    prog!(
        add_instr!(fun FunctionDefine, "factorial"),
        make_block!(
            add_instr!(R Read, 1),
            add_instr!(Rebase),
            make_block!(
                // argument of next function call
                add_instr!(Push, -1),  // Push -1
                add_instr!(Add, 0, 1)  // n - 1
            ),
            add_instr!(Push, 1),
            add_instr!(SetGreaterThan, 0, 2),
            add_instr!(ifelse 3, // if n <= 1, skip to return
                make_block!(
                    add_instr!(fun FunctionCall, String::from("factorial")), // else, calculate factorial(n - 1)
                    add_instr!(Mul, 0, 4)                                    // n * factorial(n - 1)
                ),
                add_instr!(Push, 1)
            )
        ),
        add_instr!(Push, -1),
        add_instr!(io Input, 0),
        add_instr!(fun FunctionCall, "factorial"),
    )
}

pub fn fibonacci() -> Snippet {
    prog!(
        add_instr!(fun FunctionDefine, String::from("fibonacci")),
        make_block!(
            add_instr!(R ReadReverse, 0), // n
            add_instr!(Rebase),
            add_instr!(Push, 1),              // 2
            add_instr!(SetGreaterThan, 0, 1), // n > 2
            add_instr!(ifelse 2, // if n <= 1, skip to return
                make_block!(
                    add_instr!(Push, -1),
                    add_instr!(Add, 0, 3), // n - 1
                    add_instr!(fun FunctionCall, String::from("fibonacci")), // calculate fibonacci(n - 1)
                    add_instr!(Add, 4, 3),                                   // (n - 1) - 1 = n - 2
                    add_instr!(fun FunctionCall, String::from("fibonacci")), // calculate fibonacci(n - 2)
                    add_instr!(Add, 5, 7) // fibonacci(n - 1) + fibonacci(n - 2)
                ),
                add_instr!(Push, 1)
            )
        ),
        add_instr!(io Input, 0),
        add_instr!(fun FunctionCall, String::from("fibonacci")),
    )
}
