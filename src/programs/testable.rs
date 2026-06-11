use super::*;

/// These programs ensure correctness of basic stack operations
pub mod stack {
    use super::*;

    /// [POSITIVE] Pushes 5 values onto the stack
    pub fn push() -> Snippet {
        prog!(
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(Push, 4),
            add_instr!(Push, 5),
        )
    }

    /// [POSITIVE] Pushes 3 values, then pops 2, leaving 1 on the stack
    pub fn pop_most() -> Snippet {
        prog!(
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(R Pop, 2),
        )
    }

    /// [POSITIVE] Pushes 3 values, then pops all 3, leaving stack empty
    pub fn pop_all() -> Snippet {
        prog!(
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(R Pop, 3),
        )
    }

    /// [NEGATIVE] Tries to pop from an empty stack, which should cause an error
    pub fn pop_empty() -> Snippet {
        prog!(add_instr!(R Pop, 1))
    }

    /// [NEGATIVE] Pushes 3 values, then tries to pop 4, which should cause an error
    pub fn pop_too_many() -> Snippet {
        prog!(
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(R Pop, 4), // Error
        )
    }

    /// [POSITIVE] Pushes a value and reads it back using Read with index 0
    pub fn read() -> Snippet {
        prog!(add_instr!(Push, 42), add_instr!(R Read, 0))
    }

    /// [NEGATIVE] Tries to read from an empty stack
    ///
    /// Expected: `StackUnderflow`
    pub fn read_empty() -> Snippet {
        prog!(add_instr!(R Read, 0)) // Error
    }

    /// [POSITIVE] Pushes 3 values and reads them back using Read with indices 0, 1, and 2
    ///
    /// Expected final stack state: [10, 20, 30, 10, 20, 30]
    pub fn read_multiple() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            add_instr!(R Read, 0),
            add_instr!(R Read, 1),
            add_instr!(R Read, 2)
        )
    }

    /// [NEGATIVE] Pushes a value and tries to read from index 1
    pub fn read_bad_index() -> Snippet {
        prog!(
            add_instr!(Push, 42),
            add_instr!(R Read, 1) // Error
        )
    }

    // [NEGATIVE] Read with index larger than stack size after several pushes.
    pub fn read_far_beyond_stack() -> Snippet {
        prog!(
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(R Read, 100)
        )
    }

    /// [POSITIVE] Reads the top of stack
    ///
    /// Expected final stack state: [10, 10]
    pub fn read_reverse() -> Snippet {
        prog!(add_instr!(Push, 10), add_instr!(R ReadReverse, 0))
    }

    /// [POSITIVE] Reads top 3 values
    ///
    /// Expected final stack state: [10, 20, 30, 10, 20, 30]
    pub fn read_reverse_multiple() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2),
            add_instr!(R ReadReverse, 2)
        )
    }

    /// [NEGATIVE] Pushes a value and tries to read reverse from index 1
    pub fn read_reverse_bad_index() -> Snippet {
        prog!(
            add_instr!(Push, 42),
            add_instr!(R ReadReverse, 1) // Error
        )
    }

    /// [NEGATIVE] Tries to read reverse from an empty stack
    pub fn read_reverse_bad_empty() -> Snippet {
        prog!(add_instr!(R ReadReverse, 0)) // Error
    }
}

/// These programs ensure correctness of (some) arithmetic operations
pub mod arithmetic {
    use super::*;

    /// [POSITIVE] Tests bitwise not
    pub fn bitwise_not() -> Snippet {
        prog!(add_instr!(Push, 0b1100), add_instr!(R Not, 0))
    }

    /// [POSITIVE] Tests Nop
    pub fn nop() -> Snippet {
        prog!(add_instr!(Nop))
    }

    /// [NEGATIVE] Tests division by zero
    pub fn div_by_zero() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            add_instr!(Push, 0),
            add_instr!(Div, 0, 1) // Error
        )
    }
}

/// These programs ensure correctness of conditional `IfElse` instructions
pub mod conditional {
    use super::*;

