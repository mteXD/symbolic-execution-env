//! Shared helpers for the declarative integration tests.
//!
//! Each VM program is written inline and paired with *both* its verifier and
//! executor expectations in one place via the [`test_program!`] macro, so
//! adding a program forces a statement of what both runners should do.

#![allow(dead_code, unused_imports, unused_macros)]

// Shared prelude, re-exported so the group submodules can `use super::*`.
pub(crate) use virtual_machine::{
    add_instr,
    information_flow::{FlowError, SecurityPolicy, Topology},
    instruction::Instruction,
    machine::{
        self, CoreError,
        executor::{Executor, ExecutorError},
        verifier::{ValueSpan, Verifier, VerifierError},
    },
    make_block,
    types::{self, FunctionDataError, IoBuffer, Value},
};

// ---------------------------------------------------------------------------
// Shared program-construction constants
// ---------------------------------------------------------------------------

/// Generic function name used by single-function programs.
pub(crate) const FUNC_NAME: &str = "generic_function_name";
/// Inner function name used by the nested-definition program.
pub(crate) const INNER: &str = "inner";
/// Outer function name used by the nested-definition program.
pub(crate) const OUTER: &str = "outer";

// ---------------------------------------------------------------------------
// Shared DIFTAM policy helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Confidentiality {
    Public,
    Confidential,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Integrity {
    Low,
    Medium,
    High,
}

pub(crate) fn confidentiality_policy() -> SecurityPolicy<Confidentiality> {
    use Confidentiality::*;

    let topology = Topology::linear([Public, Confidential, Secret]);
    SecurityPolicy::new(topology, Public, Secret, Public).unwrap()
}

pub(crate) fn integrity_policy() -> SecurityPolicy<Integrity> {
    use Integrity::*;

    let topology = Topology::linear([Low, Medium, High]);
    SecurityPolicy::new(topology, Low, Low, High).unwrap()
}

// ---------------------------------------------------------------------------
// Helper assertion functions
// ---------------------------------------------------------------------------

/// Asserts the verifier's final value stack equals `expected`.
pub(crate) fn check_verifier_stack(verifier: Verifier, expected: Vec<ValueSpan>) {
    assert_eq!(verifier.values(), expected);
}

/// Asserts the executor's final value stack equals `expected`.
pub(crate) fn check_executor_stack(executor: Executor, expected: Vec<Value>) {
    assert_eq!(executor.values(), expected);
}

// ---------------------------------------------------------------------------
// Expectation sub-grammar helper macros
// ---------------------------------------------------------------------------

/// Interprets a single verifier expectation against a program expression.
///
/// The `custom |program| { … }` form binds the program to a caller-provided
/// identifier so the block can reference it (sharing the caller's hygiene).
macro_rules! verify_expect {
    (($prog:expr) stack [ $($e:expr),* $(,)? ]) => {{
        let verifier = $crate::machine::verifier::Verifier::new($prog)
            .verify()
            .unwrap_or_else(|error| panic!("verifier should accept program, but returned: {error:#?}"));
        let expected: ::std::vec::Vec<$crate::machine::verifier::ValueSpan> =
            ::std::vec![ $( $crate::machine::verifier::ValueSpan::from($e) ),* ];
        $crate::check_verifier_stack(verifier, expected);
    }};
    (($prog:expr) tagged_stack with $policy:expr, [ $(($v:expr, $t:expr)),* $(,)? ]) => {{
        let verifier = $crate::machine::verifier::Verifier::with_policy($prog, $policy)
            .expect("verifier construction should succeed")
            .verify()
            .unwrap_or_else(|error| panic!("verifier should accept program, but returned: {error:#?}"));
        let expected_values: ::std::vec::Vec<$crate::machine::verifier::ValueSpan> =
            ::std::vec![ $( $crate::machine::verifier::ValueSpan::from($v) ),* ];
        let expected_tags = ::std::vec![ $( $t ),* ];
        assert_eq!(verifier.values(), expected_values, "verifier values mismatch");
        assert_eq!(verifier.tags(), expected_tags, "verifier tags mismatch");
    }};
    (($prog:expr) error with $policy:expr, $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::verifier::Verifier::with_policy($prog, $policy)
            .and_then(|verifier| verifier.verify());
        match result {
            Err($pat) $(if $guard)? => {}
            Err(other) => panic!(
                "\nEXPECTED verifier Err({})\nACTUAL Err\n{:#?}",
                stringify!($pat),
                other
            ),
            Ok(_) => panic!(
                "\nEXPECTED verifier Err({})\nACTUAL Ok(..)",
                stringify!($pat)
            ),
        }
    }};
    (($prog:expr) error $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::verifier::Verifier::new($prog).verify();
        match result {
            Err($pat) $(if $guard)? => {}
            other => panic!(
                "\nEXPECTED verifier Err({})\nACTUAL\n{:#?}",
                stringify!($pat),
                other
            ),
        }
    }};
    (($prog:expr) custom |$program:ident| $body:block) => {{
        let $program = $prog;
        $body
    }};
}

