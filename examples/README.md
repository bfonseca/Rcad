# Examples

This directory contains example models and scripts demonstrating rcad capabilities.

## Models

### box_demo.rs
Simple box creation using the OCCT backend directly.
```bash
cargo run --example box_demo
```
Outputs: `examples/models/box_output.stl`

### sketch_demo.rs
2D sketch with constraints (rectangle with center hole), solved and extruded.
```bash
cargo run --example sketch_demo
```
Outputs: `examples/models/sketch_extrude.stl`

### assembly_demo.rs
Multi-part assembly with mates (constraints between parts).
```bash
cargo run --example assembly_demo
```
Outputs: `examples/models/assembly_output.step`

### foundation_bench.rs
Foundation baseline benchmark: builds the demo and a medium plate (120×80×10,
8 holes, rounds), measures cold/incremental rebuild, a second tessellation and
the mesh size, and saves both documents for screenshots.
```bash
cargo run --release --example foundation_bench
```
Optional environment variables: `RCAD_BENCH_RUNS=<n>`, `RCAD_BENCH_ROUNDS=0`,
`RCAD_BENCH_SCALING=1` (also measures 0/2/4-hole plates).
Outputs: `examples/models/foundation_demo.rcad` and
`examples/models/foundation_plate.rcad`, plus `metric,...` lines on stdout.
See `plans/foundation-baseline.md` for the recorded numbers.

## Running Examples

```bash
# Run a specific example
cargo run --example box_demo

# Run all examples
cargo run --example box_demo
cargo run --example sketch_demo
cargo run --example assembly_demo
cargo run --release --example foundation_bench

# Run with release optimizations
cargo run --release --example box_demo
```

## Output Files

All examples generate output files in `examples/models/`:
- STL files for 3D printing / mesh viewing
- STEP files for CAD exchange

## Creating Your Own Examples

1. Copy an existing example as a template
2. Modify the geometry creation code
3. Add to the examples list in the workspace Cargo.toml (if needed)
4. Run with `cargo run --example your_example`

## Example Structure

Each example demonstrates a specific workflow:
- **Direct API usage**: Using rcad crates programmatically
- **Constraint solving**: Sketch constraints and assembly mates
- **Export**: STL and STEP export
- **Tessellation**: Converting B-rep to triangle meshes

## Notes

- Examples require the `rcad-occt` feature (enabled by default)
- Some examples may need additional dependencies (add to example's Cargo.toml if needed)
- Output files are overwritten on each run