use std::{collections::HashMap, fmt::Debug, hash::Hash};

pub trait FlowTag: Copy + Eq + Hash + Debug {}

impl<T: Copy + Eq + Hash + Debug> FlowTag for T {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError<Tag: FlowTag> {
    DuplicateTag(Tag),
    UnknownTag(Tag),
    Cycle,
    AmbiguousClosestCommonDescendant { left: Tag, right: Tag },
    NoCommonDescendant { left: Tag, right: Tag },
    InformationFlowViolation { found: Tag, guard: Tag },
}

#[derive(Debug, Clone)]
pub struct FlowGraph<Tag: FlowTag> {
    tags: Vec<Tag>,
    indices: HashMap<Tag, usize>,
    reachable: Vec<Vec<bool>>,
    ccd: Vec<Vec<Option<Tag>>>,
}

impl<Tag: FlowTag> FlowGraph<Tag> {
    pub fn new(
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

        let mut reachable = vec![vec![false; tags.len()]; tags.len()];
        for (index, row) in reachable.iter_mut().enumerate() {
            row[index] = true;
        }
        for (from, to) in edges {
            let from_index = *indices.get(&from).ok_or(FlowError::UnknownTag(from))?;
            let to_index = *indices.get(&to).ok_or(FlowError::UnknownTag(to))?;
            reachable[from_index][to_index] = true;
        }

        for through in 0..tags.len() {
            for from in 0..tags.len() {
                for to in 0..tags.len() {
                    reachable[from][to] |= reachable[from][through] && reachable[through][to];
                }
            }
        }

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
                let common: Vec<usize> = (0..tags.len())
                    .filter(|candidate| reachable[left][*candidate] && reachable[right][*candidate])
                    .collect();
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
            tags,
            indices,
            reachable,
            ccd,
        })
    }

    pub fn contains(&self, tag: Tag) -> bool {
        self.indices.contains_key(&tag)
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn can_flow(&self, from: Tag, to: Tag) -> Result<bool, FlowError<Tag>> {
        let from = *self.indices.get(&from).ok_or(FlowError::UnknownTag(from))?;
        let to = *self.indices.get(&to).ok_or(FlowError::UnknownTag(to))?;
        Ok(self.reachable[from][to])
    }

    pub fn closest_common_descendant(&self, left: Tag, right: Tag) -> Result<Tag, FlowError<Tag>> {
        let left_index = *self.indices.get(&left).ok_or(FlowError::UnknownTag(left))?;
        let right_index = *self
            .indices
            .get(&right)
            .ok_or(FlowError::UnknownTag(right))?;
        self.ccd[left_index][right_index].ok_or(FlowError::NoCommonDescendant { left, right })
    }

    pub fn closest_common_descendant_all(
        &self,
        tags: impl IntoIterator<Item = Tag>,
    ) -> Result<Option<Tag>, FlowError<Tag>> {
        let mut tags: Vec<Tag> = tags.into_iter().collect();
        let Some(mut result) = tags.pop() else {
            return Ok(None);
        };
        while let Some(left) = tags.pop() {
            result = self.closest_common_descendant(left, result)?;
        }
        Ok(Some(result))
    }
}

pub trait InformationFlowPolicy {
    type Tag: FlowTag;

    fn default_tag(&self) -> Self::Tag;
    fn input_tag(&self) -> Self::Tag;
    fn output_tag(&self) -> Self::Tag;
    fn validate_tag(&self, tag: Self::Tag) -> Result<(), FlowError<Self::Tag>>;
    fn closest_common_descendant(
        &self,
        left: Self::Tag,
        right: Self::Tag,
    ) -> Result<Self::Tag, FlowError<Self::Tag>>;
    fn can_flow(&self, from: Self::Tag, to: Self::Tag) -> Result<bool, FlowError<Self::Tag>>;
}

#[derive(Debug, Clone)]
pub struct GraphFlowPolicy<Tag: FlowTag> {
    graph: FlowGraph<Tag>,
    default_tag: Tag,
    input_tag: Tag,
    output_tag: Tag,
}

