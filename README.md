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

## MCP Server (AI Integration)

`rcad-mcp` is a self-contained Model Context Protocol server (JSON-RPC 2.0 over
stdio, one message per line). It lets an AI assistant build and export models
without the GUI:

```bash
cargo run --package rcad-mcp -- --allow-write true --project my_part.rcad
```

- **Tools**: `get_project_state`, `new_project`, `create_part`, `save_project`,
  `load_project`, `add_box`, `add_cylinder`, `add_extrude`, `add_revolve`,
  `add_boolean`, `rebuild`, `export_step`, `export_stl`
- **Resources**: `rcad://project`, `rcad://project/parts`, `rcad://project/features`
- **Transport**: stdio. `--allow-write false` exposes a read-only server.

Still to come: assembly tools and MCP prompts.

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

The MCP server is implemented directly on `serde_json`, and the toolbar renders the
committed rcad-icons PNGs (decoded with `image` and cached as egui textures), so no
extra SDK or SVG rasteriser is needed.

## Roadmap

See [instructions.md](instructions.md) for detailed plan and milestones.

### Current Status (Phase E - In Progress)
- [x] Keyboard shortcuts system
- [x] Shortcuts dialog (F1)
- [x] Standard view shortcuts (1-7, Home)
- [x] Core modeling (sketch, features, skeletons, underlay)
- [x] Project manifest (rcad.toml)
- [x] STEP/STL export through OCCT (wired to the real kernel)
- [x] MCP server (`rcad-mcp`) with modelling tools and resources
- [x] CI workflow (`ci/ci.yml`; move it into `.github/workflows/` to enable it)
- [x] Committed icon sprites so a fresh clone builds
- [ ] Assembly mates (Phase D)
- [ ] UI polish: vertical toolbar + adaptive history overlay
- [ ] rcad.toml integration in app
- [ ] Trail logging (JSON Lines)
- [ ] File extension registration
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

---

## Running the app

```bash
# Debug build (default member)
cargo run --package rcad-app

# Release build
cargo run --release --package rcad-app

# Headless MCP server on stdio
cargo run --package rcad-mcp -- --allow-write true
```

### Command line options

```
rcad [options]

  --screenshot <path>   Render the UI, write a PNG to <path>, then exit
  --frames <n>          Frames to render before capturing (default 30)
  --demo                Start with a sample bracket instead of an empty part
  --renderer <backend>  default | glow | wgpu
  --view <name>         iso | front | back | left | right | top | bottom
  --help                Show this message
```

### Screenshots, and machines without a GPU

`--screenshot` renders the real UI, captures the framebuffer and exits. It is
how documentation images and bug reports are produced:

```bash
cargo run --release --package rcad-app -- --demo --view iso --screenshot docs/app.png
```

On a headless machine or a CI runner, drive it with Mesa's software rasteriser
and eframe's OpenGL backend:

```bash
LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  cargo run --package rcad-app -- --renderer glow --demo --screenshot app.png
```

### Icons

The toolbar embeds the committed 64px PNGs from `rcad-icons/png/64/<theme>/...`
(see `crates/rcad-app/build.rs`) and decodes them into GPU textures on first
use. The two sprite SVGs are kept for the icon browser in `rcad-icons/index.html`;
regenerate them with:

```bash
cd rcad-icons/source
python3 build.py sprites ..
```
