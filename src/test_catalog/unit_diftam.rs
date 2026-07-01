//! Unit tests with inline tagged (DIFTAM) programs.
//!
//! These exercise dynamic information-flow tracking and monitoring. Each test
//! inlines a tagged program and uses `custom` bodies, since they construct
//! runners via `with_policy` and assert on tags / construction errors. The
//! shared `Confidentiality` / `Integrity` policies live in the parent module.

use super::Confidentiality::*;
use super::Integrity::*;
use super::*;

// ---------------------------------------------------------------------------
// Tag propagation
// ---------------------------------------------------------------------------

test_program! {
    /// Confidentiality taint propagates through `Add`.
    confidentiality_ift,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(tag Push, 20, Secret),
        add_instr!(Add, 0, 1),
    ],
    verifier: { custom |program| {
        let verifier = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.read_tag(0).unwrap(), Public);
        assert_eq!(verifier.read_tag(1).unwrap(), Secret);
        assert_eq!(verifier.last_tag(), Some(Secret));
    } },
    executor: { custom |program| {
        let executor = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.values()[2], Cell::Integer(30));
        assert_eq!(executor.read_tag(0).unwrap(), Public);
        assert_eq!(executor.read_tag(1).unwrap(), Secret);
        assert_eq!(executor.last_tag(), Some(Secret));
    } },
}

test_program! {
    /// Integrity taint propagates through `Add`.
    integrity_ift,
    program: vec![
        add_instr!(tag Push, 10, Low),
        add_instr!(tag Push, 20, High),
        add_instr!(Add, 0, 1),
    ],
    verifier: { custom |program| {
        let verifier = Verifier::with_policy(program.clone(), integrity_policy())
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.last_tag(), Some(High));
    } },
    executor: { custom |program| {
        let executor = Executor::with_policy(program.clone(), integrity_policy())
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.last_tag(), Some(High));
    } },
}

test_program! {
    /// Taint flows through an intermediate confidentiality level.
    confidentiality_ift_through_intermediate_level,
    program: vec![
        add_instr!(Push, 1),
        add_instr!(tag Push, 2, Confidential),
        add_instr!(Add, 0, 1),
        add_instr!(tag Push, 3, Secret),
        add_instr!(Add, 2, 3),
    ],
    verifier: { custom |program| {
        let verifier = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .verify()
            .unwrap();
        // ccd(ccd(Public, Confidential), Secret) = Secret
        assert_eq!(verifier.last_tag(), Some(Secret));
    } },
    executor: { custom |program| {
        let executor = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.last_tag(), Some(Secret));
    } },
}

// ---------------------------------------------------------------------------
// Input tag
// ---------------------------------------------------------------------------

test_program! {
    /// `Input` receives the policy's configured input tag.
    input_receives_input_tag,
    program: vec![add_instr!(io Input, 0)],
    verifier: { custom |program| {
        let input = IoBuffer::new(vec![42]);
        let verifier = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_input(input.into())
            .verify()
            .unwrap();
        assert_eq!(verifier.last_tag(), Some(Secret));
    } },
    executor: { custom |program| {
        let input = IoBuffer::new(vec![42]);
        let executor = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_input(input.into())
            .exec()
            .unwrap();
        assert_eq!(executor.values()[0], Cell::Integer(42));
        assert_eq!(executor.last_tag(), Some(Secret));
    } },
}

// ---------------------------------------------------------------------------
// Control flow tainting
// ---------------------------------------------------------------------------

test_program! {
    /// A secret condition taints values pushed inside the taken branch.
    ifelse_condition_taints_branch_pushes,
    program: vec![
        add_instr!(tag Push, 1, Secret),
        add_instr!(ifelse 0,
            add_instr!(tag Push, 7, Public),
            add_instr!(tag Push, 9, Public)
        ),
        add_instr!(tag Push, 11, Public),
    ],
    verifier: { custom |program| {
        let verifier = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.read_tag(1).unwrap(), Secret);
        assert_eq!(verifier.read_tag(2).unwrap(), Public);
    } },
    executor: { custom |program| {
        let executor = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(
            executor.values(),
            vec![Cell::Integer(1), Cell::Integer(7), Cell::Integer(11)]
        );
        assert_eq!(executor.read_tag(1).unwrap(), Secret);
        assert_eq!(executor.read_tag(2).unwrap(), Public);
    } },
}

test_program! {
    /// An unknown condition causes both branches to be explored and their
    /// pushed tags merged. Verifier-only: the executor takes a single concrete
    /// branch and cannot observe the merge.
    ifelse_unknown_condition_merges_tags,
    program: vec![
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(And, 0, 1), // result is inf (unknown), tag = Public
        add_instr!(ifelse 2,
            add_instr!(tag Push, 7, Public),
            add_instr!(tag Push, 9, Confidential)
        ),
    ],
    verifier_only: { custom |program| {
        let verifier = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .verify()
            .unwrap();
        // Merge of Public and Confidential is Confidential.
        assert_eq!(verifier.last_tag(), Some(Confidential));
    } },
}

