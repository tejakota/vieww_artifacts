//! A small typed behavior-graph evaluator. This is the runtime substrate for a
//! future visual scripting canvas; the graph itself is data and can be edited
//! by an IDE without embedding a scripting language or third-party engine.

use std::collections::{BTreeMap, HashSet};

use crate::Vec3;

pub type NodeId = u32;

/// Typed values carried between behavior nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GraphValue {
    Number(f32),
    Boolean(bool),
    Vector3(Vec3),
}
impl GraphValue {
    #[must_use]
    pub fn as_number(self) -> Option<f32> {
        if let Self::Number(value) = self {
            Some(value)
        } else {
            None
        }
    }
    #[must_use]
    pub fn as_boolean(self) -> Option<bool> {
        if let Self::Boolean(value) = self {
            Some(value)
        } else {
            None
        }
    }
    #[must_use]
    pub fn as_vector3(self) -> Option<Vec3> {
        if let Self::Vector3(value) = self {
            Some(value)
        } else {
            None
        }
    }
    #[must_use]
    pub fn is_finite(self) -> bool {
        match self {
            Self::Number(value) => value.is_finite(),
            Self::Boolean(_) => true,
            Self::Vector3(value) => value.is_finite(),
        }
    }
}

/// Built-in node kinds. Every input reference is explicit and type checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
    Constant(GraphValue),
    DeltaSeconds,
    Add {
        a: NodeId,
        b: NodeId,
    },
    Multiply {
        a: NodeId,
        b: NodeId,
    },
    Sine {
        value: NodeId,
    },
    Clamp {
        value: NodeId,
        min: f32,
        max: f32,
    },
    Lerp {
        a: NodeId,
        b: NodeId,
        t: NodeId,
    },
    GreaterThan {
        a: NodeId,
        b: NodeId,
    },
    Select {
        condition: NodeId,
        when_true: NodeId,
        when_false: NodeId,
    },
    MakeVector3 {
        x: NodeId,
        y: NodeId,
        z: NodeId,
    },
    AddVector3 {
        a: NodeId,
        b: NodeId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BehaviorNode {
    pub id: NodeId,
    pub kind: NodeKind,
}
impl BehaviorNode {
    #[must_use]
    pub const fn new(id: NodeId, kind: NodeKind) -> Self {
        Self { id, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    DuplicateNode(NodeId),
    MissingNode(NodeId),
    Cycle(NodeId),
    TypeMismatch {
        node: NodeId,
        expected: &'static str,
    },
    InvalidParameter(NodeId),
    InvalidDeltaTime,
    NonFinite(NodeId),
}
impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateNode(id) => write!(f, "behavior graph contains duplicate node {id}"),
            Self::MissingNode(id) => write!(f, "behavior graph references missing node {id}"),
            Self::Cycle(id) => write!(f, "behavior graph contains a cycle at node {id}"),
            Self::TypeMismatch { node, expected } => {
                write!(f, "behavior node {node} expects {expected}")
            }
            Self::InvalidParameter(id) => write!(f, "behavior node {id} has invalid parameters"),
            Self::InvalidDeltaTime => {
                f.write_str("behavior graph delta must be finite and non-negative")
            }
            Self::NonFinite(id) => write!(f, "behavior node {id} produced a non-finite value"),
        }
    }
}
impl std::error::Error for GraphError {}

