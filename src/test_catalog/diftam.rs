//! Unit tests with inline tagged (DIFTAM) programs.
//!
//! These exercise dynamic information-flow tracking and monitoring. Each test
//! inlines a tagged program and states both runners' expectations with the
//! declarative `tagged_stack with <policy>, …` / `error with <policy>, …`
//! forms. A `custom` body remains only where the grammar cannot reach (the
//! plain unmonitored runner). The shared `Confidentiality` / `Integrity`
//! policies live in the parent module.

use super::Confidentiality::*;
use super::Integrity::*;
use super::*;

// ---------------------------------------------------------------------------
// Policy helpers
// ---------------------------------------------------------------------------

/// Confidentiality policy extended with a single `Secret ->> Public`
/// downgrader named `name`, with a total call limit of `max_calls` for the
/// whole program run.
fn downgrader_policy(name: &str, max_calls: Option<usize>) -> SecurityPolicy<Confidentiality> {
    confidentiality_policy()
        .with_downgrader(name, Secret, Public, max_calls)
        .unwrap()
}

/// A policy that only knows `Public` and `Secret` — no `Confidential`.
fn limited_policy() -> SecurityPolicy<Confidentiality> {
    let topology = Topology::linear([Public, Secret]);
    SecurityPolicy::new(topology, Public, Secret, Public).unwrap()
}

/// Like [`confidentiality_policy`], but with a `Public` input perimeter guard:
/// `Input` yields an unknown value without tainting it.
fn public_input_policy() -> SecurityPolicy<Confidentiality> {
    let topology = Topology::linear([Public, Confidential, Secret]);
    SecurityPolicy::new(topology, Public, Public, Public).unwrap()
}

// ---------------------------------------------------------------------------
// Tag propagation
// ---------------------------------------------------------------------------

mod tag_propagation {
    use super::*;
    use crate::information_flow::DisjointTag;

    /// A policy that combines the `Confidentiality` and `Integrity` lattices
    /// as a disjoint union, so `Left(...)` and `Right(...)` tags never share a
    /// common descendant.
    fn disjoint_policy() -> SecurityPolicy<DisjointTag<Confidentiality, Integrity>> {
        use DisjointTag::Right;

        let confidentiality = Topology::linear([Public, Secret]);
        let integrity = Topology::linear([Low, High]);
        let topology = Topology::<DisjointTag<Confidentiality, Integrity>>::disjoint_union(
            confidentiality,
            integrity,
        );
        SecurityPolicy::new(topology, Right(Low), Right(Low), Right(High)).unwrap()
    }

    test_program! {
        /// Tag propagation with one default tag and one explicit tag: the `Add`
        /// result inherits the more restrictive tag.
        confidentiality_ift,
        program: vec![
            add_instr!(Push, 10),
            add_instr!(tag Push, 20, Secret),
            add_instr!(Add, 0, 1),
        ],
        verifier: { tagged_stack with confidentiality_policy(), [
                (10, Public),
                (20, Secret),
                (30, Secret)
            ]
        },
        executor: { tagged_stack with confidentiality_policy(), [
                (10, Public),
                (20, Secret),
                (30, Secret)
            ]
        },
    }

    test_program! {
        /// Tag propagation with two explicit tags: the `Add` result inherits the
        /// more restrictive tag.
        integrity_ift,
        program: vec![
            add_instr!(tag Push, 10, Low),
            add_instr!(tag Push, 20, High),
            add_instr!(Add, 0, 1),
        ],
        verifier: { tagged_stack with integrity_policy(), [
                (10, Low),
                (20, High),
                (30, High)
            ]
        },
        executor: { tagged_stack with integrity_policy(), [
                (10, Low),
                (20, High),
                (30, High)
            ]
        },
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
        verifier: { tagged_stack with confidentiality_policy(), [
                (1, Public),
                (2, Confidential),
                (3, Confidential),
                (3, Secret),
                (6, Secret)
            ]
        },
        executor: { tagged_stack with confidentiality_policy(), [
                (1, Public),
                (2, Confidential),
                (3, Confidential),
                (3, Secret),
                (6, Secret)
            ]
        },
    }