    /// [POSITIVE] Tests a simple ifelse with a statically known true condition
    ///
    /// Expected final stack state: [10, 5, 1, 42]
    pub fn ifelse_known_true() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            add_instr!(Push, 5),
            add_instr!(SetGreaterThan, 0, 1), // 10 > 5 -> known true
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // taken
                add_instr!(Push, 0)   // not taken
            )
        )
    }

    /// [POSITIVE] Tests a simple ifelse with a statically known false condition
    ///
    /// Expected final stack state: [3, 5, 0, 0]
    pub fn ifelse_known_false() -> Snippet {
        prog!(
            add_instr!(Push, 3),
            add_instr!(Push, 5),
            add_instr!(SetGreaterThan, 0, 1), // 3 > 5 -> known false
            add_instr!(ifelse 2,
                add_instr!(Push, 42), // not taken
                add_instr!(Push, 0)   // taken
            )
        )
    }

    /// [POSITIVE] Tests an ifelse with an unknown condition but balanced branches
    ///
    /// Expected final stack state if input > 5: [x, 5, 1, 42]
    /// Expected final stack state if input <= 5: [x, 5, 0, 0]
    pub fn ifelse_unknown_balanced() -> Snippet {
        prog!(
            add_instr!(io Input, 0), // [?]
            add_instr!(Push, 5),
            add_instr!(SetGreaterThan, 0, 1), // unknown condition
            add_instr!(ifelse 2,
                add_instr!(Push, 42),         // +1 cell if taken
                add_instr!(Push, 0)           // +1 cell if not taken (balanced)
            )
        )
    }

    /// [POSITIVE] Statically-known condition: only the taken branch runs, and the
    /// verifier does not need to compare branch sizes (asymmetric branches are
    /// fine here because the untaken branch is dead code).
    ///
    /// Expected final stack state: [10, 3, 1, 42]
    pub fn ifelse_known_true_asymmetric() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            add_instr!(Push, 3),
            add_instr!(SetGreaterThan, 0, 1), // 10 > 3 -> known true
            add_instr!(ifelse 2,
                add_instr!(Push, 42),                 // taken: +1 cell
                add_instr!(R Pop, 2)                  // dead:  would be -2 cells
            )
        )
    }

    /// [POSITIVE] Statically-known false condition: only the false branch runs.
    ///
    /// Expected final stack state: [0]
    pub fn ifelse_known_false_asymmetric() -> Snippet {
        prog!(
            add_instr!(Push, 0),
            add_instr!(Push, 3),
            add_instr!(SetGreaterThan, 0, 1), // 3 > 10 -> known false
            add_instr!(ifelse 2,
                add_instr!(Push, 42),                  // dead
                add_instr!(R Pop, 2)                   // taken: +1 cell
            )
        )
    }

    /// [NEGATIVE] Tests an ifelse with an unknown condition and unbalanced branches
    ///
    /// Executor cannot detect this; Verifier should reject the program.
    ///
    /// Expected final stack state if input > 5: [x, 5, 1, 42]
    /// Expected final stack state if input <= 5: [x, 5]
    /// Expected verifier error: `UnbalancedBranches`
    pub fn ifelse_unknown_unbalanced() -> Snippet {
        prog!(
            add_instr!(io Input, 0), // [?]
            add_instr!(Push, 5),
            add_instr!(SetGreaterThan, 0, 1), // unknown condition
            add_instr!(ifelse 2,
                add_instr!(Push, 42),         // +1 cell if taken
                add_instr!(R Pop, 1)          // -1 cell if not taken (unbalanced)
            )
        )
    }

    /// [NEGATIVE] Condition is not the result of a comparison instruction.
    ///
    /// This program will eventually be used to check the type system.
    ///
    /// Expected: `UnsafeCondPlacement`.
    pub fn ifelse_bad_placement() -> Snippet {
        prog!(
            add_instr!(Push, 1), // Not a comparison
            add_instr!(ifelse 0,
                add_instr!(Push, 10),
                add_instr!(Push, 20)
            )
        )
    }

    /// [NEGATIVE] No condition on the stack at all when ifelse runs.
    /// Expected: `StackUnderflow`.
    pub fn ifelse_no_condition() -> Snippet {
        prog!(add_instr!(ifelse 0,
            add_instr!(Push, 1),
            add_instr!(Push, 2)
        ))
    }
}

/// These programs ensure correctness of block semantics.
pub mod blocks {
    use super::*;

    /// [NEGATIVE] Empty blocks are prohibited.
    pub fn empty_block() -> Snippet {
        prog!(make_block!())
    }

    /// [POSITIVE] A block with some instructions is fine.
    ///
    /// Expected final stack state: [42]
    pub fn block_simple() -> Snippet {
        prog!(make_block!(add_instr!(Push, 42)))
    }

