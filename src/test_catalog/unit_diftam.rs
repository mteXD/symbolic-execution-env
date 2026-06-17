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
        assert_eq!(executor.cells[2], Cell::Integer(30));
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
        assert_eq!(executor.cells[0], Cell::Integer(42));
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
            executor.cells,
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
        assert_eq!(verifier.tags().len(), verifier.stack.cells.len());
    } },
    executor: { custom |program| {
        let executor = Executor::with_policy(program.clone(), confidentiality_policy())
            .unwrap()
            .exec()
            .unwrap();
        assert_eq!(executor.cells, vec![Cell::Integer(41), Cell::Integer(42)]);
        // ccd(Secret, Public) = Secret, so both results carry Secret.
        assert_eq!(executor.tags(), &[Secret, Secret]);
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
        assert_eq!(verifier.tags(), &[(), (), ()]);
        assert_eq!(verifier.last_tag(), Some(()));
    } },
}
