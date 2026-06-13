use super::*;

use crate::{
    add_instr,
    information_flow::{FlowError, SecurityPolicy, Topology},
    instruction::{
        BinaryOp, FunctionOp,
        Instruction::{
            AluBinary, AluFunction, AluIntrinsic, AluNullary, AluUnaryCell, AluUnaryImm,
        },
        IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    make_block,
    types::{Cell, IoBuffer},
};
use Cell::Integer;

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
    let executor = Executor::with_policy(program, confidentiality_policy())
        .unwrap()
        .exec()
        .unwrap();

    assert_eq!(executor.cells[2], Integer(30));
    assert_eq!(executor.read_tag(0).unwrap(), Public);
    assert_eq!(executor.read_tag(1).unwrap(), Secret);
    assert_eq!(executor.last_tag(), Some(Secret));
}

#[test]
fn integrity_ift() {
    use Integrity::*;

    let program = vec![
        add_instr!(tag Push, 10, Low),
        add_instr!(tag Push, 20, High),
        add_instr!(Add, 0, 1),
    ];
    let executor = Executor::with_policy(program, integrity_policy())
        .unwrap()
        .exec()
        .unwrap();

    assert_eq!(executor.last_tag(), Some(High));
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
    let executor = Executor::with_policy(program, confidentiality_policy())
        .unwrap()
        .exec()
        .unwrap();

    // ccd(ccd(Public, Confidential), Secret) = Secret
    assert_eq!(executor.last_tag(), Some(Secret));
}

// ---------------------------------------------------------------------------
// Input tag
// ---------------------------------------------------------------------------

#[test]
fn input_receives_input_tag() {
    use Confidentiality::*;

    let input = IoBuffer::new(vec![42]);
    let program = vec![add_instr!(io Input, 0)];
    let executor = Executor::with_policy(program, confidentiality_policy())
        .unwrap()
        .redirect_input(input.into())
        .exec()
        .unwrap();

    assert_eq!(executor.cells[0], Integer(42));
    assert_eq!(executor.last_tag(), Some(Secret));
}

// ---------------------------------------------------------------------------
// Control flow tainting
// ---------------------------------------------------------------------------

#[test]
fn ifelse_condition_taints_branch_pushes() {
    use Confidentiality::*;

    let program = vec![
        add_instr!(tag Push, 1, Secret),
        add_instr!(ifelse 0,
            add_instr!(tag Push, 7, Public),
            add_instr!(tag Push, 9, Public)
        ),
        add_instr!(tag Push, 11, Public),
    ];
    let executor = Executor::with_policy(program, confidentiality_policy())
        .unwrap()
        .exec()
        .unwrap();

    assert_eq!(executor.cells, vec![Integer(1), Integer(7), Integer(11)]);
    // Value pushed inside Secret branch is tainted by the Secret condition
    assert_eq!(executor.read_tag(1).unwrap(), Secret);
    // Value pushed after the branch restores pc_tag to Public
    assert_eq!(executor.read_tag(2).unwrap(), Public);
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
    let executor = Executor::with_policy(program, confidentiality_policy())
        .unwrap()
        .exec()
        .unwrap();

    assert_eq!(executor.cells, vec![Integer(41), Integer(42)]);
    // ccd(Secret, Public) = Secret, so both results carry Secret
    assert_eq!(executor.tags(), &[Secret, Secret]);
}

// ---------------------------------------------------------------------------
// Output perimeter guard
// ---------------------------------------------------------------------------

#[test]
fn output_pg_rejects_secret_tag() {
    use Confidentiality::*;

    let output = IoBuffer::new(vec![]);
    let program = vec![add_instr!(tag Push, 42, Secret), add_instr!(io Print, 0)];
    let result = Executor::with_policy(program, confidentiality_policy())
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
}

#[test]
/// Output of a Public-tagged value is still rejected when it is printed under
/// a Secret control-flow context, because the effective tag becomes Secret.
fn output_pg_rejects_public_value_under_private_control() {
    use Confidentiality::*;

    let output = IoBuffer::new(vec![]);
    let program = vec![
        add_instr!(tag Push, 1, Secret),
        add_instr!(tag Push, 42, Public),
        add_instr!(ifelse 0, add_instr!(io Print, 1), add_instr!(Nop)),
    ];
    let result = Executor::with_policy(program, confidentiality_policy())
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
}

#[test]
fn output_perimeter_accepts_public_value() {
    let output = IoBuffer::new(vec![]);
    let program = vec![add_instr!(Push, 42), add_instr!(io Print, 0)];
    let executor = Executor::with_policy(program, confidentiality_policy())
        .unwrap()
        .redirect_output(output.clone().into())
        .exec()
        .unwrap();

    let _ = executor;
    assert_eq!(*output.borrow(), vec![42]);
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
    let result = Executor::with_policy(program, limited_policy);

    assert!(matches!(
        result,
        Err(ExecutorError::Flow(FlowError::UnknownTag(Confidential)))
    ));
}