/// Interprets a single executor expectation against a program expression.
///
/// The `custom |program| { … }` form binds the program to a caller-provided
/// identifier so the block can reference it (sharing the caller's hygiene).
macro_rules! exec_expect {
    (($prog:expr) stack [ $($e:expr),* $(,)? ]) => {{
        let executor = $crate::machine::executor::Executor::new($prog)
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected: ::std::vec::Vec<$crate::types::Value> =
            ::std::vec![ $( $crate::types::Value::Integer($e) ),* ];
        $crate::check_executor_stack(executor, expected);
    }};
    (($prog:expr) tagged_stack with $policy:expr, [ $(($v:expr, $t:expr)),* $(,)? ]) => {{
        let executor = $crate::machine::executor::Executor::with_policy($prog, $policy)
            .expect("executor construction should succeed")
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected_values: ::std::vec::Vec<$crate::types::Value> =
            ::std::vec![ $( $crate::types::Value::Integer($v) ),* ];
        let expected_tags = ::std::vec![ $( $t ),* ];
        assert_eq!(executor.values(), expected_values, "executor values mismatch");
        assert_eq!(executor.tags(), expected_tags, "executor tags mismatch");
    }};
    (($prog:expr) error with $policy:expr, $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::executor::Executor::with_policy($prog, $policy)
            .and_then(|executor| executor.exec());
        match result {
            Err($pat) $(if $guard)? => {}
            Err(other) => panic!(
                "\nEXPECTED executor Err({})\nACTUAL Err\n{:#?}",
                stringify!($pat),
                other
            ),
            Ok(_) => panic!(
                "\nEXPECTED executor Err({})\nACTUAL Ok(..)",
                stringify!($pat)
            ),
        }
    }};
    (($prog:expr) error $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::executor::Executor::new($prog).exec();
        match result {
            Err($pat) $(if $guard)? => {}
            other => panic!(
                "\nEXPECTED executor Err({})\nACTUAL\n{:#?}",
                stringify!($pat),
                other
            ),
        }
    }};
    (($prog:expr) custom |$program:ident| $body:block) => {{
        let $program = $prog;
        $body
    }};
    (($prog:expr) input [ $($in:expr),* $(,)? ] => stack [ $($e:expr),* $(,)? ]) => {{
        let executor = $crate::machine::executor::Executor::new($prog)
            .redirect_input($crate::types::IoBuffer::new(::std::vec![ $($in),* ]).into())
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected: ::std::vec::Vec<$crate::types::Value> =
            ::std::vec![ $( $crate::types::Value::Integer($e) ),* ];
        $crate::check_executor_stack(executor, expected);
    }};
    (($prog:expr) input [ $($in:expr),* $(,)? ] => output [ $($o:expr),* $(,)? ]) => {{
        let out_buf = $crate::types::IoBuffer::new(::std::vec![]);
        let _ = $crate::machine::executor::Executor::new($prog)
            .redirect_input($crate::types::IoBuffer::new(::std::vec![ $($in),* ]).into())
            .redirect_output(out_buf.clone().into())
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected: ::std::vec::Vec<$crate::types::Immediate> = ::std::vec![ $($o),* ];
        assert_eq!(*out_buf.borrow(), expected);
    }};
    (($prog:expr) cases { $($cases:tt)* }) => {
        $crate::exec_expect!(@cases ($prog) $($cases)*);
    };
    (($prog:expr) cases with $policy:expr, { $($cases:tt)* }) => {
        $crate::exec_expect!(@tagged_cases ($prog) ($policy) $($cases)*);
    };
    // Internal `tagged_cases` muncher: each case is `input [..] => tagged_stack [..]`,
    // separated/terminated by `;`. The program and policy are re-evaluated for each case.
    (@tagged_cases ($prog:expr) ($policy:expr)) => {};
    (@tagged_cases ($prog:expr) ($policy:expr)
        input [ $($in:expr),* $(,)? ] => tagged_stack [ $(($v:expr, $t:expr)),* $(,)? ]
        $(; $($rest:tt)*)?
    ) => {
        {
            let executor = $crate::machine::executor::Executor::with_policy($prog, $policy)
                .expect("executor construction should succeed")
                .redirect_input($crate::types::IoBuffer::new(::std::vec![ $($in),* ]).into())
                .exec()
                .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
            let expected_values: ::std::vec::Vec<$crate::types::Value> =
                ::std::vec![ $( $crate::types::Value::Integer($v) ),* ];
            let expected_tags = ::std::vec![ $( $t ),* ];
            assert_eq!(executor.values(), expected_values, "executor values mismatch");
            assert_eq!(executor.tags(), expected_tags, "executor tags mismatch");
        }
        $( $crate::exec_expect!(@tagged_cases ($prog) ($policy) $($rest)*); )?
    };
    // Internal `cases` muncher: each case is `input [..] => stack [..]` or
    // `input [..] => output [..]`, separated/terminated by `;`. The program
    // expression is re-evaluated for each case.
    (@cases ($prog:expr)) => {};
    (@cases ($prog:expr)
        input [ $($in:expr),* $(,)? ] => stack [ $($e:expr),* $(,)? ]
        $(; $($rest:tt)*)?
    ) => {
        {
            let executor = $crate::machine::executor::Executor::new($prog)
                .redirect_input($crate::types::IoBuffer::new(::std::vec![ $($in),* ]).into())
                .exec()
                .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
            let expected: ::std::vec::Vec<$crate::types::Value> =
                ::std::vec![ $( $crate::types::Value::Integer($e) ),* ];
            $crate::check_executor_stack(executor, expected);
        }
        $( $crate::exec_expect!(@cases ($prog) $($rest)*); )?
    };
    (@cases ($prog:expr)
        input [ $($in:expr),* $(,)? ] => output [ $($o:expr),* $(,)? ]
        $(; $($rest:tt)*)?
    ) => {
        {
            let out_buf = $crate::types::IoBuffer::new(::std::vec![]);
            let _ = $crate::machine::executor::Executor::new($prog)
                .redirect_input($crate::types::IoBuffer::new(::std::vec![ $($in),* ]).into())
                .redirect_output(out_buf.clone().into())
                .exec()
                .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
            let expected: ::std::vec::Vec<$crate::types::Immediate> = ::std::vec![ $($o),* ];
            assert_eq!(*out_buf.borrow(), expected);
        }
        $( $crate::exec_expect!(@cases ($prog) $($rest)*); )?
    };
}

