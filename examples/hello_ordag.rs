use ordag::prelude::*;

// ==========================================
// 1. Your Domain Data (Payload)
// ==========================================
pub struct MyBuffer {
    pub data: Vec<f32>,
}

// ==========================================
// 2. Your Node Operations (Operation)
// ==========================================
pub enum MyOp {
    Generator { value: f32 },
    Multiplier { factor: f32 },
}

// Contract to teach the engine the rules of graph topology.
impl TopologyRules for MyOp {
    fn mutable_input_indices(&self) -> &[usize] {
        match self {
            Self::Generator { .. } => &[],
            Self::Multiplier { .. } => &[0], // Mutably update (in-place) the 0-th input
        }
    }
    fn read_input_indices(&self) -> &[usize] {
        &[] // No read-only pins in this example
    }
    fn expected_input_count(&self) -> usize {
        match self {
            Self::Generator { .. } => 0,
            Self::Multiplier { .. } => 1,
        }
    }
}

// ==========================================
// 3. Execution Loop (Main)
// ==========================================
fn main() {
    let mut engine = OrdagEngine::<MyOp>::new();
    let mut buffers = OrdexArena::<MyBuffer>::new();

    // Build the graph
    let node_a = engine.add_node(MyOp::Generator { value: 10.0 }, vec![]);
    let node_b = engine.add_node(MyOp::Multiplier { factor: 2.0 }, vec![node_a]);

    // Compile the graph (Static alias proof and layer construction)
    engine.compile();

    // Execute layer by layer in a mathematically proven safe order
    for layer in &engine.execution_layers {
        let mut multi_batch = Vec::new();

        for &node_id in layer {
            enum OpCommand {
                Generator(f32),
                Multiplier(f32, Index),
            }

            let cmd = {
                let node = engine.nodes.get(node_id).unwrap();
                match &node.op {
                    MyOp::Generator { value } => OpCommand::Generator(*value),
                    MyOp::Multiplier { factor } => OpCommand::Multiplier(*factor, node.inputs[0]),
                }
            };

            let batch_push = match cmd {
                OpCommand::Generator(value) => {
                    // Data generation and routing
                    let buf_id = buffers.insert(MyBuffer { data: vec![value] });
                    engine.nodes.get_mut(node_id).unwrap().payload_id = Some(buf_id);
                    None
                }
                OpCommand::Multiplier(factor, parent_id) => {
                    let mut bp = None;

                    // Inherit IDs and extract batch commands
                    align!(
                        engine.nodes,
                        node_id,
                        parent_id,
                        |target_node, parent_node| {
                            let buf_id = parent_node.payload_id.unwrap();
                            target_node.payload_id = Some(buf_id);
                            bp = Some((buf_id, factor));
                        }
                    );
                    bp
                }
            };

            if let Some(bp) = batch_push {
                multi_batch.push(bp);
            }
        }

        // Perform zero-cost, in-place batch updates using Ordex
        if !multi_batch.is_empty() {
            multi_batch.sort_unstable_by_key(|a| a.0.index);
            let targets: Vec<Index> = multi_batch.iter().map(|(idx, _)| *idx).collect();
            let verified = VerifiedIndices::new(targets);
            let factors: Vec<_> = multi_batch.into_iter().map(|(_, f)| f).collect();

            buffers.ordex(&verified, move |mut iter| {
                for factor in factors {
                    let buf = iter.next().unwrap();
                    for val in &mut buf.data {
                        *val *= factor;
                    }
                }
            });
        }
    }

    // Verify the results
    let final_node = engine.nodes.get(node_b).unwrap();
    let final_buf = buffers.get(final_node.payload_id.unwrap()).unwrap();

    println!("Final Result: {:?}", final_buf.data); // Expected to be [20.0]
}