// ---------------------------------------------------------------------------
// Tag alignment through functions
// ---------------------------------------------------------------------------

test_program! {
    /// Tags stay aligned with values across function call, rebase and pop.
    tags_remain_aligned_through_function_rebase_and_pop,
    program: vec![
        add_instr!(fun FunctionDefine, "add_public"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(tag Push, 1, Public),
            add_instr!(Add, 0, 1)
        ),
        add_instr!(tag Push, 41, Secret),
        add_instr!(fun FunctionCall, "add_public"),
        add_instr!(Push, 99),
        add_instr!(R Pop, 1),
    ],
    verifier: { custom |program| {
        let verifier = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.tags().len(), verifier.values().len());
    } },
    executor: { custom |program| {
        let executor = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.values(), vec![Cell::Integer(41), Cell::Integer(42)]);
        // ccd(Secret, Public) = Secret, so both results carry Secret.
        assert_eq!(executor.tags(), vec![Secret, Secret]);
    } },
}

// ---------------------------------------------------------------------------
// Output perimeter guard
// ---------------------------------------------------------------------------

test_program! {
    /// The output perimeter guard rejects printing a secret value.
    output_pg_rejects_secret_tag,
    program: vec![add_instr!(tag Push, 42, Secret), add_instr!(io Print, 0)],
    verifier: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let result = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::InformationFlowViolation {
                found: Secret,
                guard: Public,
            }))
        ));
    } },
    executor: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let result = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::InformationFlowViolation {
                found: Secret,
                guard: Public,
            }))
        ));
        assert!(output.borrow().is_empty());
    } },
}

test_program! {
    /// A public value printed under secret control flow is still rejected,
    /// because the effective tag becomes secret.
    output_pg_rejects_public_value_under_secret_control,
    program: vec![
        add_instr!(tag Push, 1, Secret),
        add_instr!(tag Push, 42, Public),
        add_instr!(ifelse 0, add_instr!(io Print, 1), add_instr!(Nop)),
    ],
    verifier: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let result = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::InformationFlowViolation {
                found: Secret,
                guard: Public,
            }))
        ));
    } },
    executor: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let result = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::InformationFlowViolation {
                found: Secret,
                guard: Public,
            }))
        ));
        assert!(output.borrow().is_empty());
    } },
}

test_program! {
    /// A public value can be printed through the output perimeter.
    output_perimeter_accepts_public_value,
    program: vec![add_instr!(Push, 42), add_instr!(io Print, 0)],
    verifier: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let _verifier = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .verify()
            .unwrap();
    } },
    executor: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let _executor = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .exec()
            .unwrap();
        assert_eq!(*output.borrow(), vec![42]);
    } },
}

// ---------------------------------------------------------------------------
// Policy validation at construction
// ---------------------------------------------------------------------------

test_program! {
    /// A tag embedded in the program but absent from the policy graph is
    /// rejected at construction time.
    invalid_embedded_tag_rejected_at_construction,
    program: vec![add_instr!(tag Push, 42, Confidential)],
    verifier: { custom |program| {
        // A policy that only knows Public and Secret — no Confidential.
        let graph = Topology::linear([Public, Secret]).into_graph().unwrap();
        let limited_policy = SecurityPolicy::new(graph, Public, Secret, Public).unwrap();
        let result = Verifier::with_policy(program.clone(), limited_policy);
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::UnknownTag(Confidential)))
        ));
    } },
    executor: { custom |program| {
        let graph = Topology::linear([Public, Secret]).into_graph().unwrap();
        let limited_policy = SecurityPolicy::new(graph, Public, Secret, Public).unwrap();
        let result = Executor::with_policy(program.clone(), limited_policy);
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::UnknownTag(Confidential)))
        ));
    } },
}

// ---------------------------------------------------------------------------
// Verifier without IFT still works
// ---------------------------------------------------------------------------

test_program! {
    /// The plain (NoFlow) verifier still works: tags are all `()`.
    no_flow_verifier_works_as_before,
    program: vec![
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Add, 0, 1),
    ],
    verifier_only: { custom |program| {
        let verifier = Verifier::new(program.clone()).verify().unwrap();
        assert_eq!(verifier.tags(), vec![(), (), ()]);
        assert_eq!(verifier.last_tag(), Some(()));
    } },
}

// ---------------------------------------------------------------------------
// Aware flow & downgraders
//
// Reuses the `Confidentiality` lattice (Public -> Confidential -> Secret, output
// guard Public); `Secret` plays the role of "Private". Downgrader policies are
// built inline via `confidentiality_policy().with_downgrader(...)`.
// ---------------------------------------------------------------------------