    /// [POSITIVE] A block can return a value, which is the last push in the block.
    ///
    /// Expected final stack state: [10, 30]
    pub fn block_return_val() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            make_block!(add_instr!(Push, 20), add_instr!(Push, 30),),
        )
    }

    /// [POSITIVE] After block execution, stack state is restored and return value is on top.
    ///
    /// Expected final stack state: [10, 20, 30, 10]
    pub fn block_pops_only() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            make_block!(
                add_instr!(R Pop, 2) //
            )
        )
    }

    /// [POSITIVE] Blocks can be nested
    ///
    /// Expected final stack state: [10, 30]
    pub fn block_nested() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            make_block!(
                add_instr!(Push, 20),
                make_block!(
                    add_instr!(Push, 30) //
                )
            )
        )
    }

    /// [NEGATIVE] A block that pops more than it pushes should cause an error.
    pub fn block_stack_underflow() -> Snippet {
        prog!(make_block!(
            add_instr!(Push, 10),
            add_instr!(R Pop, 2) // Error: tries to pop 2 but only 1 on block stack
        ),)
    }

    /// [NEGATIVE] A block must return a value
    pub fn block_no_return_val() -> Snippet {
        prog!(
            add_instr!(Push, 10),
            make_block!(
                add_instr!(R Pop, 1) // Pops the only value, leaving block with no return value
            ),
        )
    }

    /// These programs ensure correctness of rebasing and frame management.
    pub mod rebasing {
        use super::*;

        /// [POSITIVE] A `Rebase` inside of a block resters index counting.
        ///
        /// Expected final stack state: [10, 40]
        pub fn rebase_simple() -> Snippet {
            prog!(
                add_instr!(Push, 10),
                make_block!(
                    add_instr!(Rebase),
                    add_instr!(Push, 20),
                    add_instr!(Add, 0, 0) // 20 + 20 = 40
                )
            )
        }

        /// [POSITIVE] A `Rebase` without previous pushes is still valid, just redundant.
        ///
        /// Expected final stack state: [40]
        pub fn rebase_redundant() -> Snippet {
            prog!(make_block!(
                add_instr!(Rebase),
                add_instr!(Push, 20),
                add_instr!(Add, 0, 0) // 20 + 20 = 40
            ))
        }

        /// [POSITIVE] A `Rebase` inside a nested block also works as expected.
        ///
        /// Expected final stack state: [10, 80]
        pub fn rebase_nested_1() -> Snippet {
            prog!(
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
                )
            )
        }

        /// [POSITIVE] A `Rebase` is not necessarily used everywhere, neither is its position fixed
        ///
        /// Expected final stack state: [10, 90]
        pub fn rebase_nested_2() -> Snippet {
            prog!(
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
                )
            )
        }

        /// [NEGATIVE] `Rebase` cannot be used without blocks
        pub fn rebase_no_block() -> Snippet {
            prog!(
                add_instr!(Push, 10),
                add_instr!(Rebase), // Error: no block frame to rebase to
                add_instr!(Push, 20),
                add_instr!(Add, 0, 0)
            )
        }

        /// [NEGATIVE] `Rebase` cannot be used twice in the same block
        pub fn rebase_twice() -> Snippet {
            prog!(
                add_instr!(Push, 10),
                make_block!(
                    add_instr!(Push, 20),
                    add_instr!(Rebase),
                    add_instr!(Rebase), // Error: already rebased in this block
                    add_instr!(Push, 30),
                )
            )
        }

        /// [NEGATIVE] `Pop` after `Rebase` is a stack underflow.
        pub fn rebase_after_pop() -> Snippet {
            prog!(
                add_instr!(Push, 10),
                make_block!(
                    add_instr!(Rebase),
                    add_instr!(R Pop, 1), // Error
                )
            )
        }

        /// [NEGATIVE] `Rebase` cannot be used in an `IfElse` branch without an inner block.
        ///
        /// Expected: `RebaseError`
        pub fn rebase_in_ifelse_branch() -> Snippet {
            prog!(
                add_instr!(Push, 10),
                add_instr!(Push, 5),
                add_instr!(SetGreaterThan, 0, 1),
                add_instr!(ifelse 2,
                    add_instr!(Rebase), // Error: no block frame in branch
                    add_instr!(Push, 0)
                )
            )
        }

        /// [POSITIVE] `Rebase` can be used in an `IfElse` branch, as long as it's inside a block.
        ///
        /// Expected final stack state: [10, 5, 1, 20]
        pub fn rebase_in_ifelse_block() -> Snippet {
            prog!(
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
                )
            )
        }
    }
}

