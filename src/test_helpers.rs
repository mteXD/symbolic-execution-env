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

#[macro_export]
macro_rules! exec_expect {
    (($prog:expr) stack [ $($e:expr),* $(,)? ]) => {{
        let executor = $crate::machine::executor::Executor::new($prog)
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected: ::std::vec::Vec<$crate::types::Value> =
            ::std::vec![$($crate::types::Value::Integer($e)),*];
        assert_eq!(executor.values(), expected);
    }};
    (($prog:expr) tagged_stack with $policy:expr, [ $(($v:expr, $t:expr)),* $(,)? ]) => {{
        let executor = $crate::machine::executor::Executor::with_policy($prog, $policy)
            .expect("executor construction should succeed")
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        let expected_values = ::std::vec![$($crate::types::Value::Integer($v)),*];
        let expected_tags = ::std::vec![$($t),*];
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
            ::std::vec![$($crate::types::Value::Integer($e)),*];
        assert_eq!(executor.values(), expected);
    }};
    (($prog:expr) input [ $($in:expr),* $(,)? ] => output [ $($o:expr),* $(,)? ]) => {{
        $crate::exec_expect!(@input_output ($prog) [$($in),*] [$($o),*]);
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
            let executor = $crate::machine::executor::Executor::with_policy($prog, $policy)
                .expect("executor construction should succeed")
                .redirect_input($crate::types::IoBuffer::new(::std::vec![ $($in),* ]).into())
                .exec()
                .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
            let expected_values = ::std::vec![$($crate::types::Value::Integer($v)),*];
            let expected_tags = ::std::vec![$($t),*];
            assert_eq!(executor.values(), expected_values, "executor values mismatch");
            assert_eq!(executor.tags(), expected_tags, "executor tags mismatch");
        }
        $( $crate::exec_expect!(@tagged_cases ($prog) ($policy) $($rest)*); )?
    };
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
                ::std::vec![$($crate::types::Value::Integer($e)),*];
            assert_eq!(executor.values(), expected);
        }
        $( $crate::exec_expect!(@cases ($prog) $($rest)*); )?
    };
    (@cases ($prog:expr)
        input [ $($in:expr),* $(,)? ] => output [ $($o:expr),* $(,)? ]
        $(; $($rest:tt)*)?
    ) => {
        {
            $crate::exec_expect!(@input_output ($prog) [$($in),*] [$($o),*]);
        }
        $( $crate::exec_expect!(@cases ($prog) $($rest)*); )?
    };
    (@input_output ($prog:expr) [$($in:expr),*] [$($out:expr),*]) => {{
        let output = $crate::types::IoBuffer::new(::std::vec![]);
        $crate::machine::executor::Executor::new($prog)
            .redirect_input($crate::types::IoBuffer::new(::std::vec![$($in),*]).into())
            .redirect_output(output.clone().into())
            .exec()
            .unwrap_or_else(|error| panic!("executor should run program, but returned: {error:#?}"));
        assert_eq!(*output.borrow(), ::std::vec![$($out),*]);
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
