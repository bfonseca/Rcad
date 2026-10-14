# rcad User Testing Guide

## Overview
rcad is a desktop CAD application built with Rust, using egui/wgpu for the UI and OCCT (OpenCASCADE Technology) for geometry. This guide covers how to test the application like a user would - clicking, editing, and using features.

## Prerequisites
- Linux with X11/Wayland display server
- GPU with Vulkan support (for wgpu)
- Built with: `cargo build --release`
- Run with: `cargo run --release` or `./target/release/rcad`

## Application Structure

### UI Layout
```
┌─────────────────────────────────────────────────────────────┐
│  ☰ Menu  [◀/▶] [🔤/🔣] [◀/▶]                               │
├──────────┬──────────────────────────────────────┬───────────┤
│          │                                      │  Model    │
│  Model   │                                      │  Tree     │
│  Sketch  │                                      │  (Overlay)│
│  Assembly│        VIEWPORT (3D)                 │           │
│  Analysis│                                      │  [Search] │
│  View    │                                      │  Features │
│  Tools   │                                      │           │
│          │                                      │           │
│          │                                      │           │
├──────────┴──────────────────────────────────────┴───────────┤
│  Status: Camera: [x, y, z] | rcad - Open Source CAD in Rust │
└─────────────────────────────────────────────────────────────┘
```

### Left Sidebar: Vertical Toolbar (Creo-style Ribbon)
- **Tabs**: Model, Sketch, Assembly, Analysis, View, Tools
- **Modes**: Full (icon+label+shortcut), Icon-only, Collapsed (tab icons only)
- **Position**: Toggle Left/Right with ◀/▶ button

### Right Sidebar: History Overlay (Model Tree)
- **Adaptive opacity**: Transparent when not hovered, dark blur when hovered
- **Resize handle**: Drag left edge to adjust width
- **Collapsible**: Click ⬆ to collapse to top bar
- **Search**: Filter features by name
- **Context menu**: Right-click features for actions

### Viewport: Central 3D Area
- **Orbit camera**: Middle-click + drag to rotate, Shift+middle to pan, scroll to zoom
- **View cube**: Standard views (Front, Top, Right, Iso) via toolbar or keys 1-7
- **Selection**: Click to select triangles (for future feature selection)

---

## Test Scenarios

### 1. Application Launch & Basic Navigation
| Test | Steps | Expected |
|------|-------|----------|
| App launches | Run `cargo run --release` | Window opens 1400x900, titled "rcad" |
| Default geometry visible | On launch | 3 datum planes (XY, YZ, XZ) + coordinate axes visible |
| Camera controls | Middle-click drag, scroll, Shift+middle | Orbit, zoom, pan work smoothly |
| View shortcuts | Press 1-7, Home | Front, Back, Left, Right, Top, Bottom, Iso, Reset |
| Toolbar toggle | Click ◀/▶, 🔤/🔣, ◀/▶ | Collapses, icon-only, moves left/right |
| History overlay | Hover right edge | Appears with blur, shows feature tree |
| History collapse | Click ⬆ on overlay | Collapses to top bar |
| History resize | Drag left edge of overlay | Width changes |

### 2. File Operations (Menu: ☰ Menu → File)
| Test | Steps | Expected |
|------|-------|----------|
| New Document | Menu → New | Clears project, recreates datum geometry |
| Open Document | Menu → Open → select .rcad file | Loads project |
| Save | Menu → Save (or Ctrl+S) | Saves to current path |
| Save As | Menu → Save As | Prompts for new path |
| Export STL | Menu → Export STL | Opens save dialog, writes STL |
| Export STEP | Menu → Export STEP | Opens save dialog, writes STEP |
| Undo | Menu → Undo (or Ctrl+Z) | Reverts last action, rebuilds |
| Redo | Menu → Redo (or Ctrl+Y) | Reapplies undone action |

### 3. Model Tab - Primitive Creation
| Test | Steps | Expected |
|------|-------|----------|
| Create Box | Click Model tab → 📦 Box (or B) | Adds Box feature, shows 10×10×10 cube |
| Create Box via shortcut | Press B key | Same as button click |
| Verify in tree | Check History overlay | "Box" feature appears, ✅ not suppressed |
| Multiple boxes | Click Box multiple times | Each creates new Box feature |
| Undo box creation | Ctrl+Z | Removes last box, mesh updates |

