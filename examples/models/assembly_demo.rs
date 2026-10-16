//! Example: Project with multiple parts (assembly structure)
//!
//! Run with: cargo run --example assembly_demo

use rcad_doc::{Project, Part, Body, Feature, FeatureKind, Mate, MateKind, Assembly, ComponentInstance};
use rcad_occt::OcctBackend;
use rcad_geom::GeometryBackend;
use glam::DVec3;
use uuid::Uuid;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("rcad - Assembly Demo");
    println!("=====================");
    
    // Create a new project
    let mut project = Project::new("AssemblyDemo");
    
    // Create base plate part
    let base_plate = create_base_plate()?;
    project.parts.push(base_plate.clone());
    let base_plate_id = base_plate.id;
    
    // Create bracket part
    let bracket = create_bracket()?;
    project.parts.push(bracket.clone());
    let bracket_id = bracket.id;
    
    // Create shaft part
    let shaft = create_shaft()?;
    project.parts.push(shaft.clone());
    let shaft_id = shaft.id;
    
    println!("Created parts in project:");
    println!("  Base plate: {} ({})", base_plate.name, base_plate_id);
    println!("  Bracket: {} ({})", bracket.name, bracket_id);
    println!("  Shaft: {} ({})", shaft.name, shaft_id);
    
    // Create an assembly
    let mut assembly = Assembly::new("MainAssembly");
    
    // Add component instances (references to parts with transforms)
    let inst_base = ComponentInstance {
        id: Uuid::new_v4(),
        part_id: base_plate_id,
        name: "BasePlate:1".to_string(),
        translation: [0.0, 0.0, 0.0],
        rotation_deg: [0.0, 0.0, 0.0],
    };
    assembly.instances.push(inst_base.clone());
    
    let inst_bracket = ComponentInstance {
        id: Uuid::new_v4(),
        part_id: bracket_id,
        name: "Bracket:1".to_string(),
        translation: [50.0, 0.0, 10.0],
        rotation_deg: [0.0, 0.0, 0.0],
    };
    assembly.instances.push(inst_bracket.clone());
    
    let inst_shaft = ComponentInstance {
        id: Uuid::new_v4(),
        part_id: shaft_id,
        name: "Shaft:1".to_string(),
        translation: [50.0, 0.0, 35.0],
        rotation_deg: [0.0, 0.0, 0.0],
    };
    assembly.instances.push(inst_shaft.clone());
    
    println!("\nAdded {} component instances to assembly", assembly.instances.len());
    
    // Add mates (constraints between components)
    // Using simple Mate structure with a and b referencing component instance IDs
    assembly.mates.push(Mate {
        id: Uuid::new_v4(),
        kind: MateKind::Coincident,
        a: inst_base.id,
        b: inst_bracket.id,
        distance: None,
    });
    
    assembly.mates.push(Mate {
        id: Uuid::new_v4(),
        kind: MateKind::Concentric,
        a: inst_bracket.id,
        b: inst_shaft.id,
        distance: None,
    });
    
    assembly.mates.push(Mate {
        id: Uuid::new_v4(),
        kind: MateKind::Distance,
        a: inst_base.id,
        b: inst_shaft.id,
        distance: Some(25.0),
    });
    
    println!("Added {} mates to assembly", assembly.mates.len());
    
    // Add assembly to project
    project.assemblies.push(assembly.clone());
    
    // Print assembly info
    println!("\nAssembly: {}", assembly.name);
    println!("  Instances: {}", assembly.instances.len());
    println!("  Mates: {}", assembly.mates.len());
    
    // Print positions
    println!("\nComponent instance positions:");
    for inst in &assembly.instances {
        println!("  {}: pos=({:.1}, {:.1}, {:.1}) rot=({:.1}, {:.1}, {:.1})", 
            inst.name, inst.translation[0], inst.translation[1], inst.translation[2],
            inst.rotation_deg[0], inst.rotation_deg[1], inst.rotation_deg[2]);
    }
    
    // Save project
    project.save(Path::new("examples/models/assembly_project.rcad"))?;
    println!("\nSaved project to examples/models/assembly_project.rcad");
    
    // Export assembly as STEP (placeholder)
    export_assembly_step(&project, &assembly)?;
    
    println!("\nDone!");
    Ok(())
}

