mod common;
use common::*;
use ordag::prelude::*;

/// [Boundary Test 1] Geometry with zero vertices
/// Verify that generating a MeshCircle with zero vertices and applying a Transform does not crash.
/// Proves that the user‑space execution loop (execute_test_layers) and Ordex behave safely on empty collections.
#[test]
fn test_boundary_zero_vertices() {
    let mut engine = OrdagEngine::<TestOp>::new();
    let mut buffers = OrdexArena::<TestBuffer>::new();

    let circle = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 0,
        },
        vec![],
    );
    let transform = engine.add_node(
        TestOp::Transform {
            translation: [1.0, 1.0, 1.0],
        },
        vec![circle],
    );

    // Static verification and layer generation by the engine.
    engine.compile();
    // Execution loop in user space.
    execute_test_layers(&mut engine.nodes, &mut buffers, &engine.execution_layers);

    let final_node = engine.nodes.get(transform).unwrap();
    let buf_id = final_node.payload_id.unwrap();
    let buf = buffers.get(buf_id).unwrap();

    // Ensure that processing completes normally with zero vertices.
    assert_eq!(buf.positions.len(), 0);
}

/// [Boundary Test 2] Empty engine
/// Verify that compiling with no nodes added returns an empty layer immediately without panicking.
#[test]
fn test_boundary_empty_engine() {
    let mut engine = OrdagEngine::<TestOp>::new();

    // Call compile() instead of the previous evaluate_all().
    let layers = engine.compile();

    assert!(layers.is_empty());
}