    test_program! {
        /// A `Left(Confidentiality)` value cannot be joined with a `Right(Integrity)`
        /// program-counter tag: there is no common descendant.
        no_ccd,
        program: vec![
            add_instr!(tag Push, 42, DisjointTag::Left(Public)),
        ],
        verifier: { error with disjoint_policy(),
            VerifierError::Flow(FlowError::NoCommonDescendant { .. })
        },
        executor: { error with disjoint_policy(),
            ExecutorError::Flow(FlowError::NoCommonDescendant { .. })
        },
    }
}

/// These tests check perimeter guards functionality.
mod pgs {
    use super::*;

    test_program! {
        /// `Input` receives the policy's configured input perimeter guard's tag.
        input_receives_input_tag,
        program: vec![
            add_instr!(Input)
        ],
        verifier: { tagged_stack with confidentiality_policy(), [
                (ValueSpan::inf(), Secret)
            ]
        },
        executor: { cases with confidentiality_policy(), {
                input [42] => tagged_stack [
                    (42, Secret)
                ]
            }
        },
    }

    test_program! {
        /// The output perimeter guard rejects printing a secret value; nothing is
        /// written before the rejection.
        output_rejected,
        program: vec![add_instr!(tag Push, 42, Secret), add_instr!(R Print, 0)],
        verifier: { error with confidentiality_policy(),
            VerifierError::Flow(FlowError::PGViolation {
                found: Secret,
                guard: Public,
            })
        },
        executor: { error with confidentiality_policy(),
            ExecutorError::Flow(FlowError::PGViolation {
                found: Secret,
                guard: Public,
            })
        },
    }

    test_program! {
        /// A public value printed under secret control flow is still rejected,
        /// because the effective tag becomes secret.
        output_rejected_implicit,
        program: vec![
            add_instr!(tag Push, 1, Secret),
            add_instr!(tag Push, 42, Public),
            add_instr!(ifelse 0,
                add_instr!(R Print, 1),
                add_instr!(Nop)
            ),
        ],
        verifier: { error with confidentiality_policy(),
            VerifierError::Flow(FlowError::PGViolation {
                found: Secret,
                guard: Public,
            })
        },
        executor: { error with confidentiality_policy(),
            ExecutorError::Flow(FlowError::PGViolation {
                found: Secret,
                guard: Public,
            })
        },
    }

    test_program! {
        /// A public value can be printed through the output perimeter.
        output_accepted,
        program: vec![
            add_instr!(Push, 42),
            add_instr!(R Print, 0)
        ],
        verifier: { tagged_stack with confidentiality_policy(), [
            (42, Public)
        ] },
        executor: { tagged_stack with confidentiality_policy(), [
            (42, Public)
        ] },
    }
}

mod implicit_flow {
    use super::*;

    test_program! {
        /// A secret condition taints values pushed inside the taken branch.
        ifelse_known,
        program: vec![
            add_instr!(tag Push, 1, Secret),
            add_instr!(ifelse 0,
                add_instr!(tag Push, 7, Public),
                add_instr!(tag Push, 9, Public)
            ),
            add_instr!(tag Push, 11, Public),
        ],
        verifier: { tagged_stack with confidentiality_policy(), [
                (1, Secret),
                (7, Secret),
                (11, Public)
            ]
        },
        executor: { tagged_stack with confidentiality_policy(), [
                (1, Secret),
                (7, Secret),
                (11, Public)
            ]
        },
    }

    test_program! {
        /// An unknown condition causes both branches to be explored and their
        /// pushed tags merged. Verifier-only: the executor takes a single concrete
        /// branch and cannot observe the merge.
        ifelse_unknown,
        program: vec![
            add_instr!(Input),
            add_instr!(ifelse 0,
                add_instr!(tag Push, 7, Public),
                add_instr!(tag Push, 9, Confidential)
            ),
        ],
        verifier_only: { tagged_stack with public_input_policy(), [
                (ValueSpan::inf(), Public),
                (ValueSpan::new(7, 9), Confidential)
            ]
        },
    }

    test_program! {
        /// A secret condition taints every newly-created value in a selected
        /// multi-instruction branch, not only the branch's first instruction.
        ifelse_sequence_taints_every_instruction,
        program: vec![
            add_instr!(tag Push, 1, Secret),
            add_instr!(ifelse 0,
                [
                    add_instr!(tag Push, 7, Public),
                    add_instr!(tag Push, 8, Public),
                ],
                [],
            ),
            add_instr!(tag Push, 9, Public),
        ],
        verifier: { tagged_stack with confidentiality_policy(), [
                (1, Secret),
                (7, Secret),
                (8, Secret),
                (9, Public)
            ]
        },
        executor: { tagged_stack with confidentiality_policy(), [
                (1, Secret),
                (7, Secret),
                (8, Secret),
                (9, Public)
            ]
        },
    }
}

