use ordag::prelude::*;
use std::f32::consts::PI;

pub struct TestBuffer {
    pub positions: Vec<[f32; 3]>,
}

#[allow(dead_code)]
pub enum TestOp {
    MeshCircle { radius: f32, vertices: usize },
    Transform { translation: [f32; 3] },
    Merge,
}

impl TopologyRules for TestOp {
    fn mutable_input_indices(&self) -> &[usize] {
        match self {
            Self::MeshCircle { .. } => &[],
            Self::Transform { .. } => &[0],
            Self::Merge => &[0],
        }
    }
    fn read_input_indices(&self) -> &[usize] {
        match self {
            Self::Merge => &[1],
            _ => &[],
        }
    }
    fn expected_input_count(&self) -> usize {
        match self {
            Self::MeshCircle { .. } => 0,
            Self::Transform { .. } => 1,
            Self::Merge => 2,
        }
    }
}

pub fn execute_test_layers(
    nodes: &mut OrdexArena<Node<TestOp>>,
    buffers: &mut OrdexArena<TestBuffer>,
    layers: &[Vec<Index>],
) {
    let mut transform_batch: Vec<(Index, [f32; 3])> = Vec::new();
    let mut targets: Vec<Index> = Vec::new();
    let mut verified = VerifiedIndices::new(Vec::new());

    for layer in layers {
        transform_batch.clear();

        for &node_id in layer {
            enum OpCommand {
                MeshCircle(f32, usize),
                Transform([f32; 3], Index),
                Merge(Index, Index),
            }

            let cmd = {
                let node = nodes.get(node_id).unwrap();
                match &node.op {
                    TestOp::MeshCircle { radius, vertices } => {
                        OpCommand::MeshCircle(*radius, *vertices)
                    }
                    TestOp::Transform { translation } => {
                        OpCommand::Transform(*translation, node.inputs[0])
                    }
                    TestOp::Merge => OpCommand::Merge(node.inputs[0], node.inputs[1]),
                }
            };

            let batch_push = match cmd {
                OpCommand::MeshCircle(radius, vertices) => {
                    let mut positions = Vec::with_capacity(vertices);
                    for i in 0..vertices {
                        let angle = (i as f32 / vertices as f32) * 2.0 * PI;
                        positions.push([angle.cos() * radius, angle.sin() * radius, 0.0]);
                    }
                    let buf_id = buffers.insert(TestBuffer { positions });
                    nodes.get_mut(node_id).unwrap().payload_id = Some(buf_id);
                    None
                }
                OpCommand::Transform(translation, parent_id) => {
                    let mut bp = None;
                    align!(nodes, node_id, parent_id, |target_node, parent_node| {
                        let buf_id = parent_node.payload_id.unwrap();
                        target_node.payload_id = Some(buf_id);
                        bp = Some((buf_id, translation));
                    });
                    bp
                }
                OpCommand::Merge(src1, src2) => {
                    align!(
                        nodes,
                        node_id,
                        src1,
                        src2,
                        |target_node, parent1, parent2| {
                            let target_buf_id = parent1.payload_id.unwrap();
                            let src_buf_id = parent2.payload_id.unwrap();
                            target_node.payload_id = Some(target_buf_id);

                            align!(buffers, target_buf_id, src_buf_id, |target_buf, src_buf| {
                                target_buf.positions.extend_from_slice(&src_buf.positions);
                            });
                        }
                    );
                    None
                }
            };

            if let Some(bp) = batch_push {
                transform_batch.push(bp);
            }
        }

        if !transform_batch.is_empty() {
            transform_batch.sort_unstable_by_key(|a| a.0.index);

            targets.clear();
            targets.extend(transform_batch.iter().map(|(idx, _)| *idx));
            verified.clear_and_verify(&targets);

            let mut batch_iter = transform_batch.iter();
            buffers.ordex(&verified, |iter| {
                for buf in iter {
                    let (_, translation) = batch_iter.next().unwrap();
                    for pos in &mut buf.positions {
                        pos[0] += translation[0];
                        pos[1] += translation[1];
                        pos[2] += translation[2];
                    }
                }
            });
        }
    }
}
