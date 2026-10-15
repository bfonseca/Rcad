//! Example: 2D Sketch with constraints
//!
//! Run with: cargo run --example sketch_demo

use rcad_sketch::{Sketch, Constraint};
use rcad_occt::OcctBackend;
use rcad_geom::GeometryBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("rcad - Sketch Demo");
    println!("===================");
    
    // Create a new sketch
    let mut sketch = Sketch::new("DemoSketch");
    
    // Add points for a rectangle with a hole
    let p1 = sketch.add_point(-20.0, -15.0);  // Bottom-left
    let p2 = sketch.add_point(20.0, -15.0);   // Bottom-right
    let p3 = sketch.add_point(20.0, 15.0);    // Top-right
    let p4 = sketch.add_point(-20.0, 15.0);   // Top-left
    
    // Add rectangle edges
    sketch.add_line(p1, p2);  // Bottom
    sketch.add_line(p2, p3);  // Right
    sketch.add_line(p3, p4);  // Top
    sketch.add_line(p4, p1);  // Left
    
    // Add a circle in the center (using arc with full 360 degrees)
    let center = sketch.add_point(0.0, 0.0);
    sketch.add_arc(0.0, 0.0, 5.0, 0.0, std::f64::consts::TAU);
    
    println!("Created sketch with:");
    println!("  Points: {}", sketch.points.len());
    println!("  Lines: {}", sketch.lines.len());
    println!("  Arcs: {}", sketch.arcs.len());
    println!("  Constraints: {}", sketch.constraints.len());
    
    // Add constraints directly to the constraints vector
    // Horizontal constraints for bottom and top edges
    sketch.constraints.push(Constraint::Horizontal { line: sketch.lines[0].id });
    sketch.constraints.push(Constraint::Horizontal { line: sketch.lines[2].id });
    
    // Vertical constraints for left and right edges
    sketch.constraints.push(Constraint::Vertical { line: sketch.lines[1].id });
    sketch.constraints.push(Constraint::Vertical { line: sketch.lines[3].id });
    
    // Equal length for opposite sides
    sketch.constraints.push(Constraint::EqualLength { a: sketch.lines[0].id, b: sketch.lines[2].id });
    sketch.constraints.push(Constraint::EqualLength { a: sketch.lines[1].id, b: sketch.lines[3].id });
    
    // Coincident constraints for rectangle corners
    sketch.constraints.push(Constraint::Coincident { a: sketch.lines[0].id, b: sketch.lines[1].id });
    sketch.constraints.push(Constraint::Coincident { a: sketch.lines[1].id, b: sketch.lines[2].id });
    sketch.constraints.push(Constraint::Coincident { a: sketch.lines[2].id, b: sketch.lines[3].id });
    sketch.constraints.push(Constraint::Coincident { a: sketch.lines[3].id, b: sketch.lines[0].id });
    
    // Circle center at origin (fixed)
    sketch.constraints.push(Constraint::Fixed { point: center });
    sketch.constraints.push(Constraint::Coincident { a: center, b: center });
    
    println!("  Total constraints after adding: {}", sketch.constraints.len());
    
    // Solve the sketch
    println!("\nSolving constraints...");
    match sketch.solve() {
        Ok(_) => println!("Sketch solved successfully!"),
        Err(e) => println!("Solver error: {}", e),
    }
    
    // Print final positions
    println!("\nFinal point positions:");
    for point in &sketch.points {
        println!("  Point {}: ({:.2}, {:.2}) fixed={}", point.id, point.x, point.y, point.fixed);
    }
    
    // Convert to profile for 3D operations
    if let Some(profile) = sketch.to_profile() {
        println!("\nProfile created with outer loop and {} holes", profile.holes.len());
        
        // Extrude using OCCT
        extrude_sketch(&profile)?;
    } else {
        println!("\nCould not create profile from sketch");
    }
    
    println!("\nDone!");
    Ok(())
}

fn extrude_sketch(profile: &rcad_geom::Profile2D) -> Result<(), Box<dyn std::error::Error>> {
    use rcad_geom::GeometryBackend;

    let mut backend = OcctBackend::default();

    // Extrude the sketch profile itself through the OCCT kernel.
    let height = 10.0;
    let solid = backend.extrude_profile(profile, height)?;
    println!("Extruded solid created from profile (height {height})");
    
    // Tessellate
    let mesh = backend.tessellate(solid)?;
    println!("Tessellated: {} vertices, {} triangles", 
             mesh.positions.len(), mesh.triangle_count());
    
    // Export STL
    export_stl(&mesh, "examples/models/sketch_extrude.stl")?;
    
    Ok(())
}

fn export_stl(mesh: &rcad_geom::TriangleMesh, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::Write;
    
    ensure_output_dir();
    
    let mut file = File::create(path)?;
    writeln!(file, "solid sketch_extrude")?;
    
    for tri in mesh.indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        
        let p0 = mesh.positions[i0];
        let p1 = mesh.positions[i1];
        let p2 = mesh.positions[i2];
        
        // Calculate normal
        let v1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let v2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let nx = v1[1] * v2[2] - v1[2] * v2[1];
        let ny = v1[2] * v2[0] - v1[0] * v2[2];
        let nz = v1[0] * v2[1] - v1[1] * v2[0];
        let len = (nx*nx + ny*ny + nz*nz).sqrt();
        
        if len > 0.0 {
            writeln!(file, "  facet normal {} {} {}", nx/len, ny/len, nz/len)?;
        } else {
            writeln!(file, "  facet normal 0 0 0")?;
        }
        
        writeln!(file, "    outer loop")?;
        writeln!(file, "      vertex {} {} {}", p0[0], p0[1], p0[2])?;
        writeln!(file, "      vertex {} {} {}", p1[0], p1[1], p1[2])?;
        writeln!(file, "      vertex {} {} {}", p2[0], p2[1], p2[2])?;
        writeln!(file, "    endloop")?;
        writeln!(file, "  endfacet")?;
    }
    
    writeln!(file, "endsolid sketch_extrude")?;
    println!("Exported STL to {}", path);
    Ok(())
}

fn ensure_output_dir() {
    std::fs::create_dir_all("examples/models").ok();
}