test_program! {
    /// [#1/#2] Declaring a downgrader does not change oblivious flow: combining
    /// a Public and a Secret value still yields Secret (private + public = private).
    oblivious_flow_unaffected_by_downgrader,
    program: vec![
        add_instr!(Push, 1),
        add_instr!(tag Push, 2, Secret),
        add_instr!(Add, 0, 1),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let verifier = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.last_tag(), Some(Secret));
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let executor = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.values()[2], Cell::Integer(3));
        assert_eq!(executor.last_tag(), Some(Secret));
    } },
}

test_program! {
    /// [#3] Without a downgrader, a Secret-derived boolean cannot be printed:
    /// the equality result inherits Secret and the output guard rejects it.
    secret_bool_cannot_be_printed,
    program: vec![
        add_instr!(tag Push, 0, Secret),
        add_instr!(Push, 0),
        add_instr!(SetEqual, 0, 1),
        add_instr!(io Print, 2),
    ],
    verifier: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let result = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::InformationFlowViolation {
                found: Secret,
                guard: Public,
            }))
        ));
    } },
    executor: { custom |program| {
        let output = IoBuffer::new(vec![]);
        let result = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .redirect_output(output.clone().into())
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::InformationFlowViolation {
                found: Secret,
                guard: Public,
            }))
        ));
        assert!(output.borrow().is_empty());
    } },
}

test_program! {
    /// [#4] A downgrader exposes one approved derived value: whatever
    /// `is_empty(secret)` returns is *implicitly* retagged Public via
    /// `Secret ->> Public` and may cross the output guard. There is no explicit
    /// retag instruction: the body's last value (the `SetEqual` result, tagged
    /// Secret) is the return value and is forced to the connection target.
    downgrader_exposes_approved_result,
    program: vec![
        add_instr!(fun Downgrader, "is_empty"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 1)
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "is_empty"),
        add_instr!(io Print, 1),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let output = IoBuffer::new(vec![]);
        let verifier = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .redirect_output(output.clone().into())
            .verify()
            .unwrap();
        // The downgrader's result carries the connection target (Public).
        assert_eq!(verifier.read_tag(1).unwrap(), Public);
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let output = IoBuffer::new(vec![]);
        let executor = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .redirect_output(output.clone().into())
            .exec()
            .unwrap();
        assert_eq!(*output.borrow(), vec![1]);
        assert_eq!(executor.read_tag(1).unwrap(), Public);
    } },
}

test_program! {
    /// [#5] The per-data budget is enforced in both runners: downgrading the
    /// *same* cell twice (the result is popped between calls so the original
    /// secret is downgraded again) exceeds `max_calls = 1`.
    downgrader_per_data_budget_enforced,
    program: vec![
        add_instr!(fun Downgrader, "is_empty"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 1)
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "is_empty"),
        add_instr!(R Pop, 1),
        add_instr!(fun Downgrade, "is_empty"),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let result = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::DowngraderCallLimitExceeded {
                limit: 1,
                ..
            }))
        ));
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let result = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::DowngraderCallLimitExceeded {
                limit: 1,
                ..
            }))
        ));
    } },
}

test_program! {
    /// [#9] The implicit retag is strict: a `Confidential ->> Public` downgrader
    /// whose body returns a `Secret` value (not the connection `source`) is
    /// rejected by both runners. The verifier catches it at definition time;
    /// the executor when the body actually returns.
    downgrader_return_tag_must_match_source,
    program: vec![
        add_instr!(fun Downgrader, "leak"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(tag Push, 5, Secret)
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "leak"),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("leak", Confidential, Public, Some(1))
            .unwrap();
        let result = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::DowngraderReturnTagMismatch {
                found: Secret,
                expected: Confidential,
            }))
        ));
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("leak", Confidential, Public, Some(1))
            .unwrap();
        let result = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::DowngraderReturnTagMismatch {
                found: Secret,
                expected: Confidential,
            }))
        ));
    } },
}

test_program! {
    /// [#10] The budget is per data: two *distinct* secret cells are each
    /// downgraded once, so `max_calls = 1` is respected for both.
    downgrader_per_data_independent,
    program: vec![
        add_instr!(fun Downgrader, "is_empty"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 1)
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "is_empty"),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "is_empty"),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let verifier = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.read_tag(1).unwrap(), Public);
        assert_eq!(verifier.read_tag(3).unwrap(), Public);
        // Each source cell was downgraded exactly once.
        assert_eq!(verifier.counts()[0].get("is_empty"), 1);
        assert_eq!(verifier.counts()[2].get("is_empty"), 1);
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let executor = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.read_tag(1).unwrap(), Public);
        assert_eq!(executor.read_tag(3).unwrap(), Public);
    } },
}