test_program! {
    /// Tags stay aligned with values across an isolated function call and pop.
    tags_remain_aligned_through_function_block,
    program: vec![
        add_instr!(fun FunctionDefine, "add_public"),
        make_block!(1,
            add_instr!(tag Push, 1, Public),
            add_instr!(Add, 0, 1)
        ),
        add_instr!(tag Push, 41, Secret),
        add_instr!(fun FunctionCall, "add_public"),
    ],
    verifier: { tagged_stack with confidentiality_policy(), [
            (41, Secret),
            (42, Secret),
        ]
    },
    executor: { tagged_stack with confidentiality_policy(), [
            (41, Secret),
            (42, Secret)
        ]
    },
}

test_program! {
    /// A Secret argument that the function body never reads does not taint its
    /// Public return. The verifier interprets the global call with the actual
    /// argument cells and therefore matches the executor.
    tags_remain_aligned_through_function_block_unused_arg,
    program: vec![
        add_instr!(fun FunctionDefine, "add_public"),
        make_block!(1,
            add_instr!(tag Push, 1, Public),
        ),
        add_instr!(tag Push, 41, Secret),
        add_instr!(fun FunctionCall, "add_public"),
    ],
    verifier: { tagged_stack with confidentiality_policy(), [
            (41, Secret),
            (1, Public),
        ]
    },
    executor: { tagged_stack with confidentiality_policy(), [
            (41, Secret),
            (1, Public)
        ]
    },
}

// ---------------------------------------------------------------------------
// Policy validation at construction
// ---------------------------------------------------------------------------

test_program! {
    /// A tag embedded in the program but absent from the policy graph is
    /// rejected at construction time.
    invalid_embedded_tag_rejected_at_construction,
    program: vec![add_instr!(tag Push, 42, Confidential)],
    verifier: { error with limited_policy(),
        VerifierError::Flow(FlowError::UnknownTag(Confidential))
    },
    executor: { error with limited_policy(),
        ExecutorError::Flow(FlowError::UnknownTag(Confidential))
    },
}

test_program! {
    /// Tag validation traverses every instruction in an ifelse branch sequence,
    /// including instructions after an otherwise valid first element.
    invalid_embedded_tag_later_in_ifelse_sequence,
    program: vec![
        add_instr!(Push, 1),
        add_instr!(ifelse 0,
            [
                add_instr!(tag Push, 1, Public),
                add_instr!(tag Push, 42, Confidential),
            ],
            [],
        ),
    ],
    verifier: { error with limited_policy(),
        VerifierError::Flow(FlowError::UnknownTag(Confidential))
    },
    executor: { error with limited_policy(),
        ExecutorError::Flow(FlowError::UnknownTag(Confidential))
    },
}

// ---------------------------------------------------------------------------
// Aware flow & downgraders
//
// Reuses the `Confidentiality` lattice (Public -> Confidential -> Secret, output
// guard Public); `Secret` plays the role of "Private". Downgrader policies come
// from the `downgrader_policy` helper (`Secret ->> Public`), or inline
// `with_downgrader` calls where the connection differs.
//
// Call limit semantics: `max_calls` is the downgrader's TOTAL
// number of allowed calls per program run, independent of which values are
// downgraded and never refunded by pops. (Previously the call limit was counted
// per stack cell and reset when the cell was popped.)
// ---------------------------------------------------------------------------

mod downgraders {
    use super::*;

    test_program! {
        /// Declaring a downgrader does not change oblivious flow (usual behavior)
        oblivious_flow_unaffected,
        program: vec![
            add_instr!(Push, 1),
            add_instr!(tag Push, 2, Secret),
            add_instr!(Add, 0, 1),
        ],
        verifier: { tagged_stack with downgrader_policy("is_empty", Some(1)), [
                (1, Public),
                (2, Secret),
                (3, Secret)
            ]
        },
        executor: { tagged_stack with downgrader_policy("is_empty", Some(1)), [
                (1, Public),
                (2, Secret),
                (3, Secret)
            ]
        },
    }

