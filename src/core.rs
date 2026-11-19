use ordex::prelude::*;
use std::num::NonZero;

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
    /// Start offset into `OrdagEngine::input_pool` for this node's input indices.
    pub input_start: u32,
    /// Number of input pins stored in `OrdagEngine::input_pool`.
    pub input_len: u32,
    /// A generic field for the user to freely associate an ID from their own
    /// data arena (payload) during the execution phase. The engine routes this
    /// internally without needing to know the underlying data type.
    pub payload_id: Option<Index>,
}

impl<Op: TopologyRules> Node<Op> {
    /// Returns a zero-copy slice of this node's input indices from the shared pool.
    #[inline(always)]
    pub fn inputs<'a>(&self, pool: &'a [Index]) -> &'a [Index] {
        let start = self.input_start as usize;
        let end = start + self.input_len as usize;
        &pool[start..end]
    }

    /// Returns a single input `Index` at `pin` from the shared pool.
    #[inline(always)]
    pub fn pin(&self, pool: &[Index], pin: usize) -> Index {
        debug_assert!(pin < self.input_len as usize);
        pool[self.input_start as usize + pin]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

// ==========================================
// 3. Flat Build Workspace (CSR + Epoch)
// ==========================================

struct BuildWorkspace {
    in_degree: Vec<u32>,
    csr_offsets: Vec<u32>,
    write_cursor: Vec<u32>,
    csr_edges: Vec<Index>,
    stream_origins: Vec<u32>,
    access_epoch: Vec<u32>,
    access_mode: Vec<AccessMode>,
    current_epoch: u32,
}

impl BuildWorkspace {
    fn new() -> Self {
        Self {
            in_degree: Vec::new(),
            csr_offsets: Vec::new(),
            write_cursor: Vec::new(),
            csr_edges: Vec::new(),
            stream_origins: Vec::new(),
            access_epoch: Vec::new(),
            access_mode: Vec::new(),
            current_epoch: 0,
        }
    }

    #[inline]
    fn prepare(&mut self, node_count: usize, edge_count: usize) {
        let dummy_index = Index {
            index: 0,
            generation: NonZero::new(1).unwrap(),
        };

        self.in_degree.clear();
        self.in_degree.resize(node_count, 0);

        self.csr_offsets.clear();
        self.csr_offsets.resize(node_count + 1, 0);

        self.csr_edges.clear();
        self.csr_edges.resize(edge_count, dummy_index);

        self.stream_origins.clear();
        self.stream_origins.resize(node_count, 0);

        if self.access_epoch.len() < node_count {
            self.access_epoch.resize(node_count, 0);
            self.access_mode.resize(node_count, AccessMode::Read);
        }
    }

    #[inline(always)]
    fn next_epoch(&mut self) -> u32 {
        self.current_epoch = self.current_epoch.wrapping_add(1);
        if self.current_epoch == 0 {
            self.access_epoch.fill(0);
            self.current_epoch = 1;
        }
        self.current_epoch
    }
}

// ==========================================
// 4. Generic DAG Engine (Compiler & Prover)
// ==========================================

pub struct OrdagEngine<Op: TopologyRules> {
    pub nodes: OrdexArena<Node<Op>>,
    pub input_pool: Vec<Index>,
    pub all_nodes: Vec<Index>,
    pub execution_layers: Vec<Vec<Index>>,
    ws: BuildWorkspace,
    is_dirty: bool,
}

impl<Op: TopologyRules> OrdagEngine<Op> {
    pub fn new() -> Self {
        Self {
            nodes: OrdexArena::new(),
            input_pool: Vec::new(),
            all_nodes: Vec::new(),
            execution_layers: Vec::new(),
            ws: BuildWorkspace::new(),
            is_dirty: false,
        }
    }

    pub fn add_node<I: AsRef<[Index]>>(&mut self, op: Op, inputs: I) -> Index {
        let inputs_slice = inputs.as_ref();

        // [Fail-fast] Validate the node's shape (input count).
        assert_eq!(
            inputs_slice.len(),
            op.expected_input_count(),
            "Critical Error: Input count mismatch for the given operation."
        );

        // [Fail-fast] Prevent references to non-existent nodes to ensure graph integrity.
        for &input_id in inputs_slice {
            assert!(
                self.nodes.get(input_id).is_some(),
                "Critical Error: Input node {:?} does not exist.",
                input_id
            );
        }

        let input_start = u32::try_from(self.input_pool.len())
            .expect("OrdagEngine input pool capacity exceeded u32::MAX");
        let input_len =
            u32::try_from(inputs_slice.len()).expect("Node input count exceeded u32::MAX");

        self.input_pool.extend_from_slice(inputs_slice);

        let id = self.nodes.insert(Node {
            op,
            input_start,
            input_len,
            payload_id: None,
        });

        debug_assert_eq!(id.index as usize, self.all_nodes.len());
        self.all_nodes.push(id);
        self.is_dirty = true;
        self.execution_layers.clear();
        id
    }

    /// Returns a slice of input node indices registered for the given `node`.
    #[inline(always)]
    pub fn node_inputs(&self, node: &Node<Op>) -> &[Index] {
        node.inputs(&self.input_pool)
    }

    /// Returns a single input node index at `pin` registered for the given `node`.
    #[inline(always)]
    pub fn node_pin(&self, node: &Node<Op>, pin: usize) -> Index {
        node.pin(&self.input_pool, pin)
    }

    /// Analyzes the DAG in O(N+E) and builds execution layers with mathematically proven safety.
    /// It tracks stream origins and emulates Rust's borrow checker rules statically.
    fn build_execution_layers(&mut self) {
        let n = self.all_nodes.len();
        if n == 0 {
            self.execution_layers.clear();
            return;
        }

        let total_edges = self.input_pool.len();
        self.ws.prepare(n, total_edges);

        // 1. Build adjacency list for the graph
        for (idx, &node_id) in self.all_nodes.iter().enumerate() {
            let node = self.nodes.get(node_id).expect("Node missing");
            let inputs = node.inputs(&self.input_pool);
            self.ws.in_degree[idx] = inputs.len() as u32;

            for &parent_id in inputs {
                let p_idx = parent_id.index as usize;
                self.ws.csr_offsets[p_idx + 1] += 1;
            }
        }

        for i in 0..n {
            self.ws.csr_offsets[i + 1] += self.ws.csr_offsets[i];
        }

        self.ws.write_cursor.clear();
        self.ws
            .write_cursor
            .extend_from_slice(&self.ws.csr_offsets[..n]);

        for &node_id in &self.all_nodes {
            let node = self.nodes.get(node_id).expect("Node missing");
            for &parent_id in node.inputs(&self.input_pool) {
                let p_idx = parent_id.index as usize;
                let pos = self.ws.write_cursor[p_idx] as usize;
                self.ws.csr_edges[pos] = node_id;
                self.ws.write_cursor[p_idx] = (pos + 1) as u32;
            }
        }

        // 2. Topological sort using Kahn's algorithm
        let mut layers = Vec::new();
        let mut current_layer: Vec<Index> = self
            .all_nodes
            .iter()
            .enumerate()
            .filter(|&(i, _)| self.ws.in_degree[i] == 0)
            .map(|(_, &id)| id)
            .collect();

        let mut processed_count = 0;

        while !current_layer.is_empty() {
            processed_count += current_layer.len();
            layers.push(current_layer.clone());

            let mut next_layer = Vec::new();
            for node_id in current_layer {
                let p_idx = node_id.index as usize;
                let start = self.ws.csr_offsets[p_idx] as usize;
                let end = self.ws.csr_offsets[p_idx + 1] as usize;

                for &child_id in &self.ws.csr_edges[start..end] {
                    let c_idx = child_id.index as usize;
                    self.ws.in_degree[c_idx] -= 1;
                    if self.ws.in_degree[c_idx] == 0 {
                        next_layer.push(child_id);
                    }
                }
            }
            current_layer = next_layer;
        }

        assert_eq!(
            processed_count, n,
            "Compile Error: Cycle detected in node graph. Must be a strictly acyclic DAG."
        );

        // 3. Static Alias Proof: Track Stream IDs and enforce Read/Write exclusivity
        for layer in &layers {
            let epoch = self.ws.next_epoch();

            for &node_id in layer {
                let node = self.nodes.get(node_id).unwrap();
                let inputs = node.inputs(&self.input_pool);

                // A. Validate Write (mutation) requests
                for &input_idx in node.op.mutable_input_indices() {
                    let target_id = inputs[input_idx];
                    let stream_id = self.ws.stream_origins[target_id.index as usize] as usize;
                    if self.ws.access_epoch[stream_id] == epoch {
                        panic!(
                            "Compile Error: Structural Alias Detected (Write-Write or Read-Write conflict). Multiple accesses to Stream ID {} in the same execution layer. Transaction aborted.",
                            stream_id
                        );
                    }
                    self.ws.access_epoch[stream_id] = epoch;
                    self.ws.access_mode[stream_id] = AccessMode::Write;
                }

                // B. Validate Read (immutable) requests
                for &input_idx in node.op.read_input_indices() {
                    let target_id = inputs[input_idx];
                    let stream_id = self.ws.stream_origins[target_id.index as usize] as usize;
                    if self.ws.access_epoch[stream_id] == epoch {
                        if self.ws.access_mode[stream_id] == AccessMode::Write {
                            panic!(
                                "Compile Error: Structural Alias Detected (Read-Write conflict). Read and Write accesses to Stream ID {} in the same execution layer. Transaction aborted.",
                                stream_id
                            );
                        }
                    } else {
                        self.ws.access_epoch[stream_id] = epoch;
                        self.ws.access_mode[stream_id] = AccessMode::Read;
                    }
                }

                // C. Record stream propagation
                let mut_indices = node.op.mutable_input_indices();
                if mut_indices.is_empty() {
                    // Generator node: Becomes the origin of a new stream
                    self.ws.stream_origins[node_id.index as usize] = node_id.index;
                } else {
                    // Mutator node: Inherits the Stream ID from its parent to enable in-place updates
                    let parent_id = inputs[mut_indices[0]];
                    let parent_stream = self.ws.stream_origins[parent_id.index as usize];
                    self.ws.stream_origins[node_id.index as usize] = parent_stream;
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