/// These programs ensure correctness of function mechanics
pub mod functions {
    use super::*;

    /// [POSITIVE] A simple function that takes one argument, doubles it, and returns the result.
    ///
    /// Expected final stack state: [3, 9]
    pub fn simple() -> Snippet {
        prog!(
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(
                add_instr!(R ReadReverse, 0),
                add_instr!(Rebase),
                add_instr!(Mul, 0, 0) // Multiply input by 2
            ),
            add_instr!(Push, 3),
            add_instr!(fun FunctionCall, FUNC_NAME),
        )
    }

    /// [NEGATIVE] Function call with no argument provided.
    ///
    /// Expected: `StackUnderflow` when function tries to read argument.
    pub fn no_args() -> Snippet {
        prog!(
            add_instr!(fun FunctionDefine, FUNC_NAME),
            make_block!(
                add_instr!(R ReadReverse, 0),
                add_instr!(Rebase),
                add_instr!(Mul, 0, 0) // Multiply input by 2
            ),
            // Here, we forget to push an argument.
            add_instr!(fun FunctionCall, FUNC_NAME),
        )
    }

    /// [POSITIVE] Multiple sequential function definitions count as aliases
    ///
    /// Expected final stack state: [2, 2]
    pub fn sequential_defs() -> Snippet {
        prog!(
            add_instr!(fun FunctionDefine, "push2_1"),
            add_instr!(fun FunctionDefine, "push2_2"),
            add_instr!(fun FunctionDefine, "push2_3"),
            add_instr!(Push, 2),
            add_instr!(fun FunctionCall, "push2_1"),
            add_instr!(fun FunctionCall, "push2_2"),
        )
    }

    /// [NEGATIVE] Calling the function that's being defined is obvious infinite recursion.
    pub fn sequential_defs_loop() -> Snippet {
        prog!(
            add_instr!(fun FunctionDefine, "push2_1"),
            add_instr!(fun FunctionDefine, "push2_2"),
            add_instr!(fun FunctionDefine, "push2_3"),
            add_instr!(fun FunctionCall, "push2_1"),
            add_instr!(fun FunctionCall, "push2_1"),
        )
    }

    /// [NEGATIVE] Nested function definitions are prohibited
    ///
    /// TODO: Executor behavior for this program is still undecided
    pub fn nested_defs() -> Snippet {
        prog!(
            add_instr!(fun FunctionDefine, OUTER),
            make_block!(
                add_instr!(fun FunctionDefine, INNER),
                make_block!(add_instr!(Push, 42)),
                add_instr!(fun FunctionCall, INNER)
            ),
            add_instr!(fun FunctionCall, OUTER),
        )
    }

    /// [POSITIVE] A function that takes 3 arguments and adds them together.
    ///
    /// This is the standard way of providing arguments to a function and will easily work for any
    /// length of arguments, as well as as many repetitions of function calls in the main program
    /// body as desired.
    ///
    /// Expected final stack state: [10, 20, 30, 60]
    pub fn multi_args() -> Snippet {
        prog!(
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
        )
    }

    /// [NEGATIVE] A function with multiple arguments, but not all are provieded.
    pub fn multi_args_missing() -> Snippet {
        prog!(
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
        )
    }

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
    pub fn multi_args_alt() -> Snippet {
        prog!(
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
        )
    }

    /// [NEGATIVE] A function with alternative argument passing style and multiple arguments, but
    /// not all are provieded.
    pub fn multi_args_alt_missing() -> Snippet {
        prog!(
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
        )
    }
}

pub mod intrinsics {
    use super::*;

    /// [POSITIVE] A simple program that reads an input and prints it back out.
    pub fn input() -> Snippet {
        prog!(add_instr!(io Input, 0), add_instr!(io Print, 0))
    }

    /// [POSITIVE] A simple program that reads a string from a file and prints it back out.
    pub fn file_io_example() -> Snippet {
        prog!(
            add_instr!(io_str FileRead, "input.txt"),
            add_instr!(io Input, 0),
            add_instr!(io_str FileWrite, "output.txt"),
            add_instr!(io Print, 0),
            add_instr!(io_str FileRead, ""),
            add_instr!(io_str FileWrite, ""),
        )
    }

    // TODO: Write more intrinsics tests
    // - What if file does not exist? and other FileRead errors.
}