test_program! {
    /// [#11] A cell's counter is discarded when it is popped: a fresh secret
    /// pushed into the freed slot starts from zero, so it can be downgraded
    /// again under `max_calls = 1`.
    downgrader_budget_resets_on_pop,
    program: vec![
        add_instr!(fun Downgrader, "is_empty"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 1)
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "is_empty"),
        add_instr!(R Pop, 2),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "is_empty"),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let verifier = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.read_tag(1).unwrap(), Public);
        assert_eq!(verifier.counts()[0].get("is_empty"), 1);
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let executor = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.read_tag(1).unwrap(), Public);
    } },
}

test_program! {
    /// [#12] An unlimited (`None`) budget lets the same cell be downgraded any
    /// number of times.
    downgrader_unlimited_budget,
    program: vec![
        add_instr!(fun Downgrader, "is_empty"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 1)
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "is_empty"),
        add_instr!(R Pop, 1),
        add_instr!(fun Downgrade, "is_empty"),
        add_instr!(R Pop, 1),
        add_instr!(fun Downgrade, "is_empty"),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, None)
            .unwrap();
        let verifier = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify()
            .unwrap();
        assert_eq!(verifier.read_tag(1).unwrap(), Public);
        // The single source cell was downgraded three times.
        assert_eq!(verifier.counts()[0].get("is_empty"), 3);
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, None)
            .unwrap();
        let executor = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.read_tag(1).unwrap(), Public);
    } },
}

test_program! {
    /// [#13] Downgraders are never re-entrant: a downgrader (`outer`) whose body
    /// calls another downgrader (`inner`) is rejected. The verifier catches it
    /// at `outer`'s definition; the executor when `outer` runs.
    downgrader_cannot_call_downgrader,
    program: vec![
        add_instr!(fun Downgrader, "inner"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 1)
        ),
        add_instr!(fun Downgrader, "outer"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "inner")
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun Downgrade, "outer"),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("inner", Secret, Public, Some(1))
            .unwrap()
            .with_downgrader("outer", Secret, Public, Some(1))
            .unwrap();
        let result = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::RecursiveDowngrader { .. }))
        ));
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("inner", Secret, Public, Some(1))
            .unwrap()
            .with_downgrader("outer", Secret, Public, Some(1))
            .unwrap();
        let result = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::RecursiveDowngrader { .. }))
        ));
    } },
}

test_program! {
    /// [#14] A registered downgrader may not be invoked with the ordinary
    /// `FunctionCall`: it must use `Downgrade`. Both runners reject the call as
    /// `DowngraderUsedAsFunction`.
    function_call_on_downgrader_rejected,
    program: vec![
        add_instr!(fun Downgrader, "is_empty"),
        make_block!(
            add_instr!(R ReadReverse, 0),
            add_instr!(Rebase),
            add_instr!(Push, 0),
            add_instr!(SetEqual, 0, 1)
        ),
        add_instr!(tag Push, 0, Secret),
        add_instr!(fun FunctionCall, "is_empty"),
    ],
    verifier: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let result = Verifier::with_policy(program.clone(), policy)
            .unwrap()
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::DowngraderUsedAsFunction { .. }))
        ));
    } },
    executor: { custom |program| {
        let policy = confidentiality_policy()
            .with_downgrader("is_empty", Secret, Public, Some(1))
            .unwrap();
        let result = Executor::with_policy(program.clone(), policy)
            .unwrap()
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::DowngraderUsedAsFunction { .. }))
        ));
    } },
}

test_program! {
    /// [#15] `Downgrade` may only name a downgrader registered in the policy.
    /// Pointing it at an ordinary function is rejected as `NotADowngrader`.
    downgrade_on_plain_function_rejected,
    program: vec![
        add_instr!(fun FunctionDefine, "helper"),
        make_block!(add_instr!(Push, 5)),
        add_instr!(fun Downgrade, "helper"),
    ],
    verifier: { custom |program| {
        let result = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::NotADowngrader { .. }))
        ));
    } },
    executor: { custom |program| {
        let result = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::NotADowngrader { .. }))
        ));
    } },
}

test_program! {
    /// [#16] `Downgrader` may only define a downgrader registered in the policy.
    /// Using it for an ordinary function is rejected at definition time as
    /// `NotADowngrader`.
    downgrader_define_on_plain_function_rejected,
    program: vec![
        add_instr!(fun Downgrader, "helper"),
        make_block!(add_instr!(Push, 5)),
    ],
    verifier: { custom |program| {
        let result = Verifier::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .verify();
        assert!(matches!(
            result,
            Err(VerifierError::Flow(FlowError::NotADowngrader { .. }))
        ));
    } },
    executor: { custom |program| {
        let result = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .exec();
        assert!(matches!(
            result,
            Err(ExecutorError::Flow(FlowError::NotADowngrader { .. }))
        ));
    } },
}
