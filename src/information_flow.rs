//! Dynamic information-flow tags, graphs, and executor policies.
//!
//! A tag describes where a value may flow. The graph's directed edges are
//! written in the same direction as allowed information flow:
//!
//! ```text
//! Public -> Constrained -> Secret
//! ```
//!
//! Perimeter guards define how data enters and exits the system, e.g. no data
//! tagged `Secret` may flow to `Public` outputs.
//!
//! Throughout this crate, a generic parameter named `Tag` is the *tag type*:
//! the set of tags (e.g. `Public`/`Constrained`/`Secret`) that topologies and
//! policies are built over. A value of that type is one single tag.
//!
//! # Usage
//!
//! Define a [`Topology`] over your tag type, build a [`SecurityPolicy`] from it
//! (optionally registering downgraders via
//! [`SecurityPolicy::with_downgrader`]), and hand the policy to a monitored
//! executor or verifier via their `with_policy` constructors.
//!
//! # Examples
//!
//! See the tests in this module and the DIFTAM tests in the test catalog.

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use crate::instruction::{Instruction, UnaryOpImm};

type TagIndex = usize;
type ReachabilityMatrix = Vec<Vec<bool>>;

/// Trait bound required of a tag type (the set of tags a policy is built over).
///
/// This trait has a blanket implementation, so any small enum deriving
/// `Copy`, `Eq`, `Hash`, and `Debug` can be used directly as a tag type.
pub trait TagTrait: Copy + Eq + Hash + Debug {}

impl<T: Copy + Eq + Hash + Debug> TagTrait for T {}

/// Errors produced while building a flow graph or enforcing a flow policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError<Tag: TagTrait> {
    DuplicateTag(Tag),
    UnknownTag(Tag),
    Cycle,
    AmbiguousClosestCommonDescendant {
        left: Tag,
        right: Tag,
    },
    NoCommonDescendant {
        left: Tag,
        right: Tag,
    },
    InformationFlowViolation {
        found: Tag,
        guard: Tag,
    },
    ReflexiveAwareConnection(Tag),
    DuplicateDowngrader(String),
    /// A downgrader's body returned a value whose tag is not its connection
    /// `source`, so the implicit retag to `target` is rejected.
    DowngraderReturnTagMismatch {
        found: Tag,
        expected: Tag,
    },
    /// A `Downgrade` occurred inside a function or downgrader body. Downgrader
    /// calls must happen at the top level of the program, so downgrades are
    /// explicit, non-transitive, and countable.
    NestedDowngraderCall {
        downgrader: String,
    },
    /// The downgrader was called more times than its total `max_calls` budget
    /// for the program run allows.
    DowngraderCallLimitExceeded {
        downgrader: String,
        limit: usize,
    },
    /// A `Downgrader`/`Downgrade` instruction named something not registered
    /// as a downgrader in the policy.
    DowngraderUndefined {
        name: String,
    },
}

/// A tag belonging to one side of a disjoint union of topologies.
///
/// The variants keep equal tag values from different topologies distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisjointTag<Left: TagTrait, Right: TagTrait> {
    Left(Left),
    Right(Right),
}

/// An unvalidated description of an information-flow graph.
///
/// ***This is just a convenient intermediate representation.***
/// Then, [`Topology::into_graph`] converts it into a graph which is actually useful, as well as
/// validated.
///
/// Topologies make graph definitions more concise:
///
/// - [`Topology::basic`] infers tags from explicitly listed edges.
/// - [`Topology::linear`] connects every tag to the next tag in a sequence.
/// - [`Topology::disjoint_union`] represents a choice between two
///   independent topologies.
/// - [`Topology::cartesian_product`] combines two independent
///   dimensions into pairs of tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topology<Tag: TagTrait> {
    tags: Vec<Tag>,
    edges: Vec<(Tag, Tag)>,
}