// ---------------------------------------------------------------------------
// The `test_program!` macro
// ---------------------------------------------------------------------------

/// Declares a program together with both its verifier and executor
/// expectations. See the module docs for the supported forms.
macro_rules! test_program {
    // ---- Combined: one #[test] running both assertions ----
    (
        $(#[$meta:meta])*
        $name:ident,
        program: $prog:expr,
        verifier: { $($v:tt)* },
        executor: { $($e:tt)* } $(,)?
    ) => {
        $(#[$meta])*
        #[test]
        fn $name() {
            $crate::verify_expect!(($prog) $($v)*);
            $crate::exec_expect!(($prog) $($e)*);
        }
    };

    // ---- Split: two tests, each with its own attributes ----
    //
    // Each side's body may begin with its own attributes (e.g. `#[ignore]`).
    // The `@gen_v` / `@gen_e` munchers peel those leading attributes off the
    // expectation tokens and forward them onto the generated test function.
    (
        $(#[$meta:meta])*
        $name:ident,
        program: $prog:expr,
        split,
        verifier: { $($v:tt)* },
        executor: { $($e:tt)* } $(,)?
    ) => {
        $crate::test_program!(@gen_v [$(#[$meta])*] $name ($prog) [] $($v)*);
        $crate::test_program!(@gen_e [$(#[$meta])*] $name ($prog) [] $($e)*);
    };

    (@gen_v [$($outer:tt)*] $name:ident ($prog:expr) [$($attrs:tt)*] #[$m:meta] $($rest:tt)*) => {
        $crate::test_program!(
            @gen_v [$($outer)*] $name ($prog) [$($attrs)* #[$m]] $($rest)*
        );
    };
    (@gen_v [$($outer:tt)*] $name:ident ($prog:expr) [$($attrs:tt)*] $($body:tt)*) => {
        ::paste::paste! {
            $($outer)*
            $($attrs)*
            #[test]
            fn [<$name _verifier>]() {
                $crate::verify_expect!(($prog) $($body)*);
            }
        }
    };

    (@gen_e [$($outer:tt)*] $name:ident ($prog:expr) [$($attrs:tt)*] #[$m:meta] $($rest:tt)*) => {
        $crate::test_program!(
            @gen_e [$($outer)*] $name ($prog) [$($attrs)* #[$m]] $($rest)*
        );
    };
    (@gen_e [$($outer:tt)*] $name:ident ($prog:expr) [$($attrs:tt)*] $($body:tt)*) => {
        ::paste::paste! {
            $($outer)*
            $($attrs)*
            #[test]
            fn [<$name _executor>]() {
                $crate::exec_expect!(($prog) $($body)*);
            }
        }
    };

    // ---- Verifier-only ----
    (
        $(#[$meta:meta])*
        $name:ident,
        program: $prog:expr,
        verifier_only: { $($v:tt)* } $(,)?
    ) => {
        $(#[$meta])*
        #[test]
        fn $name() {
            $crate::verify_expect!(($prog) $($v)*);
        }
    };

    // ---- Executor-only ----
    (
        $(#[$meta:meta])*
        $name:ident,
        program: $prog:expr,
        executor_only: { $($e:tt)* } $(,)?
    ) => {
        $(#[$meta])*
        #[test]
        fn $name() {
            $crate::exec_expect!(($prog) $($e)*);
        }
    };
}

pub(crate) use exec_expect;
pub(crate) use test_program;
pub(crate) use verify_expect;