    test_program! {
        /// Tests intended downgrader behavior.
        /// Downgrader reads a secret value and transforms it into a public result.
        basic,
        program: vec![
            add_instr!(fun Downgrader, "is_zero"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "is_zero"),
            add_instr!(R Print, 1),
        ],
        // The downgrader's result carries the connection target (Public).
        verifier: { tagged_stack with downgrader_policy("is_zero", Some(1)), [
                (0, Secret),
                (1, Public)
            ]
        },
        executor: { tagged_stack with downgrader_policy("is_zero", Some(1)), [
                (0, Secret),
                (1, Public)
            ]
        },
    }

    test_program! {
        /// Implicit retagging is not allowed; even if A->B->C, a downgrader defined with C->A is
        /// rejected if called on B; where A, B, C are tags.
        no_implicit_retag,
        program: vec![
            add_instr!(fun Downgrader, "leak"),
            make_block!(1,
                add_instr!(tag Push, 5, Secret)
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "leak"),
        ],
        verifier: { error with confidentiality_policy()
                .with_downgrader("leak", Confidential, Public, Some(1))
                .unwrap(),
            VerifierError::Flow(FlowError::DowngraderReturnTagMismatch {
                found: Secret,
                expected: Confidential,
            })
        },
        executor: { error with confidentiality_policy()
                .with_downgrader("leak", Confidential, Public, Some(1))
                .unwrap(),
            ExecutorError::Flow(FlowError::DowngraderReturnTagMismatch {
                found: Secret,
                expected: Confidential,
            })
        },
    }

    test_program! {
        /// Passing a Public-tagged value to a Secret->Public downgrader is rejected:
        /// the body returns the argument unchanged (Public), but the connection
        /// source is Secret.
        wrong_argument_tag,
        program: vec![
            add_instr!(fun Downgrader, "is_zero"),
            make_block!(1,
                add_instr!(Nop)
            ),
            add_instr!(Push, 0),
            add_instr!(fun Downgrade, "is_zero"),
        ],
        verifier: { error with downgrader_policy("is_zero", Some(1)),
            VerifierError::Flow(FlowError::DowngraderReturnTagMismatch {
                found: Public,
                expected: Secret,
            })
        },
        executor: { error with downgrader_policy("is_zero", Some(1)),
            ExecutorError::Flow(FlowError::DowngraderReturnTagMismatch {
                found: Public,
                expected: Secret,
            })
        },
    }

    test_program! {
        /// Downgraders usually should come with a call limit, which is tested here.
        call_limit,
        program: vec![
            add_instr!(fun Downgrader, "is_empty"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "is_empty"),
            add_instr!(R Pop, 1),
            add_instr!(fun Downgrade, "is_empty"),
        ],
        verifier: { error with downgrader_policy("is_empty", Some(1)),
            VerifierError::Flow(FlowError::DowngraderCallLimitExceeded { limit: 1, .. })
        },
        executor: { error with downgrader_policy("is_empty", Some(1)),
            ExecutorError::Flow(FlowError::DowngraderCallLimitExceeded { limit: 1, .. })
        },
    }