impl<Tag: TagTrait> Topology<Tag> {
    /// Creates a basic topology and infers its tags from the supplied edges.
    ///
    /// Tags retain the order of their first appearance.
    pub fn basic(edges: impl IntoIterator<Item = (Tag, Tag)>) -> Self {
        let edges: Vec<(Tag, Tag)> = edges.into_iter().collect();
        let mut tags = Vec::new();
        let mut seen = HashSet::new();

        // Add each pair of connected tags into the tag list
        for (from, to) in edges.iter().copied() {
            for tag in [from, to] {
                if seen.insert(tag) {
                    tags.push(tag);
                }
            }
        }

        Self { tags, edges }
    }

    /// Creates a linear topology by connecting every tag to its successor.
    pub fn linear(tags: impl IntoIterator<Item = Tag>) -> Self {
        let tags: Vec<Tag> = tags.into_iter().collect();
        let edges = tags.windows(2).map(|pair| (pair[0], pair[1])).collect();
        Self { tags, edges }
    }

    /// Creates the disjoint union of two topologies.
    ///
    /// Resulting tags are wrapped in [`DisjointTag`] to preserve which
    /// topology they came from.
    pub fn disjoint_union<T1: TagTrait, T2: TagTrait>(
        tpl1: Topology<T1>,
        tpl2: Topology<T2>,
    ) -> Topology<DisjointTag<T1, T2>> {
        use DisjointTag::{Left, Right};

        // Tags from both topologies are collected and wrapped in `Left` or `Right` to keep them
        // distinct.
        let tags = {
            let tags_left = tpl1.tags.into_iter().map(Left);
            let tags_right = tpl2.tags.into_iter().map(Right);
            tags_left.chain(tags_right).collect()
        };

        let edges = {
            let edges_left = tpl1
                .edges
                .into_iter()
                .map(|(from, to)| (Left(from), Left(to)));
            let edges_right = tpl2
                .edges
                .into_iter()
                .map(|(from, to)| (Right(from), Right(to)));
            edges_left.chain(edges_right).collect()
        };

        Topology { tags, edges }
    }

    /// Creates the Cartesian product of two topologies.
    ///
    /// Each resulting tag is a pair. An edge changes one component while the
    /// other component remains fixed.
    pub fn cartesian_product<T1: TagTrait, T2: TagTrait>(
        tpl1: Topology<T1>,
        tpl2: Topology<T2>,
    ) -> Topology<(T1, T2)> {
        let mut tags = Vec::with_capacity(tpl1.tags.len().saturating_mul(tpl2.tags.len()));
        for left in tpl1.tags.iter().copied() {
            for right in tpl2.tags.iter().copied() {
                tags.push((left, right));
            }
        }

        let mut edges = Vec::with_capacity(
            tpl1.edges
                .len()
                .saturating_mul(tpl2.tags.len())
                .saturating_add(tpl2.edges.len().saturating_mul(tpl1.tags.len())),
        );
        for (from, to) in tpl1.edges.iter().copied() {
            for right in tpl2.tags.iter().copied() {
                edges.push(((from, right), (to, right)));
            }
        }
        for left in tpl1.tags.iter().copied() {
            for (from, to) in tpl2.edges.iter().copied() {
                edges.push(((left, from), (left, to)));
            }
        }

        Topology { tags, edges }
    }

    /// Returns all topology tags in stable construction order.
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// Returns the explicitly described topology edges.
    pub fn edges(&self) -> &[(Tag, Tag)] {
        &self.edges
    }

    /// Validates and preprocesses this topology as a [`PolicyGraph`].
    fn into_graph(self) -> Result<PolicyGraph<Tag>, FlowError<Tag>> {
        PolicyGraph::new(self.tags, self.edges)
    }
}

