mod common;
use common::*;
use ordag::prelude::*;

#[test]
fn test_buffer_pipeline_golden_path() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let mut buffers = OrdexArena::<TestBuffer>::new();

    let node_a = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 4,
        },
        vec![],
    );
    let node_b = engine.add_node(
        TestOp::Transform {
            translation: [10.0, 0.0, 0.0],
        },
        vec![node_a],
    );
    let node_c = engine.add_node(
        TestOp::MeshCircle {
            radius: 2.0,
            vertices: 3,
        },
        vec![],
    );
    let node_d = engine.add_node(TestOp::Merge, vec![node_b, node_c]);

    engine.compile();

    execute_test_layers(
        &mut engine.nodes,
        &engine.input_pool,
        &mut buffers,
        &engine.execution_layers,
    );

    let final_node = engine.nodes.get(node_d).unwrap();
    let final_buf_id = final_node.payload_id.unwrap();
    let final_buf = buffers.get(final_buf_id).unwrap();

    assert_eq!(final_buf.positions.len(), 7, "Vertices should be merged");
    assert!((final_buf.positions[0][0] - 11.0).abs() < f32::EPSILON);
}

#[test]
#[should_panic(expected = "Critical Error: Input node")]
fn test_fail_fast_foreign_input() {
    let mut engine1 = OrdagEngine::<TestOp>::new();
    let fake_node = engine1.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 3,
        },
        vec![],
    );

    let mut engine2 = OrdagEngine::<TestOp>::new();
    engine2.add_node(
        TestOp::Transform {
            translation: [1.0, 0.0, 0.0],
        },
        vec![fake_node],
    );
}

#[test]
#[should_panic(
    expected = "Compile Error: Structural Alias Detected (Write-Write or Read-Write conflict)"
)]
fn test_compile_error_write_write_conflict() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let circle = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 3,
        },
        vec![],
    );

    engine.add_node(
        TestOp::Transform {
            translation: [1.0, 0.0, 0.0],
        },
        vec![circle],
    );
    engine.add_node(
        TestOp::Transform {
            translation: [0.0, 1.0, 0.0],
        },
        vec![circle],
    );
    engine.compile();
}

#[test]
#[should_panic(expected = "Compile Error: Structural Alias Detected (Read-Write conflict)")]
fn test_compile_error_read_write_conflict() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let circle1 = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 3,
        },
        vec![],
    );
    let circle2 = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 3,
        },
        vec![],
    );

    engine.add_node(
        TestOp::Transform {
            translation: [1.0, 0.0, 0.0],
        },
        vec![circle1],
    );
    engine.add_node(TestOp::Merge, vec![circle2, circle1]);
    engine.compile();
}

#[test]
#[should_panic(expected = "Compile Error: Structural Alias Detected")]
fn test_compile_error_self_alias() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let circle = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 3,
        },
        vec![],
    );
    engine.add_node(TestOp::Merge, vec![circle, circle]);
    engine.compile();
}

#[test]
#[should_panic(expected = "Critical Error: Input count mismatch")]
fn test_fail_fast_merge_inputs() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let circle = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 4,
        },
        vec![],
    );
    engine.add_node(TestOp::Merge, vec![circle]);
}
