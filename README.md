# rcad - Open Source CAD in Rust

A modern, open-source CAD application built with Rust, OCCT (Open CASCADE Technology), and egui.

## Overview

rcad is a parametric CAD application designed for mechanical engineering and product design. It features:

- **Parametric modeling** - Feature-based design with constraints
- **Sketcher** - 2D constraint-based sketching with Intent Manager (weak/strong dimensions)
- **3D operations** - Extrude, revolve, boolean operations, fillet/chamfer, sew/thicken, loft, boundary
- **Skeletons (Top-Down)** - Publish/Copy Geometry workflow à la Creo
- **Image Underlay** - Scaled image tracing for reverse engineering
- **Assembly** - Multi-part assemblies with constraints (in progress)
- **Modern UI** - Built with egui for fast, responsive interface (vertical toolbar + adaptive history overlay planned)
- **Cross-platform** - Runs on Linux, macOS, and Windows
- **MCP Server** - AI integration via Model Context Protocol (planned)
- **Project manifest** - `rcad.toml` for tolerances, surface colors, external tables
- **Trail logging** - JSON Lines audit trail for debugging/replay

## Architecture

```
rcad/
├── crates/
│   ├── rcad-app/         # Main desktop application (egui + wgpu)
│   ├── rcad-geom/        # Core geometry types and traits
│   ├── rcad-occt/        # OCCT (Open CASCADE) integration via cadrum
│   ├── rcad-doc/         # Document/project data structures (MessagePack/CBOR)
│   ├── rcad-param/       # Parametric feature rebuild engine
│   ├── rcad-sketch/      # 2D constraint solver and sketch entities
│   ├── rcad-features/    # 3D feature operations (extrude, revolve, etc.)
│   ├── rcad-assembly/    # Assembly constraints and management (WIP)
│   ├── rcad-animation/   # Animation timeline and keyframes (post-MVP)
│   ├── rcad-mechanism/   # Mechanical joints and simulation (post-MVP)
│   ├── rcad-collision/   # Collision detection via Rapier (post-MVP)
│   ├── rcad-cae/         # CAE/FEA integration (post-MVP)
│   ├── rcad-skeleton/    # Skeleton/multi-body layout (Publish/Copy Geometry)
│   ├── rcad-underlay/    # Image/DXF underlay support with scale calibration
│   ├── rcad-table/       # Spreadsheet/driving table (Excel/CSV via calamine)
│   ├── rcad-mcp/         # MCP Server for AI integration (planned, separate binary)
│   └── rcad-truck/       # Alternative pure-Rust geometry backend (optional)
├── examples/             # Example models and scripts
├── rcad.toml             # Project manifest (tolerances, colors, tables, startup, MCP)
└── Cargo.toml            # Workspace configuration
```

## File Extensions

| Type | Extension | Description |
|------|-----------|-------------|
| Project | `.rcad` | Main project file (contains parts, assemblies, skeletons) |
| Part | `.rcadprt` | Individual part file (can be referenced by assemblies) |
| Skeleton | `.rcadskl` | Skeleton/layout part for top-down design |
| Assembly | `.rcadasm` | Assembly file with instances and mates |
| Trail Log | `.rcadlog` | JSON Lines audit trail (timestamp, action, params, result, duration) |

## Quick Start

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- System dependencies for OCCT:
  - **Ubuntu/Debian**: `sudo apt-get install libocct-dev libglib2.0-dev`
  - **Fedora**: `sudo dnf install occt-devel glib2-devel`
  - **Arch**: `sudo pacman -S opencascade glib2`
  - **macOS**: `brew install opencascade glib`
  - **Windows**: Use vcpkg or download OCCT binaries

### Building

```bash
# Clone the repository
git clone https://github.com/bfonseca/Rcad.git
cd rcad

# Build release version
cargo build --release

# Run the application
cargo run --release --package rcad-app
```

### First Run

On first start, rcad will:
1. Check for existing projects in `~/.config/rcad/recent.json`
2. If none found, prompt to create a new project
3. Auto-save the project to recent list
4. On subsequent starts, auto-open the last project

### Development

```bash
# Run with debug logging
RUST_LOG=rcad=debug cargo run --package rcad-app

# Run tests
cargo test --workspace

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --workspace
```

## Features

### Sketching
- Points, lines, circles, arcs, rectangles, splines
- Constraints: coincident, horizontal, vertical, parallel, perpendicular, tangent, equal, distance, angle, symmetric, concentric, midpoint
- **Intent Manager**: Weak dimensions (auto-generated) vs Strong dimensions (user-defined)
- DOF (degrees of freedom) visualization and constraint status
- Constraint solving with iterative relaxation + SolveSpace backend (optional)