/// A validated directed graph of allowed information flows.
///
/// `PolicyGraph` stores the edges for a tag type and precomputes the reflexive transitive
/// closure and closest common descendants. This speeds up the
/// [`Executor`](crate::machine::executor::Executor) checks.
///
/// Cycles and ambiguous closest common descendants are rejected.
/// Pairs with no common descendant are allowed, but attempting to combine such
/// a pair later returns [`FlowError::NoCommonDescendant`].
#[derive(Debug, Clone)]
struct PolicyGraph<Tag: TagTrait> {
    /// Maps tag values to indices used by the graph matrices.
    indices: HashMap<Tag, TagIndex>,
    /// `reachable[a][b]` == true iff `a` may flow to `b`.
    reachable: ReachabilityMatrix,
    /// Precomputed closest common descendant for every pair of tags.
    ccd: Vec<Vec<Option<Tag>>>,
}

impl<Tag: TagTrait> PolicyGraph<Tag> {
    fn new(
        tags: impl IntoIterator<Item = Tag>,
        edges: impl IntoIterator<Item = (Tag, Tag)>,
    ) -> Result<Self, FlowError<Tag>> {
        let tags: Vec<Tag> = tags.into_iter().collect();
        let mut indices = HashMap::with_capacity(tags.len());
        for (index, tag) in tags.iter().copied().enumerate() {
            if indices.insert(tag, index).is_some() {
                return Err(FlowError::DuplicateTag(tag));
            }
        }

        // Reflexivity: every tag flows to itself.
        let mut reachable = vec![vec![false; tags.len()]; tags.len()];
        for (index, row) in reachable.iter_mut().enumerate() {
            row[index] = true;
        }

        // Add the edges supplied
        for (from, to) in edges {
            let from_index = *indices.get(&from).ok_or(FlowError::UnknownTag(from))?;
            let to_index = *indices.get(&to).ok_or(FlowError::UnknownTag(to))?;
            reachable[from_index][to_index] = true;
        }

        // Transitivity: if `a -> b` and `b -> c`, then `a -> c`.
        for through in 0..tags.len() {
            for from in 0..tags.len() {
                for to in 0..tags.len() {
                    reachable[from][to] |= reachable[from][through] && reachable[through][to];
                }
            }
        }

        // Reject cycles
        for (left, left_row) in reachable.iter().enumerate() {
            for (right, right_row) in reachable.iter().enumerate().skip(left + 1) {
                if left_row[right] && right_row[left] {
                    return Err(FlowError::Cycle);
                }
            }
        }

        let mut ccd = vec![vec![None; tags.len()]; tags.len()];
        for left in 0..tags.len() {
            for right in 0..tags.len() {
                // Find all common descendants of `left` and `right`.
                let common: Vec<usize> = (0..tags.len())
                    .filter(|candidate| reachable[left][*candidate] && reachable[right][*candidate])
                    .collect();
                // Find closest among common descendants. If any of the candidates is reachable
                // from another, it's not closest (they get filtered out).
                let closest: Vec<usize> = common
                    .iter()
                    .copied()
                    .filter(|candidate| {
                        !common
                            .iter()
                            .copied()
                            .any(|other| other != *candidate && reachable[other][*candidate])
                    })
                    .collect();

                match closest.as_slice() {
                    [] => {}
                    [only] => ccd[left][right] = Some(tags[*only]),
                    _ => {
                        return Err(FlowError::AmbiguousClosestCommonDescendant {
                            left: tags[left],
                            right: tags[right],
                        });
                    }
                }
            }
        }

        Ok(Self {
            indices,
            reachable,
            ccd,
        })
    }

    /// Returns whether `tag` belongs to this graph.
    fn contains(&self, tag: Tag) -> bool {
        self.indices.contains_key(&tag)
    }

    /// Returns whether information tagged `from` may flow to `to`.
    fn can_flow(&self, from: Tag, to: Tag) -> Result<bool, FlowError<Tag>> {
        let from_index = self.index_of(from)?;
        let to_index = self.index_of(to)?;
        Ok(self.reachable[from_index][to_index])
    }