impl<Tag: FlowTag> GraphFlowPolicy<Tag> {
    pub fn new(
        graph: FlowGraph<Tag>,
        default_tag: Tag,
        input_tag: Tag,
        output_tag: Tag,
    ) -> Result<Self, FlowError<Tag>> {
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
        })
    }

    pub fn graph(&self) -> &FlowGraph<Tag> {
        &self.graph
    }
}

impl<Tag: FlowTag> InformationFlowPolicy for GraphFlowPolicy<Tag> {
    type Tag = Tag;

    fn default_tag(&self) -> Tag {
        self.default_tag
    }

    fn input_tag(&self) -> Tag {
        self.input_tag
    }

    fn output_tag(&self) -> Tag {
        self.output_tag
    }

    fn validate_tag(&self, tag: Tag) -> Result<(), FlowError<Tag>> {
        if self.graph.contains(tag) {
            Ok(())
        } else {
            Err(FlowError::UnknownTag(tag))
        }
    }

    fn closest_common_descendant(&self, left: Tag, right: Tag) -> Result<Tag, FlowError<Tag>> {
        self.graph.closest_common_descendant(left, right)
    }

    fn can_flow(&self, from: Tag, to: Tag) -> Result<bool, FlowError<Tag>> {
        self.graph.can_flow(from, to)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoFlow;

impl InformationFlowPolicy for NoFlow {
    type Tag = ();

    fn default_tag(&self) {}
    fn input_tag(&self) {}
    fn output_tag(&self) {}
    fn validate_tag(&self, _tag: ()) -> Result<(), FlowError<()>> {
        Ok(())
    }
    fn closest_common_descendant(&self, _left: (), _right: ()) -> Result<(), FlowError<()>> {
        Ok(())
    }
    fn can_flow(&self, _from: (), _to: ()) -> Result<bool, FlowError<()>> {
        Ok(true)
    }
}

// TODO: Review
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

    #[test]
    fn chain_reachability_and_ccd() {
        let graph = FlowGraph::new(
            [Tag::Public, Tag::Constrained, Tag::Private],
            [
                (Tag::Public, Tag::Constrained),
                (Tag::Constrained, Tag::Private),
            ],
        )
        .unwrap();
        assert!(graph.can_flow(Tag::Public, Tag::Private).unwrap());
        assert_eq!(
            graph
                .closest_common_descendant(Tag::Public, Tag::Private)
                .unwrap(),
            Tag::Private
        );
        assert_eq!(
            graph
                .closest_common_descendant_all([Tag::Public, Tag::Constrained, Tag::Private])
                .unwrap(),
            Some(Tag::Private)
        );
    }

    #[test]
    fn missing_and_ambiguous_descendants() {
        let separate = FlowGraph::new([Tag::Public, Tag::Separate], []).unwrap();
        assert!(matches!(
            separate.closest_common_descendant(Tag::Public, Tag::Separate),
            Err(FlowError::NoCommonDescendant { .. })
        ));

        let ambiguous = FlowGraph::new(
            [Tag::Public, Tag::Separate, Tag::Constrained, Tag::Private],
            [
                (Tag::Public, Tag::Constrained),
                (Tag::Separate, Tag::Constrained),
                (Tag::Public, Tag::Private),
                (Tag::Separate, Tag::Private),
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
            FlowGraph::new([Tag::Public, Tag::Public], []),
            Err(FlowError::DuplicateTag(Tag::Public))
        ));
        assert!(matches!(
            FlowGraph::new(
                [Tag::Public, Tag::Private],
                [(Tag::Public, Tag::Private), (Tag::Private, Tag::Public)]
            ),
            Err(FlowError::Cycle)
        ));

        let graph =
            FlowGraph::new([Tag::Public, Tag::Private], [(Tag::Public, Tag::Private)]).unwrap();
        assert!(matches!(
            GraphFlowPolicy::new(graph, Tag::Public, Tag::Private, Tag::Separate),
            Err(FlowError::UnknownTag(Tag::Separate))
        ));
    }
}