    test_program! {
        /// Another test for the downgrader call limit, this time with 2 allowed calls.
        call_limit_2,
        program: vec![
            add_instr!(fun Downgrader, "is_empty"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "is_empty"),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "is_empty"),
        ],
        verifier: { tagged_stack with downgrader_policy("is_empty", Some(2)), [
                (0, Secret),
                (1, Public),
                (0, Secret),
                (1, Public)
            ]
        },
        executor: { tagged_stack with downgrader_policy("is_empty", Some(2)), [
                (0, Secret),
                (1, Public),
                (0, Secret),
                (1, Public)
            ]
        },
    }

    test_program! {
        /// A downgrader with no call limit
        no_call_limit,
        program: vec![
            add_instr!(fun Downgrader, "is_empty"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "is_empty"),
            add_instr!(R Pop, 1),
            add_instr!(fun Downgrade, "is_empty"),
            add_instr!(R Pop, 1),
            add_instr!(fun Downgrade, "is_empty"),
        ],
        verifier: { tagged_stack with downgrader_policy("is_empty", None), [
                (0, Secret),
                (1, Public)
            ]
        },
        executor: { tagged_stack with downgrader_policy("is_empty", None), [
                (0, Secret),
                (1, Public)
            ]
        },
    }

    test_program! {
        /// Exploring both branches of an unknown condition must not double-charge
        /// the call limit: at runtime only one branch executes, so the verifier merges
        /// the two branches' call counts with MAX, not their sum. Here each branch
        /// reaches one downgrade after an earlier no-op (merged count: 1), and a
        /// final top-level downgrade makes it 2, exactly within `max_calls = 2`.
        call_limit_ifelse,
        program: vec![
            add_instr!(fun Downgrader, "is_empty"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(Input), // unknown condition
            add_instr!(tag Push, 0, Secret),
            add_instr!(ifelse 0,
                [
                    add_instr!(Nop),
                    add_instr!(fun Downgrade, "is_empty"),
                ],
                [
                    add_instr!(Nop),
                    add_instr!(fun Downgrade, "is_empty"),
                ],
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "is_empty"),
        ],
        verifier: { tagged_stack with downgrader_policy("is_empty", Some(2)), [
                (ValueSpan::inf(), Secret),
                (0, Secret),
                (1, Public),
                (0, Secret),
                (1, Public)
            ]
        },
        executor: { cases with downgrader_policy("is_empty", Some(2)), {
            input [1] => tagged_stack [
                (1, Secret),
                (0, Secret),
                (1, Public),
                (0, Secret),
                (1, Public)
            ];
            input [0] => tagged_stack [
                (0, Secret),
                (0, Secret),
                (1, Public),
                (0, Secret),
                (1, Public)
            ]
        } },
    }

    test_program! {
        /// Downgraders cannot be called from inside another downgrader or another ordinary function.
        nested_rejected,
        program: vec![
            add_instr!(fun Downgrader, "inner"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(fun Downgrader, "outer"),
            make_block!(1,
                add_instr!(tag Push, 0, Secret),
                add_instr!(fun Downgrade, "inner") // FAILS: nested call
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "outer"),
        ],
        verifier: { error with downgrader_policy("inner", Some(1))
                .with_downgrader("outer", Secret, Public, Some(1))
                .unwrap(),
            VerifierError::Flow(FlowError::NestedDowngraderCall { .. })
        },
        executor: { error with downgrader_policy("inner", Some(1))
                .with_downgrader("outer", Secret, Public, Some(1))
                .unwrap(),
            ExecutorError::Flow(FlowError::NestedDowngraderCall { .. })
        },
    }

    test_program! {
        /// The verifier rejects an ordinary `FunctionCall` inside a downgrader
        /// body rather than traversing or summarizing it. The executor retains
        /// its concrete behavior and executes the helper normally.
        function_call_inside_downgrader_rejected_by_verifier,
        program: vec![
            add_instr!(fun FunctionDefine, "helper"),
            make_block!(0,
                add_instr!(tag Push, 5, Secret)
            ),
            add_instr!(fun Downgrader, "d"),
            make_block!(1,
                add_instr!(fun FunctionCall, "helper")
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun Downgrade, "d"),
        ],
        split,
        verifier: { error with downgrader_policy("d", Some(1)),
            VerifierError::FunctionCallInsideDowngrader { function, downgrader }
                if function == "helper" && downgrader == "d"
        },
        executor: { tagged_stack with downgrader_policy("d", Some(1)), [
                (0, Secret),
                (5, Public)
            ]
        },
    }

    test_program! {
        /// A registered downgrader may not be invoked with the ordinary `FunctionCall`:
        /// it must use `Downgrade`. Both runners reject the call as `DowngraderUndefined`.
        function_call_rejected,
        program: vec![
            add_instr!(fun Downgrader, "is_empty"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(tag Push, 0, Secret),
            add_instr!(fun FunctionCall, "is_empty"), // FAILS: must use Downgrade
        ],
        verifier: { error with downgrader_policy("is_empty", Some(1)),
            // VerifierError::Flow(FlowError::DowngraderUndefined { .. })
            VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionUndefined(_)))
        },
        executor: { error with downgrader_policy("is_empty", Some(1)),
            // ExecutorError::Flow(FlowError::DowngraderUndefined { .. })
            ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionUndefined(_)))
        },
    }

    test_program! {
        /// `Downgrade` may only name a downgrader registered in the policy.
        /// Pointing it at an ordinary function is rejected as `DowngraderUndefined`.
        normal_function_rejected,
        program: vec![
            add_instr!(fun FunctionDefine, "helper"),
            make_block!(0,
                add_instr!(Push, 5)
            ),
            add_instr!(fun Downgrade, "helper"),
        ],
        verifier: { error with confidentiality_policy(),
            VerifierError::Flow(FlowError::DowngraderUndefined { .. })
        },
        executor: { error with confidentiality_policy(),
            ExecutorError::Flow(FlowError::DowngraderUndefined { .. })
        },
    }

    test_program! {
        /// A downgrader must be specified with the security policy.
        undefined_in_policy,
        program: vec![
            add_instr!(fun Downgrader, "helper"),
            make_block!(0,
                add_instr!(Push, 5)
            ),
        ],
        verifier: { error with confidentiality_policy(),
            VerifierError::Flow(FlowError::DowngraderUndefined { .. })
        },
        executor: { error with confidentiality_policy(),
            ExecutorError::Flow(FlowError::DowngraderUndefined { .. })
        },
    }

    test_program! {
        /// Functions and downgraders have separate namespaces.
        function_no_clash,
        program: vec![
            add_instr!(fun Downgrader, "is_empty"),
            make_block!(0,
                add_instr!(Nop)
            ),
            add_instr!(fun FunctionDefine, "is_empty"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(Push, 0),
            add_instr!(fun FunctionCall, "is_empty"),
        ],
        verifier: { tagged_stack with downgrader_policy("is_empty", Some(1)), [
                (0, Public),
                (1, Public)
            ]
        },
        executor: { tagged_stack with downgrader_policy("is_empty", Some(1)), [
                (0, Public),
                (1, Public)
            ]
        },
    }

    test_program! {
        /// A downgrader defined inside a function body is a nested definition.
        /// The verifier rejects it at `outer`'s definition; the executor cannot
        /// catch this and runs the body when `outer` is called.
        nested_define_rejected,
        program: vec![
            add_instr!(fun FunctionDefine, OUTER),
            make_block!(0,
                add_instr!(fun Downgrader, "is_empty"),
                make_block!(1,
                    add_instr!(Push, 0),
                    add_instr!(CmpEqual, 0, 1)
                ),
                add_instr!(Push, 42)
            ),
            add_instr!(fun FunctionCall, OUTER),
        ],
        split,
        verifier: { error with downgrader_policy("is_empty", Some(1)),
            VerifierError::NestedFunctionDefinition { .. }
        },
        executor: { tagged_stack with downgrader_policy("is_empty", Some(1)), [
            (42, Public)
        ] },
    }

    test_program! {
        /// Downgrader calls must happen at the top level of the program: a
        /// `Downgrade` inside an ordinary function body is rejected. The verifier
        /// catches it at the function's definition; the executor when the function
        /// runs.
        inside_function_rejected,
        program: vec![
            add_instr!(fun Downgrader, "is_empty"),
            make_block!(1,
                add_instr!(Push, 0),
                add_instr!(CmpEqual, 0, 1)
            ),
            add_instr!(fun FunctionDefine, "wrapper"),
            make_block!(0,
                add_instr!(tag Push, 0, Secret),
                add_instr!(fun Downgrade, "is_empty")
            ),
            add_instr!(fun FunctionCall, "wrapper"),
        ],
        verifier: { error with downgrader_policy("is_empty", Some(1)),
            VerifierError::Flow(FlowError::NestedDowngraderCall { .. })
        },
        executor: { error with downgrader_policy("is_empty", Some(1)),
            ExecutorError::Flow(FlowError::NestedDowngraderCall { .. })
        },
    }

    test_program! {
        /// A downgrader defined twice under the same name is a redefinition.
        redefinition_rejected,
        program: vec![
            add_instr!(fun Downgrader, "foo"),
            make_block!(0,
                add_instr!(tag Push, 0, Secret)
            ),
            add_instr!(fun Downgrader, "foo"),
            make_block!(0,
                add_instr!(tag Push, 0, Secret)
            ),
        ],
        verifier: { error with downgrader_policy("foo", Some(1)),
            VerifierError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionRedefinition(_)))
        },
        executor: { error with downgrader_policy("foo", Some(1)),
            ExecutorError::Core(CoreError::FunctionDataError(FunctionDataError::FunctionRedefinition(_)))
        },
    }
}