    /// Returns the closest common descendant of `left` and `right`, if it exists.
    fn ccd(&self, left: Tag, right: Tag) -> Result<Tag, FlowError<Tag>> {
        let left_index = self.index_of(left)?;
        let right_index = self.index_of(right)?;
        self.ccd[left_index][right_index].ok_or(FlowError::NoCommonDescendant { left, right })
    }

    fn index_of(&self, tag: Tag) -> Result<TagIndex, FlowError<Tag>> {
        self.indices
            .get(&tag)
            .copied()
            .ok_or(FlowError::UnknownTag(tag))
    }
}

/// Standard information-flow policy backed by a validated flow graph.
///
/// Besides the graph, this policy configures the three perimeter/default tags
/// needed by the executor:
///
/// - `default_tag`: tag for ordinary constants.
/// - `input_tag`: tag automatically applied to input values.
/// - `output_tag`: guard that output values must be allowed to flow to.
#[derive(Debug, Clone)]
pub struct SecurityPolicy<Tag: TagTrait> {
    graph: PolicyGraph<Tag>,
    default_tag: Tag,
    input_tag: Tag,
    output_tag: Tag,
    /// Named aware-flow downgraders, layered on top of the oblivious `graph`.
    downgraders: HashMap<String, Downgrader<Tag>>,
}

impl<Tag: TagTrait> SecurityPolicy<Tag> {
    /// Creates a policy over `topology` and validates all configured policy
    /// tags.
    pub fn new(
        topology: Topology<Tag>,
        default_tag: Tag,
        input_tag: Tag,
        output_tag: Tag,
    ) -> Result<Self, FlowError<Tag>> {
        let graph = topology.into_graph()?;
        for tag in [default_tag, input_tag, output_tag] {
            if !graph.contains(tag) {
                return Err(FlowError::UnknownTag(tag));
            }
        }
        Ok(Self {
            graph,
            default_tag,
            input_tag,
            output_tag,
            downgraders: HashMap::new(),
        })
    }

    /// Registers an aware connection `source ->> target` behind a named
    /// downgrader, returning the augmented policy for chaining.
    ///
    /// Rejects reflexive connections ([`FlowError::ReflexiveAwareConnection`]),
    /// tags unknown to the graph ([`FlowError::UnknownTag`]), and duplicate
    /// downgrader names ([`FlowError::DuplicateDowngrader`]).
    ///
    /// `max_calls` is the downgrader's total call budget for a whole program
    /// run (`None` = unlimited).
    pub fn with_downgrader(
        mut self,
        name: impl Into<String>,
        source: Tag,
        target: Tag,
        max_calls: Option<usize>,
    ) -> Result<Self, FlowError<Tag>> {
        if source == target {
            return Err(FlowError::ReflexiveAwareConnection(source));
        }
        for tag in [source, target] {
            if !self.graph.contains(tag) {
                return Err(FlowError::UnknownTag(tag));
            }
        }
        let name = name.into();
        if self.downgraders.contains_key(&name) {
            return Err(FlowError::DuplicateDowngrader(name));
        }
        self.downgraders.insert(
            name,
            Downgrader {
                connection: AwareConnection { source, target },
                max_calls,
            },
        );
        Ok(self)
    }

    /// Returns whether an aware connection `source ->> target` was declared
    /// directly. Never infers transitive chains.
    pub fn has_aware_connection(&self, source: Tag, target: Tag) -> bool {
        self.downgraders
            .values()
            .any(|d| d.connection.source == source && d.connection.target == target)
    }

    /// Tag for ordinary constants.
    pub fn default_tag(&self) -> Tag {
        self.default_tag
    }

    /// Tag automatically applied to input values.
    pub fn input_tag(&self) -> Tag {
        self.input_tag
    }

    /// Guard that output values must be allowed to flow to.
    pub fn output_tag(&self) -> Tag {
        self.output_tag
    }

