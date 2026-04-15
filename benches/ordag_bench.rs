use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ordag::prelude::*;

// Import the shared test module from the benchmark directory.
#[path = "../tests/common/mod.rs"]
mod common;
use common::{execute_test_layers, TestBuffer, TestOp};

fn bench_build_phase(c: &mut Criterion) {
    c.bench_function("build_10k_deep_chain", |b| {
        b.iter(|| {
            let mut engine = OrdagEngine::<TestOp>::new();
            let mut current_node = engine.add_node(
                TestOp::MeshCircle {
                    radius: 1.0,
                    vertices: 1,
                },
                vec![],
            );

            for _ in 0..10_000 {
                current_node = engine.add_node(
                    TestOp::Transform {
                        translation: [0.001, 0.0, 0.0],
                    },
                    vec![current_node],
                );
            }
            // Measure only the compile phase (static verification).
            black_box(engine.compile());
        })
    });
}

fn bench_execute_phase(c: &mut Criterion) {
    let mut engine = OrdagEngine::<TestOp>::new();
    let mut buffers = OrdexArena::<TestBuffer>::new();

    let mut current_node = engine.add_node(
        TestOp::MeshCircle {
            radius: 1.0,
            vertices: 1,
        },
        vec![],
    );
    for _ in 0..10_000 {
        current_node = engine.add_node(
            TestOp::Transform {
                translation: [0.001, 0.0, 0.0],
            },
            vec![current_node],
        );
    }

    // Compile once to finalize the execution layers.
    let _ = engine.compile();

    c.bench_function("execute_10k_deep_chain", |b| {
        b.iter(|| {
            // Measure only the user‑space execution loop using the pre‑compiled layers. (For benchmarking, the layers are assumed to be passed by cloning each time.)
            execute_test_layers(&mut engine.nodes, &mut buffers, &engine.execution_layers);
            black_box(&buffers);
        })
    });
}

criterion_group!(benches, bench_build_phase, bench_execute_phase);
criterion_main!(benches);
