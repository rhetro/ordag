# Ordag: Secure & High-Performance DAG Execution Engine

Ordag is a memory-efficient, safety-focused Directed Acyclic Graph (DAG) execution engine built in Rust, powered by the `ordex` crate. 

Its defining feature is the **Static Alias Proof** performed during graph compilation. By mathematically proving the absence of data conflicts (Write-Write, Read-Write) before execution, Ordag provides a perfectly safe "execution plan" without incurring the runtime overhead of Rust's borrow checker or complex locking mechanisms.

## Core Philosophy

1. **Mathematical Certainty**: Utilizing Kahn's algorithm and Stream ID tracking, Ordag detects structural aliases at compile-time. This eliminates data race risks and ensures that the execution layers returned by the engine are 100% mathematically safe to execute.
2. **Infrastructure for Any Execution**: Ordag takes responsibility *only* up to generating the execution plan (layers). The actual execution loop lives entirely in the user's space. This gives you the ultimate freedom to choose the best strategy for your domain: single-threaded cache optimization, multi-threaded parallelization, or GPU task dispatching.
3. **Hardware Saturation**: Thanks to `ordex`'s contiguous memory layout, Ordag serves as a top-tier fast general-purpose arena. Furthermore, under specific conditions (single-threaded, in-place mutation), it unlocks extreme performance that pushes the physical limits of hardware bandwidth.

## Quick Start

Add Ordag to your `Cargo.toml`:

```toml
[dependencies]
ordag = "0.1.1"
```

Run the built-in template to see how to define your domain, compile the graph, and write the execution loop:

```bash
cargo run --example hello_ordag
```

## Usage: The Anatomy of Static Proof

Ordag requires you to define the rules of your domain up front. Once the topology is mathematically proven during compilation, the execution loop is handed back to you, completely free of runtime checks.

```rust
use ordag::prelude::*;

// 1. Define your domain operations
pub enum MyOp {
    Generator { value: f32 },
    Multiplier { factor: f32 },
}

// 2. Teach the engine your structural rules (The Contract)
impl TopologyRules for MyOp {
    fn mutable_input_indices(&self) -> &[usize] {
        match self {
            Self::Generator { .. } => &[],
            Self::Multiplier { .. } => &[0], // Mutates the 0-th input in-place
        }
    }
    fn read_input_indices(&self) -> &[usize] { &[] }
    fn expected_input_count(&self) -> usize {
        match self {
            Self::Generator { .. } => 0,
            Self::Multiplier { .. } => 1,
        }
    }
}

fn main() {
    let mut engine = OrdagEngine::<MyOp>::new();

    // 3. Build the graph topology
    let node_a = engine.add_node(MyOp::Generator { value: 10.0 }, vec![]);
    let node_b = engine.add_node(MyOp::Multiplier { factor: 2.0 }, vec![node_a]);

    // 4. Compile: Mathematically prove the absence of structural aliases
    // If there is a Read-Write or Write-Write conflict, it panics here.
    engine.compile(); 

    // 5. Execute: The engine yields perfectly safe layers to process
    for layer in &engine.execution_layers {
        for &node_id in layer {
            // Execute your domain logic here using `ordex` or any custom strategy.
            // Ordag guarantees that nodes in the same layer have absolutely NO data conflicts.
        }
    }
}
```

## Two-Tier Performance Path

Ordag adapts to your performance requirements, offering two distinct paths:

### Tier 1: General Purpose (Safety & Base Speed)
For complex node-based systems, Ordag eliminates data conflict bugs entirely. Even in multi-threaded environments, the engine proves "which nodes can run simultaneously," serving as an incredibly robust foundation for safe parallel processing with minimal synchronization overhead.

### Tier 2: Extreme Performance (Lock-Free Saturation)
By combining single-threaded execution with the `ordex` crate's `align!` macro and in-place mutations, you can completely eliminate locks and cache contention. This path fully saturates the CPU's L1/L2 cache bandwidth, allowing you to execute large contiguous data transforms and high-density simulation loops in mere milliseconds with zero allocations (e.g., audio sample buffers, procedural geometry pipelines, particle simulation buffers).

## Performance Limits (Reference)

Benchmarked in Release mode (`opt-level = 3`, `lto = "fat"`):

**1. Deep Chain (10,001 nodes — 10,000 sequential in-place mutations)**
- **Build Phase (Compile & Prove)**: ~8.8 ms (`O(N+E)` static analysis)
- **Execute Phase (Extreme Path)**: ~344 µs (~34 ns per node, zero-allocation layer traversal)

**2. Wide / Fan-Out DAG (20,000 nodes — 10,000 parallel streams)**
- **Build Phase (Compile & Prove)**: ~9.3 ms (`O(N+E)` static analysis)
- **Execute Phase (Batch Routing via `ordex` v0.3.0)**: ~1.06 ms (~53 ns per node, including buffer allocation and strict ascending batch mutation)

## Usage Contract
- **Append-Only**: Nodes can only be added. To restructure the graph, drop the `OrdagEngine` and rebuild it.
- **Fail-Fast**: Invalid inputs, cyclic dependencies, or structural aliases will cause an immediate, unrecoverable panic at compile-time. We do not return graceful errors for structural flaws; we abort to guarantee safety.

## License

This project is licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
* MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

at your option.
