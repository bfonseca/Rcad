//! Example: Simple box creation using rcad API
//!
//! Run with: cargo run --example box_demo

use rcad_occt::OcctBackend;
use rcad_geom::{TriangleMesh, GeometryBackend};
use glam::DVec3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("rcad - Box Demo");
    println!("================");
    
    // Create OCCT backend
    let mut backend = OcctBackend::default();
    
    // Create a box solid
    let size = DVec3::new(50.0, 30.0, 20.0);
    let solid = backend.box_solid(size)?;
    
    println!("Created box: {} x {} x {}", size.x, size.y, size.z);
    
    // Tessellate for display
    let mesh = backend.tessellate(solid)?;
    
    println!("Tessellated mesh:");
    println!("  Vertices: {}", mesh.positions.len());
    println!("  Triangles: {}", mesh.triangle_count());
    println!("  Normals: {}", mesh.normals.len());
    
    // Export to STL (placeholder)
    export_stl_demo(&mesh)?;
    
    println!("\nDone!");
    Ok(())
}

fn export_stl_demo(mesh: &TriangleMesh) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::Write;
    
    ensure_output_dir();
    
    let mut file = File::create("examples/models/box_output.stl")?;
    
    writeln!(file, "solid box")?;
    
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
    
    writeln!(file, "endsolid box")?;
    
    println!("\nExported STL to examples/models/box_output.stl");
    Ok(())
}

fn ensure_output_dir() {
    std::fs::create_dir_all("examples/models").ok();
}