    /// Checks that a tag embedded in a program is known to this policy.
    pub fn validate_tag(&self, tag: Tag) -> Result<(), FlowError<Tag>> {
        self.graph.index_of(tag).map(|_| ())
    }

    /// Computes the closest common descendant of two tags, if it exists.
    pub fn closest_common_descendant(&self, left: Tag, right: Tag) -> Result<Tag, FlowError<Tag>> {
        self.graph.ccd(left, right)
    }

    /// Checks whether 'from -> to' holds for given tags.
    pub fn can_flow(&self, from: Tag, to: Tag) -> Result<bool, FlowError<Tag>> {
        self.graph.can_flow(from, to)
    }

    /// Returns the downgrader registered under `name`, if any.
    pub fn downgrader(&self, name: &str) -> Option<Downgrader<Tag>> {
        self.downgraders.get(name).copied()
    }
}

impl SecurityPolicy<()> {
    /// Policy for unmonitored runs: a single unit tag, everything allowed.
    ///
    /// Used by [`Executor::new`](crate::machine::executor::Executor::new) and
    /// [`Verifier::new`](crate::machine::verifier::Verifier::new); every tag is
    /// `()`, every flow and combination succeeds, and no downgraders exist.
    pub fn no_flow() -> Self {
        Self::new(Topology::linear([()]), (), (), ())
            .expect("the unit topology is always a valid policy")
    }
}

/// Checks that every tag embedded in `program` (in `TaggedPush` instructions,
/// recursively through blocks and ifelse branches) is known to `policy`.
///
/// Used by both runners' `with_policy` constructors; their error types convert
/// the returned [`FlowError`] at the call site.
pub(crate) fn validate_program_tags<Tag: TagTrait>(
    program: &[Instruction<Tag>],
    policy: &SecurityPolicy<Tag>,
) -> Result<(), FlowError<Tag>> {
    for instruction in program {
        validate_instruction_tags(instruction, policy)?;
    }
    Ok(())
}

fn validate_instruction_tags<Tag: TagTrait>(
    instruction: &Instruction<Tag>,
    policy: &SecurityPolicy<Tag>,
) -> Result<(), FlowError<Tag>> {
    match instruction {
        Instruction::AluUnaryImm(UnaryOpImm::TaggedPush(tag), _) => {
            policy.validate_tag(*tag)?;
        }
        Instruction::Block(body) => validate_program_tags(body, policy)?,
        Instruction::IfElse(_, when_true, when_false) => {
            validate_instruction_tags(when_true, policy)?;
            validate_instruction_tags(when_false, policy)?;
        }
        _ => {}
    }
    Ok(())
}

/// An explicit, deliberately-controlled downgrade relation `source ->> target`.
///
/// Aware connections are explicit, non-reflexive, and non-transitive. They are
/// **not** part of the oblivious lattice and never participate in `ccd` or
/// `can_flow`. A connection is only usable through its trusted [`Downgrader`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AwareConnection<Tag: TagTrait> {
    pub source: Tag,
    pub target: Tag,
}

