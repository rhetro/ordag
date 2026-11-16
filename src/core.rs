// src/core.rs

use ordex::prelude::*;
use std::collections::HashMap;

// ==========================================
// 1. Trait to be implemented by the user (Contract)
// ==========================================

pub trait TopologyRules {
    /// Indices of input pins where this node performs a mutable access (Write).
    /// This defines which incoming data streams will be updated in-place.
    fn mutable_input_indices(&self) -> &[usize];

    /// Indices of input pins where this node performs an immutable access (Read).
    /// This defines which incoming data streams will be read without modification.
    fn read_input_indices(&self) -> &[usize];

    /// [Fail-fast] The exact number of input pins required by this node.
    /// Used to validate the node topology at the moment of insertion.
    fn expected_input_count(&self) -> usize;
}

// ==========================================
// 2. Generic Node Structure
// ==========================================

pub struct Node<Op: TopologyRules> {
    pub op: Op,
    pub inputs: Vec<Index>,
    /// A generic field for the user to freely associate an ID from their own
    /// data arena (payload) during the execution phase. The engine routes this
    /// internally without needing to know the underlying data type.
    pub payload_id: Option<Index>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

// ==========================================
// 3. Generic DAG Engine (Compiler & Prover)
// ==========================================

pub struct OrdagEngine<Op: TopologyRules> {
    pub nodes: OrdexArena<Node<Op>>,
    pub all_nodes: Vec<Index>,
    pub execution_layers: Vec<Vec<Index>>,
    is_dirty: bool,
}

impl<Op: TopologyRules> OrdagEngine<Op> {
    pub fn new() -> Self {
        Self {
            nodes: OrdexArena::new(),
            all_nodes: Vec::new(),
            execution_layers: Vec::new(),
            is_dirty: false,
        }
    }

    pub fn add_node(&mut self, op: Op, inputs: Vec<Index>) -> Index {
        // [Fail-fast] Validate the node's shape (input count).
        assert_eq!(
            inputs.len(),
            op.expected_input_count(),
            "Critical Error: Input count mismatch for the given operation."
        );

        // [Fail-fast] Prevent references to non-existent nodes to ensure graph integrity.
        for &input_id in &inputs {
            assert!(
                self.nodes.get(input_id).is_some(),
                "Critical Error: Input node {:?} does not exist.",
                input_id
            );
        }

        let id = self.nodes.insert(Node {
            op,
            inputs,
            payload_id: None,
        });
        self.all_nodes.push(id);
        self.is_dirty = true;
        self.execution_layers.clear();
        id
    }

    /// Analyzes the DAG in O(N+E) and builds execution layers with mathematically proven safety.
    /// It tracks stream origins and emulates Rust's borrow checker rules statically.
    fn build_execution_layers(&mut self) {
        let mut in_degree: HashMap<_, usize> = HashMap::new();
        let mut children: HashMap<_, Vec<Index>> = HashMap::new();

        // 1. Build adjacency list for the graph
        for &node_id in &self.all_nodes {
            let node = self.nodes.get(node_id).expect("Node missing");
            in_degree.insert(node_id.index, node.inputs.len());
            for &parent_id in &node.inputs {
                children.entry(parent_id.index).or_default().push(node_id);
            }
        }

        // 2. Topological sort using Kahn's algorithm
        let mut layers = Vec::new();
        let mut current_layer: Vec<Index> = self
            .all_nodes
            .iter()
            .filter(|&&id| in_degree[&id.index] == 0)
            .copied()
            .collect();

        let mut processed_count = 0;

        while !current_layer.is_empty() {
            processed_count += current_layer.len();
            layers.push(current_layer.clone());

            let mut next_layer = Vec::new();
            for node_id in current_layer {
                if let Some(node_children) = children.get(&node_id.index) {
                    for &child_id in node_children {
                        let degree = in_degree.get_mut(&child_id.index).unwrap();
                        *degree -= 1;
                        if *degree == 0 {
                            next_layer.push(child_id);
                        }
                    }
                }
            }
            current_layer = next_layer;
        }

        assert_eq!(
            processed_count,
            self.all_nodes.len(),
            "Compile Error: Cycle detected in node graph. Must be a strictly acyclic DAG."
        );

        // 3. Static Alias Proof: Track Stream IDs and enforce Read/Write exclusivity
        let mut stream_origins: HashMap<_, _> = HashMap::new();

        for layer in &layers {
            let mut accesses: HashMap<_, AccessMode> = HashMap::new();

            for &node_id in layer {
                let node = self.nodes.get(node_id).unwrap();

                // A. Validate Write (mutation) requests
                for &input_idx in node.op.mutable_input_indices() {
                    let target_id = node.inputs[input_idx];
                    let stream_id = stream_origins[&target_id.index];
                    if let Some(_existing) = accesses.insert(stream_id, AccessMode::Write) {
                        panic!(
                            "Compile Error: Structural Alias Detected (Write-Write or Read-Write conflict). Multiple accesses to Stream ID {:?} in the same execution layer. Transaction aborted.",
                            stream_id
                        );
                    }
                }

                // B. Validate Read (immutable) requests
                for &input_idx in node.op.read_input_indices() {
                    let target_id = node.inputs[input_idx];
                    let stream_id = stream_origins[&target_id.index];
                    match accesses.get(&stream_id) {
                        Some(AccessMode::Write) => panic!(
                            "Compile Error: Structural Alias Detected (Read-Write conflict). Read and Write accesses to Stream ID {:?} in the same execution layer. Transaction aborted.",
                            stream_id
                        ),
                        _ => {
                            accesses.insert(stream_id, AccessMode::Read);
                        }
                    }
                }

                // C. Record stream propagation
                let mut_indices = node.op.mutable_input_indices();
                if mut_indices.is_empty() {
                    // Generator node: Becomes the origin of a new stream
                    stream_origins.insert(node_id.index, node_id.index);
                } else {
                    // Mutator node: Inherits the Stream ID from its parent to enable in-place updates
                    let parent_id = node.inputs[mut_indices[0]];
                    let parent_stream = stream_origins[&parent_id.index];
                    stream_origins.insert(node_id.index, parent_stream);
                }
            }
        }

        self.execution_layers = layers;
    }

    /// Compiles the graph and returns a reference to the safely executable layers.
    /// Re-compilation only occurs if the graph topology has been modified (is_dirty).
    pub fn compile(&mut self) -> &[Vec<Index>] {
        if self.is_dirty {
            self.build_execution_layers();
            self.is_dirty = false;
        }
        &self.execution_layers
    }
}

impl<Op: TopologyRules> Default for OrdagEngine<Op> {
    fn default() -> Self {
        Self::new()
    }
}