### 4. Sketch Workflow (Creo-style)
| Test | Steps | Expected |
|------|-------|----------|
| New Sketch | Click Sketch tab → 📝 New Sketch (or K) | Creates Sketch1, shows in tree, enters sketch mode |
| Sketch on plane | Select datum plane (FRONT_PLANE) → New Sketch | Sketch created on selected plane |
| Add Point | Sketch tab → ➕ Point (or P) | Adds point at origin in sketch |
| Add Line | Add 2 points → 📏 Line (or L) | Connects last two points |
| Add Rectangle | Sketch tab → ▭ Rect (or R) | Adds 50×50 rectangle centered at origin |
| Solve Constraints | Sketch tab → ✅ Solve | Runs constraint solver |
| Extrude Sketch | Sketch tab → ⬆ Extrude (or E) | Creates Extrude feature, 3D solid visible |
| Revolve Sketch | Sketch tab → 🔄 Revolve (or V) | Creates Revolve feature |
| Sketch visibility | Check tree | Sketch shows as feature, can suppress |

### 5. Feature Tree Interaction (History Overlay)
| Test | Steps | Expected |
|------|-------|----------|
| Select feature | Click feature in tree | Highlights, shows in viewport (TODO) |
| Suppress feature | Right-click → Suppress (or Ctrl+H) | Feature strikethrough, geometry hidden |
| Resume feature | Right-click → Resume | Feature restored, geometry visible |
| Edit Definition | Right-click → Edit Definition | Opens dashboard (TODO) |
| Rename | Right-click → Rename (F2) | Inline edit (TODO) |
| Delete | Right-click → Delete (Del) | Removes feature, rebuilds |
| Insert Here | Right-click → Insert Here | Green ▶ marker, new features insert here |
| Rollback | Right-click → Rollback Here | Orange ⏪ marker, rebuilds to here |
| Drag to reorder | Drag feature onto another | Reorders in tree, rebuilds |
| Show Parents | Right-click → Parents | Highlights parent features (TODO) |
| Show Children | Right-click → Children | Highlights child features (TODO) |
| Search filter | Type in search box | Filters tree to matching names |

### 6. View Tab - Camera Controls
| Test | Steps | Expected |
|------|-------|----------|
| Standard views | Click Front, Back, Left, Right, Top, Bottom, Iso | Camera snaps to view |
| Reset view | Click Reset (Home) | Returns to default iso view |
| Perspective toggle | Click Perspective | Toggles perspective/ortho (TODO) |
| Shaded/Wireframe | Click Shaded/Wireframe | Changes display mode (TODO) |
| Zoom Fit | Click Zoom Fit | Fits model in view (TODO) |
| Previous view | Click Previous | Returns to previous view (TODO) |

### 7. Keyboard Shortcuts
| Shortcut | Action |
|----------|--------|
| 1-7 | Standard views (Front, Back, Left, Right, Top, Bottom, Iso) |
| Home | Reset view |
| B | Box primitive |
| E | Extrude |
| V | Revolve |
| K | New Sketch |
| L | Sketch Line |
| R | Sketch Rectangle |
| P | Sketch Point |
| Ctrl+N | New Document |
| Ctrl+O | Open Document |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+Z | Undo |
| Ctrl+Y | Redo |
| Ctrl+E | Edit Definition (in tree) |
| Ctrl+H | Suppress Feature |
| Del | Delete Feature |
| F2 | Rename Feature |
| F1 | Shortcuts Help |

### 8. Advanced Features (Partially Implemented)
| Feature | Status | Notes |
|---------|--------|-------|
| Fillet/Chamfer | UI only | Button exists, no implementation |
| Shell/Draft | UI only | Button exists, no implementation |
| Hole | UI only | Button exists, no implementation |
| Pattern/Mirror | UI only | Button exists, no implementation |
| Boolean | UI only | Button exists, no implementation |
| Assembly mates | UI only | Tab exists, no implementation |
| Analysis tools | UI only | Tab exists, no implementation |
| Parameters/Relations | UI only | Tab exists, no implementation |

---

## Known Issues / TODO Items

### High Priority
- [ ] Sketch plane selection: Click datum plane/face before New Sketch
- [ ] Sketch editing: Click-to-place points, drag lines
- [ ] Feature edit dashboard: Double-click or Edit Definition
- [ ] Viewport feature selection: Click 3D face/edge/vertex to select in tree
- [ ] Display modes: Shaded, wireframe, hidden line
- [ ] Real extrude from sketch profile (currently uses placeholder box)

