use super::*;

use crate::{
    add_instr,
    information_flow::{FlowError, SecurityPolicy, Topology},
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{AluBinary, AluFunction, AluIntrinsic, AluNullary, AluUnaryCell, AluUnaryImm},
        IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    make_block,
    types::IoBuffer,
};

// ---------------------------------------------------------------------------
// Policy helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Confidentiality {
    Public,
    Confidential,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Integrity {
    Low,
    Medium,
    High,
}

fn confidentiality_policy() -> SecurityPolicy<Confidentiality> {
    use Confidentiality::*;

    let graph = Topology::linear([Public, Confidential, Secret])
        .into_graph()
        .unwrap();
    SecurityPolicy::new(graph, Public, Secret, Public).unwrap()
}

fn integrity_policy() -> SecurityPolicy<Integrity> {
    use Integrity::*;

    let graph = Topology::linear([Low, Medium, High]).into_graph().unwrap();
    SecurityPolicy::new(graph, Low, Low, High).unwrap()
}

// ---------------------------------------------------------------------------
// Tag propagation
// ---------------------------------------------------------------------------

#[test]
fn confidentiality_ift() {
    use Confidentiality::*;

    let program = vec![
        add_instr!(Push, 10),
        add_instr!(tag Push, 20, Secret),
        add_instr!(Add, 0, 1),
    ];
    let verifier = Verifier::with_policy(program, confidentiality_policy())
        .unwrap()
        .verify()
        .unwrap();

    assert_eq!(verifier.read_tag(0).unwrap(), Public);
    assert_eq!(verifier.read_tag(1).unwrap(), Secret);
    assert_eq!(verifier.last_tag(), Some(Secret));
}

#[test]
fn integrity_ift() {
    use Integrity::*;

    let program = vec![
        add_instr!(tag Push, 10, Low),
        add_instr!(tag Push, 20, High),
        add_instr!(Add, 0, 1),
    ];
    let verifier = Verifier::with_policy(program, integrity_policy())
        .unwrap()
        .verify()
        .unwrap();

    assert_eq!(verifier.last_tag(), Some(High));
}

#[test]
fn confidentiality_ift_through_intermediate_level() {
    use Confidentiality::*;

    let program = vec![
        add_instr!(Push, 1),
        add_instr!(tag Push, 2, Confidential),
        add_instr!(Add, 0, 1),
        add_instr!(tag Push, 3, Secret),
        add_instr!(Add, 2, 3),
    ];
    let verifier = Verifier::with_policy(program, confidentiality_policy())
        .unwrap()
        .verify()
        .unwrap();

    // ccd(ccd(Public, Confidential), Secret) = Secret
    assert_eq!(verifier.last_tag(), Some(Secret));
}

// ---------------------------------------------------------------------------
// Input tag
// ---------------------------------------------------------------------------

#[test]
fn input_receives_input_tag() {
    use Confidentiality::*;

    let input = IoBuffer::new(vec![42]);
    let program = vec![add_instr!(io Input, 0)];
    let verifier = Verifier::with_policy(program, confidentiality_policy())
        .unwrap()
        .redirect_input(input.into())
        .verify()
        .unwrap();

    // Input tag for confidentiality policy is Secret
    assert_eq!(verifier.last_tag(), Some(Secret));
}

// ---------------------------------------------------------------------------
// Control flow tainting
// ---------------------------------------------------------------------------

#[test]
fn ifelse_condition_taints_branch_pushes() {
    use Confidentiality::*;

    // Condition is Secret and statically true (nonzero), so only the true
    // branch runs. The push inside the branch should carry Secret because
    // the pc_tag is elevated by the condition's tag.
    let program = vec![
        add_instr!(tag Push, 1, Secret),
        add_instr!(ifelse 0,
            add_instr!(tag Push, 7, Public),
            add_instr!(tag Push, 9, Public)
        ),
        add_instr!(tag Push, 11, Public),
    ];
    let verifier = Verifier::with_policy(program, confidentiality_policy())
        .unwrap()
        .verify()
        .unwrap();

    // Value pushed inside Secret branch is tainted by the Secret condition
    assert_eq!(verifier.read_tag(1).unwrap(), Secret);
    // Value pushed after the branch restores pc_tag to Public
    assert_eq!(verifier.read_tag(2).unwrap(), Public);
}