### Part Design
- Extrude (with draft, offset, symmetric)
- Revolve
- Fillet/Chamfer
- Shell
- Boolean operations (union, cut, intersect)
- Patterns (linear, circular)
- Mirror
- **Surfaces**: Extrude Surface, Loft, Boundary, Sew, Thicken (surface → solid)

### Skeletons (Top-Down Design)
- Create skeleton parts with datum planes, axes, points, curves
- **Publish Geometry** - Select and name reference entities
- **Copy Geometry** - Consume published geometry in child parts with associativity
- **External References** - Visual status: UpToDate / OutOfDate / Broken

### Image Underlay (Reverse Engineering)
- Import PNG/JPEG/SVG as underlay on sketch plane
- Scale calibration: two points + real distance
- Opacity, lock, rotation, display order
- Snap-to-feature for manual tracing

### Assembly (In Progress)
- Mate constraints (coincident, concentric, distance)
- Part instances with transformations
- Hierarchy management

### Import/Export
- STEP (AP203/AP214/AP242) via OCCT
- STL (ASCII/Binary) via OCCT
- Native format: MessagePack/CBOR with schema

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| 1 | Front view |
| 2 | Back view |
| 3 | Left view |
| 4 | Right view |
| 5 | Top view |
| 6 | Bottom view |
| 7 | Isometric view |
| Home | Reset view |
| Middle Mouse + Drag | Orbit |
| Shift + Middle Mouse + Drag | Pan |
| Scroll Wheel | Zoom |
| Ctrl+N | New document |
| Ctrl+O | Open document |
| Ctrl+S | Save document |
| Ctrl+Shift+S | Save As |
| Ctrl+Shift+E | Export STL |
| Ctrl+Shift+P | Export STEP |
| Ctrl+Z | Undo |
| Ctrl+Y / Ctrl+Shift+Z | Redo |
| Delete | Delete selected |
| K | New sketch |
| Enter | Solve constraints |
| P | Add point |
| L | Add line |
| R | Add rectangle |
| E | Extrude sketch |
| V | Revolve sketch |
| B | Add box |
| H | Suppress feature |
| Ctrl+A | Select all |
| Escape | Deselect |
| F1 | Show shortcuts dialog |

## Configuration (rcad.toml)

The project manifest `rcad.toml` controls:
- **General tolerances** (linear, angular)
- **Global parameters** (default extrusion length, etc.)
- **Surface colors** per client/type (threads, fit surfaces, sealing zones)
- **External tables** path/format for Excel/CSV-driven parameters
- **Startup** behavior (auto-open last, recent projects file, max count)
- **Trail log** (enabled, format JSONL, level, rotation)
- **File extensions** and OS association
- **MCP Server** (enabled, TCP port, allow write, timeout)

## MCP Server (AI Integration) - Planned

The `rcad-mcp` crate will provide a separate binary implementing the Model Context Protocol:
- **Resources**: Project, parts, sketches, features, assemblies
- **Tools**: create_part, add_sketch, add_extrude, add_revolve, add_boolean, create_assembly, add_mate, export_step, export_stl, get_project_state
- **Prompts**: Common workflows (new_part_from_sketch, create_assembly_from_parts, etc.)
- **Transport**: stdio (default) or TCP for headless/CI use

## Project Structure