### Medium Priority
- [ ] Persistent sketch constraints visualization
- [ ] Dimension constraints in sketch
- [ ] Parameter table (Tools → Parameters)
- [ ] Assembly constraints/mates
- [ ] Export to STEP/STL with real geometry

### Low Priority
- [ ] Customize toolbar dialog
- [ ] Family tables
- [ ] Design exploration
- [ ] Interference/collision detection

---

## Automated Testing

### Unit Tests
```bash
# Run all unit tests
cargo test --workspace

# Run specific crate tests
cargo test -p rcad-sketch
cargo test -p rcad-features
cargo test -p rcad-doc
```

### Integration Tests (CLI examples)
```bash
cd examples
cargo run --example box_demo      # Creates box, exports STL
cargo run --example sketch_demo   # Creates sketch, solves constraints
cargo run --example assembly_demo # Creates assembly
```

### Headless Rendering Test
```bash
# Test if wgpu/egui initializes (requires display)
# In CI with xvfb:
xvfb-run -a cargo test --release -p rcad-app
```

---

## Manual Test Checklist (Run After Each Build)

- [ ] App launches without crash
- [ ] Default datum geometry visible (3 planes + axes)
- [ ] Camera orbit/pan/zoom works
- [ ] View shortcuts 1-7 work
- [ ] Toolbar tabs switch (Model, Sketch, Assembly, Analysis, View, Tools)
- [ ] Toolbar collapse/expand/icon-only modes work
- [ ] Toolbar position toggle (left/right) works
- [ ] History overlay shows feature tree
- [ ] History overlay hover blur effect works
- [ ] History overlay collapse/expand works
- [ ] History overlay resize handle works
- [ ] Search filter in history overlay works
- [ ] Model → Box creates box feature
- [ ] Sketch → New Sketch creates sketch
- [ ] Sketch → Point/Line/Rectangle add geometry
- [ ] Sketch → Extrude creates 3D solid
- [ ] Sketch → Revolve creates 3D solid
- [ ] Right-click feature → Suppress/Resume works
- [ ] Right-click feature → Insert Here/Rollback works
- [ ] Right-click feature → Delete works
- [ ] Drag feature reorder works
- [ ] File → New/Open/Save works
- [ ] File → Export STL/STEP works
- [ ] Undo/Redo (Ctrl+Z/Ctrl+Y) works
- [ ] Shortcuts dialog (F1) opens
- [ ] Status bar shows camera position

---

## Debugging Tips

### Enable Debug Logging
```bash
RUST_LOG=rcad=debug cargo run --release
```

### Inspect Project Structure
- Projects saved as `.rcad` files (MessagePack format)
- Use `rcad-doc` crate to load/inspect programmatically

### Geometry Debugging
- Check `self.mesh` in app for current tessellation
- `self.last_error` shows rebuild errors
- OCCT backend errors logged at `rcad_occt=debug`

---

## Screenshot Test Reference

When running manually, verify these visual states:

1. **Fresh launch**: 3 datum planes (transparent colored quads) + RGB axes
2. **After Box**: 10×10×10 cube at origin, "Box" in tree
3. **After Sketch+Rectangle**: Rectangle outline in 3D (on XY plane), "Sketch1" in tree
4. **After Extrude**: 3D block extruded from rectangle, "Extrude" in tree
5. **Suppressed feature**: Strikethrough in tree, geometry hidden
6. **History hover**: Right panel darkens with blur, readable text
7. **History collapsed**: Thin bar at top-right "Model Tree ⬇"
8. **Toolbar collapsed**: Only tab icons (📦✏️🔧📊👁🔧) visible
9. **Toolbar icon-only**: Icons only, no labels
10. **Search active**: Tree filtered, non-matching hidden

---

## Performance Benchmarks

| Operation | Target |
|-----------|--------|
| App launch | < 2 seconds |
| Box creation | < 100ms |
| Sketch solve (100 entities) | < 500ms |
| Rebuild (50 features) | < 1 second |
| Viewport FPS (static) | 60 FPS |
| Viewport FPS (orbiting) | 30+ FPS |
| Memory (empty) | < 200 MB |
| Memory (100 features) | < 500 MB |

---

## Reporting Issues

When reporting bugs, include:
1. OS / GPU / Driver version
2. Steps to reproduce
3. Expected vs actual behavior
4. `RUST_LOG=rcad=debug` output
5. Screenshot if visual issue
6. `.rcad` file if project-specific