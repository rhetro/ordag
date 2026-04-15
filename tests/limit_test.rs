mod common;
use common::*;
use ordag::prelude::*;
use std::time::Instant;

#[test]
fn test_limit_deep_chain() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let mut buffers = OrdexArena::<TestBuffer>::new();
    const CHAIN_LENGTH: usize = 10_000;

    let mut current_node = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 1,
        },
        vec![],
    );

    for _ in 0..CHAIN_LENGTH {
        current_node = engine.add_node(
            TestOp::Transform {
                translation: [0.001, 0.0, 0.0],
            },
            vec![current_node],
        );
    }

    let start = Instant::now();

    engine.compile();

    execute_test_layers(&mut engine.nodes, &mut buffers, &engine.execution_layers);

    let duration = start.elapsed();

    let final_node = engine.nodes.get(current_node).unwrap();
    let buf_id = final_node.payload_id.unwrap();
    let buf = buffers.get(buf_id).unwrap();

    let x_pos = buf.positions[0][0];

    println!("---------------------------------------");
    println!("--- [Limit Test 1: Deep Chain] ---");
    println!("Elapsed Time : {:?}", duration);
    println!("Final X Pos  : {:.4} (Expected: ~11.0)", x_pos);
    println!("---------------------------------------");

    assert!((x_pos - 11.0).abs() < 0.1);
}

#[test]
fn test_limit_massive_reads() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let mut buffers = OrdexArena::<TestBuffer>::new();
    const FAN_OUT_SIZE: usize = 10_000;

    let source_a = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 10,
        },
        vec![],
    );
    let mut merge_nodes = Vec::with_capacity(FAN_OUT_SIZE);

    for _ in 0..FAN_OUT_SIZE {
        let source_b = engine.add_node(
            TestOp::MeshCircle {
                radius: 1.0,
                vertices: 3,
            },
            vec![],
        );
        let merge = engine.add_node(TestOp::Merge, vec![source_b, source_a]);
        merge_nodes.push(merge);
    }

    let start = Instant::now();

    engine.compile();

    execute_test_layers(&mut engine.nodes, &mut buffers, &engine.execution_layers);

    let duration = start.elapsed();

    println!("---------------------------------------");
    println!("--- [Limit Test 2: Massive Reads] ---");
    println!("Elapsed Time : {:?}", duration);

    for &node_id in &[merge_nodes[0], merge_nodes[FAN_OUT_SIZE - 1]] {
        let node = engine.nodes.get(node_id).unwrap();
        let buf_id = node.payload_id.unwrap();
        let buf = buffers.get(buf_id).unwrap();

        println!("Merged buffer Vertices: {}", buf.positions.len());
        assert_eq!(buf.positions.len(), 13);
    }

    println!("---------------------------------------");
}