/// Deterministic dataflow graph. `evaluate` returns every node value, making it
/// easy for a visual editor to inspect pins and show intermediate values.
#[derive(Debug, Clone, Default)]
pub struct BehaviorGraph {
    nodes: BTreeMap<NodeId, NodeKind>,
}
impl BehaviorGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, node: BehaviorNode) -> Result<(), GraphError> {
        if self.nodes.contains_key(&node.id) {
            return Err(GraphError::DuplicateNode(node.id));
        }
        self.nodes.insert(node.id, node.kind);
        Ok(())
    }
    pub fn remove(&mut self, id: NodeId) -> Option<NodeKind> {
        self.nodes.remove(&id)
    }
    #[must_use]
    pub fn nodes(&self) -> &BTreeMap<NodeId, NodeKind> {
        &self.nodes
    }

    pub fn evaluate(&self, delta_seconds: f32) -> Result<BTreeMap<NodeId, GraphValue>, GraphError> {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(GraphError::InvalidDeltaTime);
        }
        let mut values = BTreeMap::new();
        let mut visiting = HashSet::new();
        for id in self.nodes.keys().copied() {
            self.evaluate_node(id, delta_seconds, &mut values, &mut visiting)?;
        }
        Ok(values)
    }

    fn evaluate_node(
        &self,
        id: NodeId,
        delta_seconds: f32,
        values: &mut BTreeMap<NodeId, GraphValue>,
        visiting: &mut HashSet<NodeId>,
    ) -> Result<GraphValue, GraphError> {
        if let Some(value) = values.get(&id) {
            return Ok(*value);
        }
        if !visiting.insert(id) {
            return Err(GraphError::Cycle(id));
        }
        let kind = *self.nodes.get(&id).ok_or(GraphError::MissingNode(id))?;
        let value = match kind {
            NodeKind::Constant(value) => value,
            NodeKind::DeltaSeconds => GraphValue::Number(delta_seconds),
            NodeKind::Add { a, b } => GraphValue::Number(
                number(self, a, id, delta_seconds, values, visiting)?
                    + number(self, b, id, delta_seconds, values, visiting)?,
            ),
            NodeKind::Multiply { a, b } => GraphValue::Number(
                number(self, a, id, delta_seconds, values, visiting)?
                    * number(self, b, id, delta_seconds, values, visiting)?,
            ),
            NodeKind::Sine { value } => {
                GraphValue::Number(number(self, value, id, delta_seconds, values, visiting)?.sin())
            }
            NodeKind::Clamp { value, min, max } => {
                if !min.is_finite() || !max.is_finite() || min > max {
                    return Err(GraphError::InvalidParameter(id));
                }
                GraphValue::Number(
                    number(self, value, id, delta_seconds, values, visiting)?.clamp(min, max),
                )
            }
            NodeKind::Lerp { a, b, t } => {
                let from = number(self, a, id, delta_seconds, values, visiting)?;
                let to = number(self, b, id, delta_seconds, values, visiting)?;
                let progress = number(self, t, id, delta_seconds, values, visiting)?;
                GraphValue::Number(from + (to - from) * progress)
            }
            NodeKind::GreaterThan { a, b } => GraphValue::Boolean(
                number(self, a, id, delta_seconds, values, visiting)?
                    > number(self, b, id, delta_seconds, values, visiting)?,
            ),
            NodeKind::Select {
                condition,
                when_true,
                when_false,
            } => {
                let choose_true = boolean(self, condition, id, delta_seconds, values, visiting)?;
                let input = if choose_true { when_true } else { when_false };
                self.evaluate_node(input, delta_seconds, values, visiting)?
            }
            NodeKind::MakeVector3 { x, y, z } => GraphValue::Vector3(Vec3::new(
                number(self, x, id, delta_seconds, values, visiting)?,
                number(self, y, id, delta_seconds, values, visiting)?,
                number(self, z, id, delta_seconds, values, visiting)?,
            )),
            NodeKind::AddVector3 { a, b } => {
                let left = vector3(self, a, id, delta_seconds, values, visiting)?;
                let right = vector3(self, b, id, delta_seconds, values, visiting)?;
                GraphValue::Vector3(left + right)
            }
        };
        visiting.remove(&id);
        if !value.is_finite() {
            return Err(GraphError::NonFinite(id));
        }
        values.insert(id, value);
        Ok(value)
    }
}

fn number(
    graph: &BehaviorGraph,
    input: NodeId,
    current: NodeId,
    dt: f32,
    values: &mut BTreeMap<NodeId, GraphValue>,
    visiting: &mut HashSet<NodeId>,
) -> Result<f32, GraphError> {
    graph
        .evaluate_node(input, dt, values, visiting)?
        .as_number()
        .ok_or(GraphError::TypeMismatch {
            node: current,
            expected: "a number",
        })
}
fn boolean(
    graph: &BehaviorGraph,
    input: NodeId,
    current: NodeId,
    dt: f32,
    values: &mut BTreeMap<NodeId, GraphValue>,
    visiting: &mut HashSet<NodeId>,
) -> Result<bool, GraphError> {
    graph
        .evaluate_node(input, dt, values, visiting)?
        .as_boolean()
        .ok_or(GraphError::TypeMismatch {
            node: current,
            expected: "a boolean",
        })
}
fn vector3(
    graph: &BehaviorGraph,
    input: NodeId,
    current: NodeId,
    dt: f32,
    values: &mut BTreeMap<NodeId, GraphValue>,
    visiting: &mut HashSet<NodeId>,
) -> Result<Vec3, GraphError> {
    graph
        .evaluate_node(input, dt, values, visiting)?
        .as_vector3()
        .ok_or(GraphError::TypeMismatch {
            node: current,
            expected: "a Vec3",
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_a_typed_graph_independent_of_insertion_order() {
        let mut graph = BehaviorGraph::new();
        graph
            .add(BehaviorNode::new(3, NodeKind::Multiply { a: 1, b: 2 }))
            .unwrap();
        graph
            .add(BehaviorNode::new(
                2,
                NodeKind::Constant(GraphValue::Number(4.0)),
            ))
            .unwrap();
        graph
            .add(BehaviorNode::new(
                1,
                NodeKind::Constant(GraphValue::Number(2.5)),
            ))
            .unwrap();
        assert_eq!(graph.evaluate(1.0).unwrap()[&3], GraphValue::Number(10.0));
    }

    #[test]
    fn cycles_and_type_errors_are_explicit() {
        let mut graph = BehaviorGraph::new();
        graph
            .add(BehaviorNode::new(1, NodeKind::Add { a: 2, b: 2 }))
            .unwrap();
        graph
            .add(BehaviorNode::new(2, NodeKind::Sine { value: 1 }))
            .unwrap();
        assert!(matches!(graph.evaluate(0.016), Err(GraphError::Cycle(_))));
        let mut wrong_type = BehaviorGraph::new();
        wrong_type
            .add(BehaviorNode::new(
                1,
                NodeKind::Constant(GraphValue::Boolean(true)),
            ))
            .unwrap();
        wrong_type
            .add(BehaviorNode::new(2, NodeKind::Sine { value: 1 }))
            .unwrap();
        assert!(matches!(
            wrong_type.evaluate(0.016),
            Err(GraphError::TypeMismatch { .. })
        ));
    }
}