fn create_base_plate() -> Result<Part, Box<dyn std::error::Error>> {
    let mut part = Part::new_solid("BasePlate");
    let mut backend = OcctBackend::default();
    
    // Create a rectangular plate
    let solid = backend.box_solid(DVec3::new(100.0, 80.0, 10.0))?;
    let _mesh = backend.tessellate(solid)?;
    
    // Add body
    let body_id = Uuid::new_v4();
    part.bodies.push(Body {
        id: body_id,
        name: "BasePlateBody".to_string(),
        feature_id: Uuid::nil(),
        is_solid: true,
        color: Some([100, 100, 200, 255]),
        visible: true,
    });
    
    // Add feature (using Extrude variant since Base doesn't exist)
    let _feat_id = Uuid::new_v4();
    part.features.push(Feature::new(
        "BasePlate",
        FeatureKind::Extrude {
            profile: rcad_geom::Profile2D { outer: vec![], holes: vec![] },
            height: 10.0,
            placement: None,
        },
    ));
    
    Ok(part)
}

fn create_bracket() -> Result<Part, Box<dyn std::error::Error>> {
    let mut part = Part::new_solid("Bracket");
    let mut backend = OcctBackend::default();
    
    // Create an L-shaped bracket (base + vertical)
    let base = backend.box_solid(DVec3::new(60.0, 40.0, 10.0))?;
    let _mesh = backend.tessellate(base)?;
    
    // Add body
    let body_id = Uuid::new_v4();
    part.bodies.push(Body {
        id: body_id,
        name: "BracketBody".to_string(),
        feature_id: Uuid::nil(),
        is_solid: true,
        color: Some([200, 100, 100, 255]),
        visible: true,
    });
    
    // Add features
    part.features.push(Feature::new(
        "BracketBase",
        FeatureKind::Extrude {
            profile: rcad_geom::Profile2D { outer: vec![], holes: vec![] },
            height: 10.0,
            placement: None,
        },
    ));
    
    part.features.push(Feature::new(
        "BracketVertical",
        FeatureKind::Extrude {
            profile: rcad_geom::Profile2D { outer: vec![], holes: vec![] },
            height: 50.0,
            placement: None,
        },
    ));
    
    Ok(part)
}

fn create_shaft() -> Result<Part, Box<dyn std::error::Error>> {
    let mut part = Part::new_solid("Shaft");
    let mut backend = OcctBackend::default();
    
    // Create a cylindrical shaft
    let solid = backend.cylinder(5.0, 30.0)?;
    let _mesh = backend.tessellate(solid)?;
    
    // Add body
    let body_id = Uuid::new_v4();
    part.bodies.push(Body {
        id: body_id,
        name: "ShaftBody".to_string(),
        feature_id: Uuid::nil(),
        is_solid: true,
        color: Some([100, 200, 100, 255]),
        visible: true,
    });
    
    // Add feature
    part.features.push(Feature::new(
        "Shaft",
        FeatureKind::Revolve {
            profile: rcad_geom::Profile2D { outer: vec![], holes: vec![] },
            angle_deg: 360.0,
        },
    ));
    
    Ok(part)
}

fn export_assembly_step(project: &Project, assembly: &Assembly) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::Write;
    use chrono::Utc;
    
    ensure_output_dir();
    
    let mut file = File::create("examples/models/assembly_output.step")?;
    
    writeln!(file, "ISO-10303-21;")?;
    writeln!(file, "HEADER;")?;
    writeln!(file, "FILE_DESCRIPTION(('rcad Assembly Export'),'1');")?;
    writeln!(file, "FILE_NAME('assembly_output.step','{}',(''),(''),'rcad','rcad');", Utc::now().format("%Y-%m-%dT%H:%M:%S"))?;
    writeln!(file, "FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));")?;
    writeln!(file, "ENDSEC;")?;
    writeln!(file, "DATA;")?;
    
    // Write parts
    for inst in &assembly.instances {
        if let Some(part) = project.part(inst.part_id) {
            writeln!(file, "#{} = PRODUCT('{}', '{}', $);", inst.id.as_u128(), part.name, inst.name)?;
            writeln!(file, "#{} = PRODUCT_DEFINITION('{}', $, #{}, #1);", inst.id.as_u128() + 1000, inst.name, inst.id.as_u128())?;
        }
    }
    
    writeln!(file, "ENDSEC;")?;
    writeln!(file, "END-ISO-10303-21;")?;
    
    println!("\nExported assembly STEP to examples/models/assembly_output.step");
    Ok(())
}

fn ensure_output_dir() {
    std::fs::create_dir_all("examples/models").ok();
}