#[test]
fn ifelse_unknown_condition_merges_tags() {
    use Confidentiality::*;

    // And produces ValueSpan::inf() (unbounded), so the condition is
    // unknown to the verifier and both branches are explored.
    let program = vec![
        add_instr!(Push, 1),
        add_instr!(Push, 2),
        add_instr!(And, 0, 1), // result is inf (unknown), tag = Public
        add_instr!(ifelse 2,
            add_instr!(tag Push, 7, Public),
            add_instr!(tag Push, 9, Confidential)
        ),
    ];
    let verifier = Verifier::with_policy(program, confidentiality_policy())
        .unwrap()
        .verify()
        .unwrap();

    // Both branches run. True branch pushes Public, false pushes
    // Confidential. The merge is ccd(Public, Confidential) = Confidential.
    // The condition tag is Public (from two Public operands), so pc_tag
    // inside branches is ccd(Public, Public) = Public — no extra taint.
    assert_eq!(verifier.last_tag(), Some(Confidential));
}

// ---------------------------------------------------------------------------
// Output perimeter guard
// ---------------------------------------------------------------------------

#[test]
fn output_pg_rejects_secret_tag() {
    use Confidentiality::*;

    let output = IoBuffer::new(vec![]);
    let program = vec![add_instr!(tag Push, 42, Secret), add_instr!(io Print, 0)];
    let result = Verifier::with_policy(program, confidentiality_policy())
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
}

#[test]
/// Output of a Public-tagged value is still rejected when it is printed under
/// a Secret control-flow context, because the effective tag becomes Secret.
fn output_pg_rejects_public_value_under_secret_control() {
    use Confidentiality::*;

    let output = IoBuffer::new(vec![]);
    let program = vec![
        add_instr!(tag Push, 1, Secret),
        add_instr!(tag Push, 42, Public),
        // Condition is Secret and statically true → pc_tag elevated to Secret
        add_instr!(ifelse 0, add_instr!(io Print, 1), add_instr!(Nop)),
    ];
    let result = Verifier::with_policy(program, confidentiality_policy())
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
}

#[test]
fn output_perimeter_accepts_public_value() {
    let output = IoBuffer::new(vec![]);
    let program = vec![add_instr!(Push, 42), add_instr!(io Print, 0)];
    let _verifier = Verifier::with_policy(program, confidentiality_policy())
        .unwrap()
        .redirect_output(output.clone().into())
        .verify()
        .unwrap();
}

// ---------------------------------------------------------------------------
// Policy validation at construction
// ---------------------------------------------------------------------------

#[test]
fn invalid_embedded_tag_rejected_at_construction() {
    use Confidentiality::*;

    // Build a policy that only knows Public and Secret — no Confidential.
    let graph = Topology::linear([Public, Secret]).into_graph().unwrap();
    let limited_policy = SecurityPolicy::new(graph, Public, Secret, Public).unwrap();

    // The program embeds Confidential, which is not in the limited policy graph.
    let program = vec![add_instr!(tag Push, 42, Confidential)];
    let result = Verifier::with_policy(program, limited_policy);

    assert!(matches!(
        result,
        Err(VerifierError::Flow(FlowError::UnknownTag(Confidential)))
    ));
}

// ---------------------------------------------------------------------------
// Tag alignment through functions
// ---------------------------------------------------------------------------

#[test]
fn tags_remain_aligned_through_function_rebase_and_pop() {
    use Confidentiality::*;

    let program = vec![
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
    ];
    let verifier = Verifier::with_policy(program, confidentiality_policy())
        .unwrap()
        .verify()
        .unwrap();

    // The function's return tag was computed during definition (with
    // default_tag for the unknown caller arg), so it's default_tag (Public)
    // combined with Public from the Add → Public.
    // At the call site, the return is pushed via push_existing (no double
    // pc_tag combine).
    assert_eq!(verifier.tags().len(), verifier.stack.cells.len());
}

// ---------------------------------------------------------------------------
// Verifier without IFT still works
// ---------------------------------------------------------------------------

#[test]
fn no_flow_verifier_works_as_before() {
    let program = vec![
        add_instr!(Push, 10),
        add_instr!(Push, 20),
        add_instr!(Add, 0, 1),
    ];
    let verifier = Verifier::new(program).verify().unwrap();

    // Tags are all () for NoFlow
    assert_eq!(verifier.tags(), &[(), (), ()]);
    assert_eq!(verifier.last_tag(), Some(()));
}
