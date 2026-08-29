#[macro_export]
macro_rules! verify_expect {
    (($prog:expr) stack [ $($e:expr),* $(,)? ]) => {{
        let verifier = $crate::machine::verifier::Verifier::new($prog)
            .verify()
            .unwrap_or_else(|error| panic!("verifier should accept program, but returned: {error:#?}"));
        let expected: ::std::vec::Vec<$crate::machine::verifier::ValueSpan> =
            ::std::vec![$($crate::machine::verifier::ValueSpan::from($e)),*];
        assert_eq!(verifier.values(), expected);
    }};
    (($prog:expr) tagged_stack with $policy:expr, [ $(($v:expr, $t:expr)),* $(,)? ]) => {{
        let verifier = $crate::machine::verifier::Verifier::with_policy($prog, $policy)
            .expect("verifier construction should succeed")
            .verify()
            .unwrap_or_else(|error| panic!("verifier should accept program, but returned: {error:#?}"));
        let expected_values = ::std::vec![$($crate::machine::verifier::ValueSpan::from($v)),*];
        let expected_tags = ::std::vec![$($t),*];
        assert_eq!(verifier.values(), expected_values, "verifier values mismatch");
        assert_eq!(verifier.tags(), expected_tags, "verifier tags mismatch");
    }};
    (($prog:expr) error with $policy:expr, $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::verifier::Verifier::with_policy($prog, $policy)
            .and_then(|verifier| verifier.verify());
        $crate::verify_expect!(@error_result verifier (result), $pat $(if $guard)?);
    }};
    (($prog:expr) error $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::verifier::Verifier::new($prog).verify();
        $crate::verify_expect!(@error_result verifier (result), $pat $(if $guard)?);
    }};
    (($prog:expr) custom |$program:ident| $body:block) => {{
        let $program = $prog;
        $body
    }};
    (@error_result $runner:ident ($result:expr), $pat:pat $(if $guard:expr)?) => {{
        match $result {
            Err($pat) $(if $guard)? => {}
            Err(other) => panic!(
                "\nEXPECTED {} Err({})\nACTUAL Err\n{:#?}",
                stringify!($runner),
                stringify!($pat),
                other
            ),
            Ok(_) => panic!(
                "\nEXPECTED {} Err({})\nACTUAL Ok(..)",
                stringify!($runner),
                stringify!($pat)
            ),
        }
    }};
}