/// A trusted gate for a single [`AwareConnection`]. Whatever the downgrader
/// returns is implicitly retagged from `source` to `target`.
///
/// `max_calls` bounds how many times this gate may be called in a whole
/// program run; `None` means unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Downgrader<Tag: TagTrait> {
    pub connection: AwareConnection<Tag>,
    pub max_calls: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Tag {
        Public,
        Constrained,
        Private,
        Separate,
    }

    use Tag::*;

    #[test]
    fn chain_reachability_and_ccd() {
        let graph = Topology::linear([Public, Constrained, Private])
            .into_graph()
            .unwrap();

        assert!(graph.can_flow(Public, Private).unwrap());
        assert_eq!(graph.ccd(Public, Private).unwrap(), Private);
    }

    #[test]
    fn basic_topology_infers_stable_tags() {
        let graph = Topology::basic([(Public, Private), (Public, Separate), (Private, Separate)])
            .into_graph()
            .unwrap();

        assert!(graph.can_flow(Public, Separate).unwrap());
        assert!(!graph.can_flow(Separate, Public).unwrap());
    }

    #[test]
    fn algebraic_topologies_support_product_and_disjoint_union() {
        use DisjointTag::{Left, Right};

        let combined = {
            let part1 = Topology::linear([Public, Private]);
            let part2 = Topology::linear([Constrained, Separate]);
            let part3 = Topology::basic([(Public, Private)]);
            let tmp = Topology::<Tag>::cartesian_product(part1, part2);
            Topology::<Tag>::disjoint_union(tmp, part3)
        };

        let graph = combined.into_graph().unwrap();

        assert!(
            graph
                .can_flow(Left((Public, Constrained)), Left((Private, Separate)))
                .unwrap()
        );
        assert_eq!(
            graph
                .ccd(Left((Private, Constrained)), Left((Public, Separate)))
                .unwrap(),
            Left((Private, Separate))
        );
        assert!(graph.can_flow(Right(Public), Right(Private)).unwrap());
        assert!(
            !graph
                .can_flow(Left((Public, Constrained)), Right(Private))
                .unwrap()
        );
        assert!(matches!(
            graph.ccd(Left((Public, Constrained)), Right(Public)),
            Err(FlowError::NoCommonDescendant { .. })
        ));
    }

    #[test]
    fn missing_and_ambiguous_descendants() {
        let separate = PolicyGraph::new([Public, Separate], []).unwrap();
        assert!(matches!(
            separate.ccd(Public, Separate),
            Err(FlowError::NoCommonDescendant { .. })
        ));

        let ambiguous = PolicyGraph::new(
            [Public, Separate, Constrained, Private],
            [
                (Public, Constrained),
                (Separate, Constrained),
                (Public, Private),
                (Separate, Private),
            ],
        );
        assert!(matches!(
            ambiguous,
            Err(FlowError::AmbiguousClosestCommonDescendant { .. })
        ));
    }

    #[test]
    fn rejects_bad_graphs_and_policy_tags() {
        assert!(matches!(
            PolicyGraph::new([Public, Public], []),
            Err(FlowError::DuplicateTag(Public))
        ));
        assert!(matches!(
            Topology::linear([Public, Public]).into_graph(),
            Err(FlowError::DuplicateTag(Public))
        ));
        assert!(matches!(
            PolicyGraph::new([Public, Private], [(Public, Private), (Private, Public)]),
            Err(FlowError::Cycle)
        ));

        let topology = Topology::basic([(Public, Private)]);
        assert!(matches!(
            SecurityPolicy::new(topology, Public, Private, Separate),
            Err(FlowError::UnknownTag(Separate))
        ));
    }

    #[test]
    fn rejects_reflexive_aware_connection() {
        let topology = Topology::basic([(Public, Private)]);
        let policy = SecurityPolicy::new(topology, Public, Private, Public).unwrap();
        assert!(matches!(
            policy.with_downgrader("identity", Private, Private, Some(1)),
            Err(FlowError::ReflexiveAwareConnection(Private))
        ));
    }

    #[test]
    fn aware_connections_are_not_transitive() {
        let topology = Topology::linear([Public, Constrained, Private]);
        let policy = SecurityPolicy::new(topology, Public, Private, Public)
            .unwrap()
            .with_downgrader("a_to_b", Private, Constrained, Some(1))
            .unwrap()
            .with_downgrader("b_to_c", Constrained, Public, Some(1))
            .unwrap();

        // Direct connections are present, but no chain is inferred.
        assert!(policy.has_aware_connection(Private, Constrained));
        assert!(policy.has_aware_connection(Constrained, Public));
        assert!(!policy.has_aware_connection(Private, Public));
    }
}