Each crate follows a consistent structure:
```
crate-name/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public API
│   ├── [module].rs     # Implementation modules
│   └── ...
├── tests/              # Integration tests
└── benches/            # Benchmarks (optional)
```

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Code Style
- Follow Rust conventions (run `cargo fmt`)
- Run `cargo clippy` before submitting
- Write tests for new functionality
- Update documentation for public APIs

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [Open CASCADE Technology](https://www.opencascade.com/) - 3D modeling kernel
- [cadrum](https://github.com/cadrum/cadrum) - Rust bindings for OCCT
- [egui](https://github.com/emilk/egui) - Immediate mode GUI
- [glam](https://github.com/bitshifter/glam-rs) - Linear algebra
- [calamine](https://github.com/tafia/calamine) - Excel/CSV parsing
- [mcp-rs](https://github.com/modelcontextprotocol/rust-sdk) - MCP SDK

## Roadmap

See [instructions.md](instructions.md) for detailed plan and milestones.

### Current Status (Phase E - In Progress)
- [x] Keyboard shortcuts system
- [x] Shortcuts dialog (F1)
- [x] Standard view shortcuts (1-7, Home)
- [x] Core modeling (sketch, features, skeletons, underlay)
- [x] Project manifest (rcad.toml)
- [ ] Assembly mates (Phase D)
- [ ] UI polish: vertical toolbar + adaptive history overlay
- [ ] rcad.toml integration in app
- [ ] Trail logging (JSON Lines)
- [ ] File extension registration
- [ ] MCP Server crate
- [ ] Documentation and example models
- [ ] `good first issue` labels per crate

### Post-MVP
- [ ] Mechanisms and motion (`rcad-mechanism`)
- [ ] Collision detection (`rcad-collision` via Rapier)
- [ ] Animation timeline (`rcad-animation`)
- [ ] CAE/FEA adapters (`rcad-cae` → CalculiX, OpenFOAM)
- [ ] WASM/web build
- [ ] CI/CD pipeline
- [ ] Automated releases

---

**Repository**: https://github.com/bfonseca/Rcad
**License**: MIT

## Quick Start

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- System dependencies for OCCT:
  - **Ubuntu/Debian**: `sudo apt-get install libocct-dev libglib2.0-dev`
  - **Fedora**: `sudo dnf install occt-devel glib2-devel`
  - **Arch**: `sudo pacman -S opencascade glib2`
  - **macOS**: `brew install opencascade glib`
  - **Windows**: Use vcpkg or download OCCT binaries

### Building

```bash
# Clone the repository
git clone https://github.com/yourusername/rcad.git
cd rcad

# Build release version
cargo build --release

# Run the application
cargo run --release --package rcad-app
```

### Development

```bash
# Run with debug logging
RUST_LOG=rcad=debug cargo run --package rcad-app

# Run tests
cargo test --workspace

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --workspace
```

## Features

### Sketching
- Points, lines, circles, arcs, rectangles
- Constraints: coincident, horizontal, vertical, parallel, perpendicular, tangent, equal, distance, angle
- Dof (degrees of freedom) visualization
- Constraint solving with Newton-Raphson

### Part Design
- Extrude (with draft, offset, symmetric)
- Revolve
- Fillet/Chamfer
- Shell
- Boolean operations (union, cut, intersect)
- Patterns (linear, circular)
- Mirror

### Assembly
- Mate constraints (coincident, parallel, perpendicular, tangent, distance, angle)
- Sub-assemblies
- Motion simulation
- BOM generation

### Import/Export
- STEP (AP203/AP214/AP242)
- STL (ASCII/Binary)
- DXF (2D sketches)
- OBJ (meshes)

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| 1 | Front view |
| 2 | Back view |
| 3 | Left view |
| 4 | Right view |
| 5 | Top view |
| 6 | Bottom view |
| 7 | Isometric view |
| Home | Reset view |
| Middle Mouse + Drag | Orbit |
| Shift + Middle Mouse + Drag | Pan |
| Scroll Wheel | Zoom |
| Ctrl+N | New document |
| Ctrl+O | Open document |
| Ctrl+S | Save document |
| Ctrl+Shift+S | Save As |
| Ctrl+Shift+E | Export STL |
| Ctrl+Shift+P | Export STEP |
| Ctrl+Z | Undo |
| Ctrl+Y / Ctrl+Shift+Z | Redo |
| Delete | Delete selected |
| K | New sketch |
| Enter | Solve constraints |
| P | Add point |
| L | Add line |
| R | Add rectangle |
| E | Extrude sketch |
| V | Revolve sketch |
| B | Add box |
| H | Suppress feature |
| Ctrl+A | Select all |
| Escape | Deselect |
| F1 | Show shortcuts dialog |

## Project Structure

Each crate follows a consistent structure:
```
crate-name/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public API
│   ├── [module].rs     # Implementation modules
│   └── ...
├── tests/              # Integration tests
└── benches/            # Benchmarks (optional)
```

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Code Style
- Follow Rust conventions (run `cargo fmt`)
- Run `cargo clippy` before submitting
- Write tests for new functionality
- Update documentation for public APIs

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [Open CASCADE Technology](https://www.opencascade.com/) - 3D modeling kernel
- [egui](https://github.com/emilk/egui) - Immediate mode GUI
- [glam](https://github.com/bitshifter/glam-rs) - Linear algebra
- [nalgebra](https://nalgebra.org/) - Advanced math (in some crates)

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features and milestones.

### Phase E (Current)
- [x] Keyboard shortcuts system
- [x] Shortcuts dialog (F1)
- [x] Standard view shortcuts (1-7, Home)
- [ ] UI polish and theming
- [ ] Documentation
- [ ] Example models

### Post-MVP
- [ ] Mechanisms and motion
- [ ] Collision detection
- [ ] Animation timeline
- [ ] CAE/FEA adapters

### Infrastructure
- [ ] CI/CD pipeline
- [ ] WASM/web build
- [ ] Automated releases

### Starting the app
- [ ] cd /home/bruno/Projectos/Rcad && cargo run --release --package rcad-app