#[macro_export]
macro_rules! exec_expect {
    (($prog:expr) stack [ $($e:expr),* $(,)? ]) => {
        $crate::exec_expect!(
            @stack ($crate::machine::executor::Executor::new($prog)),
            [$($e),*]
        );
    };
    (($prog:expr) tagged_stack with $policy:expr, [ $(($v:expr, $t:expr)),* $(,)? ]) => {
        $crate::exec_expect!(
            @tagged_stack (
                $crate::machine::executor::Executor::with_policy($prog, $policy)
                    .expect("executor construction should succeed")
            ),
            [$(($v, $t)),*]
        );
    };
    (($prog:expr) error with $policy:expr, $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::executor::Executor::with_policy($prog, $policy)
            .and_then(|executor| executor.exec());
        $crate::verify_expect!(@error_result executor (result), $pat $(if $guard)?);
    }};
    (($prog:expr) error $pat:pat $(if $guard:expr)?) => {{
        let result = $crate::machine::executor::Executor::new($prog).exec();
        $crate::verify_expect!(@error_result executor (result), $pat $(if $guard)?);
    }};
    (($prog:expr) custom |$program:ident| $body:block) => {{
        let $program = $prog;
        $body
    }};
    (($prog:expr) input [ $($in:expr),* $(,)? ] => stack [ $($e:expr),* $(,)? ]) => {
        $crate::exec_expect!(
            @stack (
                $crate::machine::executor::Executor::new($prog)
                    .redirect_input($crate::types::IoBuffer::new(::std::vec![$($in),*]).into())
            ),
            [$($e),*]
        );
    };
    (($prog:expr) input [ $($in:expr),* $(,)? ] => output [ $($o:expr),* $(,)? ]) => {{
        let output = $crate::types::IoBuffer::new(::std::vec![]);
        $crate::machine::executor::Executor::new($prog)
            .redirect_input($crate::types::IoBuffer::new(::std::vec![$($in),*]).into())
            .redirect_output(output.clone().into())
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        assert_eq!(*output.borrow(), ::std::vec![$($o),*]);
    }};
    (($prog:expr) cases { $($cases:tt)* }) => {
        $crate::exec_expect!(@cases ($prog) $($cases)*);
    };
    (($prog:expr) cases with $policy:expr, { $($cases:tt)* }) => {
        $crate::exec_expect!(@tagged_cases ($prog) ($policy) $($cases)*);
    };
    (@tagged_cases ($prog:expr) ($policy:expr)) => {};
    (@tagged_cases ($prog:expr) ($policy:expr)
        input [ $($in:expr),* $(,)? ] => tagged_stack [ $(($v:expr, $t:expr)),* $(,)? ]
        $(; $($rest:tt)*)?
    ) => {
        {
            $crate::exec_expect!(
                @tagged_stack (
                    $crate::machine::executor::Executor::with_policy($prog, $policy)
                        .expect("executor construction should succeed")
                        .redirect_input($crate::types::IoBuffer::new(::std::vec![$($in),*]).into())
                ),
                [$(($v, $t)),*]
            );
        }
        $( $crate::exec_expect!(@tagged_cases ($prog) ($policy) $($rest)*); )?
    };
    (@cases ($prog:expr)) => {};
    (@cases ($prog:expr)
        input [ $($in:expr),* $(,)? ] => stack [ $($e:expr),* $(,)? ]
        $(; $($rest:tt)*)?
    ) => {
        {
            $crate::exec_expect!(($prog) input [$($in),*] => stack [$($e),*]);
        }
        $( $crate::exec_expect!(@cases ($prog) $($rest)*); )?
    };
    (@cases ($prog:expr)
        input [ $($in:expr),* $(,)? ] => output [ $($o:expr),* $(,)? ]
        $(; $($rest:tt)*)?
    ) => {
        {
            $crate::exec_expect!(($prog) input [$($in),*] => output [$($o),*]);
        }
        $( $crate::exec_expect!(@cases ($prog) $($rest)*); )?
    };
    (@stack ($executor:expr), [$($e:expr),* $(,)?]) => {{
        let executor = ($executor)
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected: ::std::vec::Vec<$crate::types::Value> =
            ::std::vec![$($crate::types::Value::Integer($e)),*];
        assert_eq!(executor.values(), expected);
    }};
    (@tagged_stack ($executor:expr), [$(($v:expr, $t:expr)),* $(,)?]) => {{
        let executor = ($executor)
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected_values = ::std::vec![$($crate::types::Value::Integer($v)),*];
        let expected_tags = ::std::vec![$($t),*];
        assert_eq!(executor.values(), expected_values, "executor values mismatch");
        assert_eq!(executor.tags(), expected_tags, "executor tags mismatch");
    }};
}

#[macro_export]
macro_rules! test_program {
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

    (
        $(#[$meta:meta])*
        $name:ident,
        program: $prog:expr,
        split,
        verifier: { $($v:tt)* },
        executor: { $($e:tt)* } $(,)?
    ) => {
        $crate::test_program!(
            @gen [verifier => verify_expect] [$(#[$meta])*] $name ($prog) [] $($v)*
        );
        $crate::test_program!(
            @gen [executor => exec_expect] [$(#[$meta])*] $name ($prog) [] $($e)*
        );
    };

    (@gen [$suffix:ident => $expect:ident]
        [$($outer:tt)*] $name:ident ($prog:expr) [$($attrs:tt)*]
        #[$m:meta] $($rest:tt)*
    ) => {
        $crate::test_program!(
            @gen [$suffix => $expect]
            [$($outer)*] $name ($prog) [$($attrs)* #[$m]] $($rest)*
        );
    };
    (@gen [$suffix:ident => $expect:ident]
        [$($outer:tt)*] $name:ident ($prog:expr) [$($attrs:tt)*] $($body:tt)*
    ) => {
        ::paste::paste! {
            $($outer)*
            $($attrs)*
            #[test]
            fn [<$name _ $suffix>]() {
                $crate::$expect!(($prog) $($body)*);
            }
        }
    };

    (
        $(#[$meta:meta])*
        $name:ident,
        program: $prog:expr,
        verifier_only: { $($v:tt)* } $(,)?
    ) => {
        $crate::test_program!(
            @single [verify_expect] [$(#[$meta])*] $name ($prog) $($v)*
        );
    };

    (
        $(#[$meta:meta])*
        $name:ident,
        program: $prog:expr,
        executor_only: { $($e:tt)* } $(,)?
    ) => {
        $crate::test_program!(
            @single [exec_expect] [$(#[$meta])*] $name ($prog) $($e)*
        );
    };

    (@single [$expect:ident] [$($outer:tt)*] $name:ident ($prog:expr) $($body:tt)*) => {
        $($outer)*
        #[test]
        fn $name() {
            $crate::$expect!(($prog) $($body)*);
        }
    };
}
