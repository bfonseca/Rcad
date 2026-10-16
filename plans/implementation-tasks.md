# Detailed Task Breakdown - Creo Workflow Implementation for rcad

---

## ESTADO E PRÓXIMOS PASSOS (handover - ler primeiro)

**Onde estamos (última sessão):** o sketch está a meio. Os pontos 1 e 2 abaixo
ficaram por fazer; tudo o resto descrito neste ficheiro, até essa data, está
feito, commitado e verde.

**Próximo, por esta ordem (instrução do utilizador):**

1. **Desenhar com o rato**: dois cliques para um retângulo no viewport, com snap
   à grelha, em coordenadas do sketch. Hoje a geometria entra só pelos
   botões/shortcuts do sketch (`add_sketch_rectangle` etc.).
2. **Restrições, dimensões e graus de liberdade no ecrã**: o crate `rcad-sketch`
   já tem `analyze_dof()` e `constraint_status()`; nada é mostrado.

**Feito até aqui (não repetir):** nomes estáveis de faces e arestas; referências
persistentes por feature; seleção ordenada partilhada (viewport + árvore) com
consultas `boundary` / `tangent chain` / `surface ring`; fillet e chamfro por
`EdgeSet` (References / All / Outer / Inner / Vertical / Horizontal) com ângulo
diedral calculado no kernel; reparação de referências (deteção, memória da última
geometria, lista com substituição); sketch sobre datum **ou face**, plano visível
com grelha e eixos; extrude e revolve com `SketchPlacement` (varrem segundo a
normal / eixo do plano); plano do sketch como referência persistente.

**Portão de qualidade:** `cargo test --workspace` tem de dar exit 0 e
`cargo check --workspace --all-targets` **0 avisos**. Commits vão para `main` e
são enviados com `git push origin main`.

**Detalhes que poupam tempo:**

- Capturas de ecrã: `--screenshot <path> --frames 30` com
  `LIBGL_ALWAYS_SOFTWARE=1`. Em Wayland a app força o backend X11 sozinha (ver a
  secção "Screenshots on a Wayland session"); sem isso o processo pendura-se.
- `rcad-param` tem `rcad-occt` como *dev-dependency*, para os testes com kernel
  real (arestas, faces, fillets geométricos).
- Duplicação conhecida a unificar: a regra de referências perdidas existe na app
  (`lost_references`) e no kernel (`RebuildResult.lost_references`).
- `Extrude`/`Revolve` sem `placement` mantêm o comportamento antigo (XY, eixo Z),
  por compatibilidade com documentos antigos e com o MCP.


Based on the approved plan in `plans/creo-workflow-analysis.md`, this document breaks down each phase into actionable coding tasks.

---

## Phase 1: Core UI Architecture - Vertical Toolbar + Adaptive History Overlay 🔴

### 1.1 Remove Top Menu Bar & Create Vertical Toolbar
- [ ] **1.1.1** Create `crates/rcad-app/src/ui/toolbar.rs` with `VerticalToolbar`, `ToolbarTab`, `ToolbarButton` structs
- [ ] **1.1.2** Define `ToolbarAction` enum for all toolbar actions (mapping to existing app methods)
- [ ] **1.1.3** Implement 6 tabs: Model, Sketch, Assembly, Analysis, View, Tools with Creo-parity buttons
- [ ] **1.1.4** Add collapse/expand toggle (icon-only vs icon+text mode)
- [ ] **1.1.5** Add dockable position state (left/right) with persistence
- [ ] **1.1.6** Remove `egui::TopBottomPanel::top("menu_bar")` from `app.rs`
- [ ] **1.1.7** Move File/Help actions to hamburger menu (☰) in toolbar header

### 1.2 Adaptive History Overlay (Right Side)
- [ ] **1.2.1** Create `crates/rcad-app/src/ui/history_overlay.rs` with `HistoryOverlay` struct
- [ ] **1.2.2** Implement hover detection → blur + darken effect (wgpu shader)
- [ ] **1.2.3** Render feature tree with Parent/Child indentation, visual indicators
- [ ] **1.2.4** Add Insert Here (▶) and Rollback (⏪) markers
- [ ] **1.2.5** Implement right-click context menu (Edit Definition, Suppress, Delete, Rollback, etc.)
- [ ] **1.2.6** Add drag-reorder with validation (no circular refs)
- [ ] **1.2.7** Implement virtual scrolling for large trees
- [ ] **1.2.8** Persist expanded/collapsed state across sessions

### 1.3 Viewport Integration
- [ ] **1.3.1** Update `app.rs` layout: CentralPanel for viewport, SidePanel for toolbar, overlay for history
- [ ] **1.3.2** Keep status bar at bottom
- [ ] **1.3.3** Add mini-toolbar on selection (context-sensitive)
- [ ] **1.3.4** Implement blur shader for history overlay (Gaussian blur when hovered)

### 1.4 Keyboard Shortcuts Integration
- [ ] **1.4.1** Map toolbar actions to existing shortcut system
- [ ] **1.4.2** Update F1 shortcuts dialog with new toolbar actions
- [ ] **1.4.3** Add toolbar-specific shortcuts (Tab switching, collapse, etc.)

---

## Phase 2: rcad.toml Integration in rcad-app 🔴

### 2.1 Config Module
- [ ] **2.1.1** Create `crates/rcad-app/src/config.rs` with all structs from plan
- [ ] **2.1.2** Implement `AppConfig::load()` and `save()` with TOML
- [ ] **2.1.3** Add `ConfigError` type with proper error handling
- [ ] **2.1.4** Implement `Default` for all config structs matching rcad.toml
- [ ] **2.1.5** Add helper methods: `get_surface_color()`, `get_global_param()`

### 2.2 RcadApp Integration
- [ ] **2.2.1** Add `config: AppConfig`, `recent_projects: Vec<RecentProject>`, `trail_logger: Option<TrailLogger>` to `RcadApp`
- [ ] **2.2.2** Load config in `RcadApp::new()`, initialize trail logger
- [ ] **2.2.3** Implement recent projects loading/saving (platform config dir)
- [ ] **2.2.4** Auto-open last project on startup if configured
- [ ] **2.2.5** Add config watcher (notify crate) for hot-reload

### 2.3 Runtime Application
- [ ] **2.3.1** Pass tolerances to sketch solver (`rcad-sketch`)
- [ ] **2.3.2** Pass tolerances to feature rebuild (`rcad-param`)
- [ ] **2.3.3** Apply surface colors in viewport rendering (`Body.surface_type` → config)
- [ ] **2.3.4** Load external tables on startup via `rcad-table`
- [ ] **2.3.5** Spawn MCP server process if enabled

### 2.4 Config UI (Tools → Options)
- [ ] **2.4.1** Create Options dialog with tabs: General, Tolerances, Colors, Tables, Startup, Logging, Extensions, MCP
- [ ] **2.4.2** Live preview of color changes in viewport
- [ ] **2.4.3** "Reset to Defaults" button
- [ ] **2.4.4** Auto-save on dialog close

---

## Phase 3: Feature Tree / Model Tree Enhancements 🔴

### 3.1 Data Model Extensions (rcad-doc)
- [ ] **3.1.1** Add `FeatureReferences`, `Reference`, `ReferenceType` to `rcad-doc/src/lib.rs`
- [ ] **3.1.2** Add `FeatureParameters`, `ParameterValue` to `Feature`
- [ ] **3.1.3** Add `parent_features`, `child_features`, `insert_here_marker`, `rollback_marker`, timestamps to `Feature`
- [ ] **3.1.4** Add `insert_here_index`, `rollback_index`, `layers`, `parameters`, `relations`, `family_table`, `regeneration_mode` to `Part`
- [ ] **3.1.5** Add `Layer`, `LayerItem`, `Parameter`, `Relation`, `FamilyTable`, `FamilyColumn`, `FamilyRow`, `ColumnType`, `RegenerationMode`
- [ ] **3.1.6** Add `#[serde(default)]` for backward compatibility
- [ ] **3.1.7** Add version migration in `Project::load()`

### 3.2 Feature Tree Operations (rcad-features)
- [ ] **3.2.1** Enhance `move_feature()` with circular ref validation
- [ ] **3.2.2** Add `set_insert_here(part, index)` and `set_rollback(part, index)`
- [ ] **3.2.3** Add `edit_feature_definition(part, feature_id, new_params)` 
- [ ] **3.2.4** Add `edit_feature_references(part, feature_id, new_refs)`
- [ ] **3.2.5** Add `get_parents(part, feature_id)` and `get_children(part, feature_id)`
- [ ] **3.2.6** Add `delete_feature_with_deps(part, feature_id, cascade: bool)`
- [ ] **3.2.7** Add `copy_feature(part, feature_id)` and `paste_feature(part, feature_id, insert_index)`
- [ ] **3.2.8** Auto-compute `child_features` from `parent_features` on rebuild

### 3.3 History Overlay Tree Rendering
- [ ] **3.3.1** Render tree with indentation based on parent/child depth
- [ ] **3.3.2** Visual indicators: ✅ Active, 🚫 Suppressed, ▶ Insert Here, ⏪ Rollback, ❌ Error, ⚠️ Warning, 🔗 External Ref
- [ ] **3.3.3** Right-click context menu with all actions
- [ ] **3.3.4** Click → select feature (highlight in viewport)
- [ ] **3.3.5** Double-click → Edit Definition dashboard
- [ ] **3.3.6** Drag → reorder with validation feedback
- [ ] **3.3.7** Toggle parent/child highlighting

---

## Phase 4: Intent Manager & Sketch Enhancements 🔴

### 4.1 Weak/Strong Dimension UX (rcad-sketch)
- [ ] **4.1.1** Enhance `ConstrainedConstraint` with `auto_generated`, `promoted_from`, `dimension_text_position`, `dimension_orientation`
- [ ] **4.1.2** Add `DimensionOrientation` enum (Horizontal, Vertical, Parallel, Radial, Diameter)
- [ ] **4.1.3** Implement `add_dimension_with_intent()` with auto-promotion logic
- [ ] **4.1.4** Implement `promote_weak_to_strong()` 
- [ ] **4.1.5** Implement `analyze_dof_detailed()` returning `DOFAnalysis`
- [ ] **4.1.6** Implement `find_unconstrained_entities()` for UI highlighting
- [ ] **4.1.7** Visual distinction in viewport: dashed/gray/italic (weak) vs solid/black/normal (strong)
- [ ] **4.1.8** Dimension context menu: Toggle Driving/Driven, Promote, Delete, Edit, Flip Arrows
- [ ] **4.1.9** Drag dimension text → update `dimension_text_position`
- [ ] **4.1.10** Double-click dimension → in-place value edit (driving only)

### 4.2 Reference Geometry Improvements
- [ ] **4.2.1** Add `ProjectionMode` enum (OneSide, BothSides, AlongVector)
- [ ] **4.2.2** Enhance `ReferenceGeometry` with `projection_mode`, `source_feature`, `source_body`, `is_associative`, `status`
- [ ] **4.2.3** Implement `project_references(entities, mode, source_feature)`
- [ ] **4.2.4** Implement `update_references(project)` for associative updates
- [ ] **4.2.4** Create Reference Geometry Panel in Sketch mode
- [ ] **4.2.5** Auto-projection on sketch creation (pre-selected entities)

### 4.3 Sketch Plane Selection Workflow
- [ ] **4.3.1** Create `SketchCreator` state machine (SelectPlane → SelectOrientation → Confirm)
- [ ] **4.3.2** Implement planar face highlighting on hover
- [ ] **4.3.3** Implement datum plane selection
- [ ] **4.3.4** Implement orientation edge/axis selection with triad preview
- [ ] **4.3.5** Show sketch coordinate system triad (X=red/H, Y=green/V, Z=blue)
- [ ] **4.3.6** Click triad axes to flip orientation

### 4.4 Additional Sketch Tools
- [ ] **4.4.1** Line Chain tool (continuous click-click-click, Esc to end)
- [ ] **4.4.2** Tangent Arc tool
- [ ] **4.4.3** 3-Point Arc tool
- [ ] **4.4.4** Conic tool (rho parameter)
- [ ] **4.4.5** Text tool (TrueType as sketch geometry)
- [ ] **4.4.6** Import DXF/SVG → sketch entities
- [ ] **4.4.7** Sketch Region auto-detection
- [ ] **4.4.8** Show DOF arrows toggle
- [ ] **4.4.9** Highlight conflicts (red on over-constrained)
- [ ] **4.4.10** Constraint Table (spreadsheet view, editable)
- [ ] **4.4.11** Drag Test (Shift+drag to test DOF)

---

## Phase 5: Skeleton Workflow Enhancements 🟠

### 5.1 Publish Geometry
- [ ] **5.1.1** Enhance `PublishedRef` with `description`, `feature_id`, `body_id`, `selection_query`, `publish_type`, `include_children`
- [ ] **5.1.2** Add `PublishType` enum (Geometry, Shrinkwrap, CopyGeometry, Interface)
- [ ] **5.1.3** Implement `Part::get_publishable_items()` returning `PublishableItem`
- [ ] **5.1.4** Create Publish Geometry dialog (Model tab)
- [ ] **5.1.5** Implement `publish_geometry_dialog(part, selections)`

### 5.2 Copy Geometry
- [ ] **5.2.1** Add `CopyGeometryOptions` struct
- [ ] **5.2.2** Create Copy Geometry dialog (Model tab)
- [ ] **5.2.3** Implement `copy_geometry(project, child_id, skeleton_id, selections, options)`
- [ ] **5.2.4** Handle "Copy as Reference" (associative) vs "Independent Copy"
- [ ] **5.2.5** Implement `resolve_published_geometry()` and `create_static_construction_feature()`

### 5.3 External Reference Manager
- [ ] **5.3.1** Add `status: ExternalRefStatus` to `PublishedRef` (rcad-doc)
- [ ] **5.3.2** Create External Reference Manager panel (Tools tab)
- [ ] **5.3.3** Implement `update_external_references(project, part_id)`
- [ ] **5.3.4** Implement `replace_external_reference()`
- [ ] **5.3.5** Implement `break_external_reference()`
- [ ] **5.3.6** Status icons: ✅ UpToDate, ⚠️ OutOfDate, ❌ Broken
- [ ] **5.3.7** Actions: Update (🔄), Replace (🔗), Break (✂)

### 5.4 Skeleton-Specific Enhancements
- [ ] **5.4.1** Skeleton Creation Wizard dialog (Model → New Skeleton)
- [ ] **5.4.2** Template options: Standard, Sheet Metal, Mechanism, etc.
- [ ] **5.4.3** Auto-publish options
- [ ] **5.4.4** Skeleton editing in assembly context
- [ ] **5.4.5** "Update External Refs" button in assembly toolbar
- [ ] **5.4.6** Visual indicators in assembly tree for out-of-date parts

---

## Phase 6: Surface Color Rendering 🟠

- [ ] **6.1** Add `surface_type: Option<String>` to `Body` struct (rcad-doc)
- [ ] **6.2** Add "Surface Type" dropdown in feature creation dashboards
- [ ] **6.3** Implement `apply_surface_colors(mesh, bodies, config)` in viewport rendering
- [ ] **6.4** Map body triangles to surface colors in tessellation
- [ ] **6.5** Default types: threads, fit_surfaces, sealing_zones, custom

---

## Phase 7: External Tables Integration 🟠

- [ ] **7.1** Load tables on project open from `external_tables.default_path`
- [ ] **7.2** Cache tables per `cache_in_memory` setting
- [ ] **7.3** Watch table files for changes
- [ ] **7.4** Add "Link Parameter to Table" in Edit Definition dashboard
- [ ] **7.5** Implement `TableDrivenParameter` struct
- [ ] **7.6** Map table columns → feature parameters at rebuild time

---

## Phase 8: Trail Logging 🟠

- [ ] **8.1** Create `TrailLogger` struct with JSONL output
- [ ] **8.2** Implement log rotation (10MB max, 5 files)
- [ ] **8.3** Log format: timestamp, action, params, result, duration_ms
- [ ] **8.4** Integration points:
  - [ ] Feature creation/edit/delete
  - [ ] Sketch operations
  - [ ] File operations (open/save/export)
  - [ ] Rebuild operations
  - [ ] Assembly mate operations
- [ ] **8.5** Add "Replay" mode for replicating sessions

---

## Phase 9: Project Initialization & File Associations 🟡

- [ ] **9.1** First-run dialog: "Create New Project" if no recent projects
- [ ] **9.2** Auto-save to recent.json
- [ ] **9.3** Linux: `.desktop` file + `xdg-mime` registration
- [ ] **9.4** macOS: `Info.plist` + `LSHandlers`
- [ ] **9.5** Windows: Registry keys for `.rcad`, `.rcadprt`, `.rcadskl`, `.rcadasm`, `.rcadlog`

---

## Phase 10: MCP Server Implementation 🟡

- [ ] **10.1** Create `rcad-mcp` binary with `mcp-rs`
- [ ] **10.2** Resources: `rcad://project/{id}`, `rcad://part/{id}`, `rcad://sketch/{id}`, `rcad://assembly/{id}`
- [ ] **10.3** Tools: `create_part`, `add_sketch`, `add_extrude`, `add_revolve`, `add_boolean`, `create_assembly`, `add_mate`, `export_step`, `export_stl`, `get_project_state`, `rebuild_part`
- [ ] **10.4** Prompts: `new_part_from_sketch`, `create_assembly_from_parts`, `create_skeleton_driven_part`, `reverse_engineer_from_image`
- [ ] **10.5** Transport: stdio (default) + TCP (optional, port from config)

---

## Implementation Order & Dependencies

```
Phase 1 (UI) ──────────────────┐
                               ├──→ Phase 3 (Feature Tree needs UI)
Phase 2 (Config) ──────────────┤
                               ├──→ Phase 6 (Colors needs Config)
Phase 3 (Feature Tree) ────────┘

Phase 4 (Sketch) ──────────────┐ (can parallel with Phase 1-3)
                               ├──→ Phase 5 (Skeleton needs Sketch refs)

Phase 5 (Skeleton) ────────────┘

Phases 6-10: Can be done in parallel after core phases stable
```

---

## Estimated Task Count

| Phase | Tasks | Est. Days |
|-------|-------|-----------|
| 1 | 15 | 10-15 |
| 2 | 12 | 8-12 |
| 3 | 15 | 10-15 |
| 4 | 20 | 15-20 |
| 5 | 16 | 10-15 |
| 6 | 5 | 3-5 |
| 7 | 6 | 5-8 |
| 8 | 6 | 5-8 |
| 9 | 5 | 3-5 |
| 10 | 5 | 5-10 |
| **Total** | **105** | **74-113** |

---

## Next Steps

1. **Switch to Code mode** to begin Phase 1 implementation
2. Start with **1.1.1** - Create toolbar module
3. Set up GitHub issues with `good first issue` labels for each task
4. Weekly sync on progress

*Document Version: 1.0*
*Date: 2026-10-12*

---

## Phase 0: Project Model (blocking - nothing can be created before this) 🔴

Added from the 2026-10 user brief. This is the foundation everything else sits
on: a project is a set of general definitions shared by every component, plus
the folder where those components are stored. No part, assembly or skeleton can
exist outside a project.

Much of this is already specified in `instructions.md` (section "Gestão de
Projeto via rcad.toml") and `rcad.toml`; the items marked NEW are from the brief
and are not written down anywhere yet. `plans/creo-workflow-analysis.md` already
flags rcad.toml integration as Critical: the file exists but nothing reads it.

### 0.1 Project folder is the entry point
- [ ] **0.1.1** (NEW) On a clean install - no recent projects, no history - the app
      opens the project picker first: create a project (choosing its folder) or
      open an existing one. The modelling UI is unreachable until a project is
      active.
- [ ] **0.1.2** (NEW) A project owns its folder; new components are saved there by
      default.
- [ ] **0.1.3** Persist the recent-project list (instructions.md names
      `~/.config/rcad/recent.json`) and honour `[startup] auto_open_last` and
      `max_recent_projects`.

### 0.2 Project-wide definitions
- [ ] **0.2.1** Load `rcad.toml` when a project opens. The parser is the missing
      piece flagged as Critical in the Creo workflow analysis.
- [ ] **0.2.2** `[general_tolerance]` linear / angular, used by kernel operations
      and shown in the UI.
- [ ] **0.2.3** `[global_parameters]` - variables callable from any component or
      feature in the project.
- [ ] **0.2.4** (NEW) Project **functions** callable from relations: a named
      formula shared by the whole project, so a relation can read
      `wall_thickness(diameter)` instead of repeating the expression. Needs a
      new `[functions]` section in rcad.toml.
- [ ] **0.2.5** `[external_tables]` - Excel/CSV driven parameters (ties into the
      rcad-table crate).

### 0.3 Colour scheme: names are global, values are per project (NEW)
The distinction the brief draws, which the current `[surface_colors]` section
does not make: the *vocabulary* of colour ids belongs to the installation, the
*RGB values* belong to the project.
- [ ] **0.3.1** In the application settings (per install, shared by all projects)
      the user defines any number of colour ids - a unique name/identifier each,
      e.g. "fit zone", "metric thread", "high gloss zone". Changing this list
      changes only the vocabulary, never any geometry.
- [ ] **0.3.2** In the project file, `[surface_colors]` maps each id to an RGB
      value. Client A and client B can therefore use different colours for the
      same logical id.
- [ ] **0.3.3** Changing an RGB in the project file must propagate to every
      component and feature of that project on the next load: nothing is baked
      into a component, they only store the logical id.
- [ ] **0.3.4** Apply the mapping in the viewport (already tracked as
      `surface-color-rendering` in instructions.md).
- [ ] **0.3.5** A face is tagged with the logical colour id, never with a literal
      RGB value.

### 0.4 Acceptance
- [ ] A fresh install cannot reach the modelling UI without an active project.
- [ ] Editing one colour in rcad.toml changes the display of every component in
      that project, without touching any of them.
- [ ] The same global parameter is visible and usable from a part, a skeleton and
      an assembly.

### 0.3 Implementation note: the kernel already gives us face ids

**Finding (de-risks the whole of 0.3).** `cadrum::Mesh` carries exactly what
per-face colouring needs, and rcad throws it away:

```rust
// cadrum-0.8.20/src/common/mesh.rs
/// Per-triangle face ID. Length equals `indices.len() / 3`.
pub face_ids: Vec<u64>,
/// Per-face color map (face_id -> Color).
#[cfg(feature = "color")]
pub colormap: HashMap<u64, Color>,
```

`triangle_mesh_from_cadrum` in `crates/rcad-occt/src/lib.rs` copies vertices,
normals and indices, and drops `face_ids`. So the triangle-to-face mapping is
already computed by OpenCascade on every rebuild; no new kernel work is needed.

#### Steps
- [x] **A.** `rcad_geom::TriangleMesh` gains `face_ids: Vec<u64>`, one per
      triangle, with `#[serde(default)]` and a `Default` impl so existing
      constructors keep working. Also `NO_FACE_ID` (u64::MAX),
      `face_id(triangle)`, `normalize_face_ids()` and
      `append(&mesh, face_id)`, which keeps the two lists aligned.
- [x] **B.** `triangle_mesh_from_cadrum` copies `mesh.face_ids`.
- [x] **C.** In `rcad-param`, when the construction mesh is appended to the
      display mesh, extend `face_ids` with a sentinel (`u64::MAX`) so
      construction geometry is never colour-matched to a solid face.
- [x] **D.** The document maps a face to its colour id: `Part::face_colours`,
      with `set_face_colour` / `clear_face_colour` / `face_colour_id` /
      `face_colour(name, manifest)`. It takes part in serde round trips. The key
      started as the kernel face id, then became the geometric signature, and is
      now the feature-anchored reference (see the resolution below), which
      survives a parameter change as well.
- [x] **E.** The viewport resolves each triangle: `face_ids[i]` -> its stable
      name through `face_names` -> colour id -> `manifest.colour(id)` -> RGB,
      falling back to the current shading when the face is untagged or the
      project does not define the id. The project colour is lit by the same
      `n·light` factor, so a coloured face still reads as a solid.
- [x] **F.** A way to tag a face in the UI: pick a triangle in the viewport and
      choose an id from the list of colours the project defines, in the status
      bar. `--demo-colours` (with `--hide-datums`) is the headless path for
      screenshots and docs.

#### Verification
- [x] Unit test: a cube tessellates to six distinct face ids
      (`rcad-occt::tests::a_box_tessellates_to_six_distinct_face_ids`).
- [x] Unit test: an untagged face keeps the default shading
      (`rcad-app::viewport::tests::an_untagged_face_keeps_the_default_shading`).
- [x] Screenshot: two faces of the demo part with different project colours,
      confirming the colour comes from rcad.toml and not from the component
      (`face-colours-a.png`, `face-colours-b.png`). Two projects with the same
      demo part and different `[surface_colors]`: the top face and one side
      change colour (116 368 pixels), everything else, including the third
      face, is pixel-identical.
- [x] Changing the RGB in rcad.toml and re-running changes the display without
      touching the part: the two screenshots above differ only in rcad.toml,
      and `rcad-doc::face_colour_tests::the_project_owns_the_rgb_value` plus
      `rcad-app::viewport::tests::editing_the_project_changes_the_colour_without_touching_the_part`
      pin it down in code.

Note: cadrum also exposes its own `colormap`, but the project manifest must own
the RGB values, so rcad should use `face_ids` and resolve colours itself.

#### The limitation that motivated this work (resolved below)

`cadrum` face ids are the `TopoDS_TShape*` **address** of the face
(`src/occt/face.rs` -> `ffi::face_tshape_id`), so they are process-local, not
persistent names. Evidence from the two screenshot runs: the same demo top face
was `140117738460192` in one process and `1401181894554144` in the next, and the
two PNGs differ *only* in that status-bar text (1 176 pixels, all inside the
status bar); the geometry and the colours are pixel-identical.

Within one process the id is stable as long as the kernel shape is alive or
reused from `ShapeCache` (which is what "rebuild after editing one parameter"
does, so step D does survive that case). But a fully re-created shape, a reopened
document, or another machine produces different ids, so a colour saved in a
part file would attach to the wrong face - or to none - on reload.

Before this becomes a user-facing feature the tag key has to become stable, for
example by re-resolving at load time from a geometric signature (quantised face
centroid + normal + area), or by storing a Creo-style stable face name on the
feature that created the face. The map on `Part` can stay; only the key needs to
change. Tracked here so the next session does not re-discover it.

#### Resolved: stable names for faces and edges

The limitation above was real: cadrum identifies a face or an edge by the
`TopoDS_TShape*` address, so the id changes in every process - and can even be
reused by the allocator once the shape is freed. A colour saved against one was
meaningless. The fix is to name topology from its geometry, in
`rcad_geom::naming`:

- A face name is `F:<kind>:a<area>:c<centre>[:n<normal>]:e<edges>:p<perimeter>`,
  with the surface kind and its parameters in `<kind>` (`plane`,
  `cylinder(r5.0000)`, ...). The normal is included for planar faces only: on a
  curved face the normal at the centre depends on where the projection lands.
- An edge name is `E:<kind>:l<length>:s<start>:t<end>:m<midpoint>`. The endpoints
  are canonicalised, so the curve parameter direction OCCT happens to choose
  does not change the name.
- Lengths, areas and positions are rounded to 1e-4 mm and directions to 1e-3, so
  numeric noise between runs cannot change a name.
- Faces (or edges) sharing a signature are the same geometry, for instance two
  identical holes: they get a `#1`, `#2` suffix. The set of names is
  deterministic; which of two *identical* elements gets which suffix is not, and
  cannot matter.
- `TriangleMesh` carries `face_names` (kernel id -> `NamedFace`) and `edges`
  (`NamedEdge`: name, label, endpoints, length, polyline), filled by `rcad-occt`
  from `Solid::iter_face()` / `Solid::iter_edge()`. Construction geometry is
  appended without names, so it can never match a face.

`Part::face_colours` is now keyed by the stable name. A tag whose face was
edited or deleted no longer matches anything: `Part::unmatched_face_colours`
reports it and the status bar says how many tags were lost, instead of painting
the colour on whichever face took the old one's place.

Verification (in the suite): a box has six named faces and twelve named edges;
two independent builds give identical name sets; every triangle's face id has a
name; the names carry the face areas (200/300/600 mm2) and never an address.
`a_face_colour_survives_a_save_and_a_reload_in_a_fresh_kernel` saves a document,
loads it and resolves the colour against a second kernel;
`the_same_name_resolves_whatever_the_kernel_address_is` pins the viewport side;
`a_lost_face_tag_is_reported_in_the_status_bar` covers the honest failure mode.

Screenshots (`--save`, then `--open` in a second process): face-colours-a.png
(saved and coloured) and face-colours-reloaded.png (document reopened by a fresh
process) are pixel-identical in the model; face-colours-a.png against
face-colours-b.png differs only in the two tagged faces. Reproduce with
`--demo --demo-colours --hide-datums --save <file>`, then `--open <file>` under a
second project.

A signature is stable only while the geometry is unchanged: change the height of
a box and every signature on it moves with the geometry. On top of the
signatures, the rebuild now names topology from the *feature* that created it and
follows that name through regeneration, which is what Creo does.

- A primitive names its own faces by role: `<feature uuid>/box/z+`,
  `<feature uuid>/cylinder/side`, `<feature uuid>/extrude/side3`. The role comes
  from the feature definition, not from the computed geometry, so it survives a
  parameter change. The uuid is the key, not the feature name, so renaming a
  feature cannot break a stored reference.
- An operation records its face history *when it runs*, in terms of the
  **signature** of the source face in the input it worked on. The kernel reports
  the ids of the copies it operated on, which the caller never sees, and those
  ids are reused by the allocator, so ids cannot carry the meaning; the geometry
  can. A result face inherits the reference of the face it came from: a boolean
  keeps the block faces and namespaces the ones taken from its tool under the
  boolean (`<boolean uuid>/<tool uuid>/cylinder/side`, so two holes cut with the
  same tool stay distinct), a fillet keeps the faces it trimmed, and a translate
  inherits by kernel id because it preserves them.
- Anything a feature cannot name keeps its geometric signature as the fallback.
- An edge is named from the **two faces it bounds**: `edge(<face>|<face>)`, or
  `edge(<face>|seam)`. An edge therefore follows its faces, which is what makes a
  fillet or chamfer reference on the top edge of a hole survive a change of the
  block height.

Verification in the suite: `rcad-param::naming::tests` covers the derivation
(roles, history, id inheritance, seam edges, duplicate suffixes) and drives the
real kernel: a box keeps all 6 face and 12 edge references when its height
changes; a drilled block keeps its 7 face and 15 edge references when the height
changes; the six original faces keep their references through a fillet. End to
end in `rcad-app`,
`a_face_colour_survives_a_parameter_change_and_a_reload` tags a face, changes
the height, saves the document, reopens it in a fresh kernel and resolves the
colour against the rebuilt model.

The saved document now carries these references instead of signatures: the
`--demo --demo-colours --save` run writes `<feature uuid>/box/z+` and
`<feature uuid>/box/x+` for the two tagged faces. The screenshots above were
captured before this change (geometric keys); re-capturing them needs a working
GL session - the display stopped delivering frames while this was being written,
and the previous commit hangs the same way, so it is the session and not the
code. The tests cover what the screenshots would show.

Still open: the features whose topology the kernel does not describe. `Extrude`
gets cap and side roles (sides by position order, so editing the profile can
relabel a side); `Revolve`, `Loft`, `Boundary`, `Sew`, `Shell`, `Draft`, `Hole`,
`Pattern` and `Mirror` keep their signatures until they get a role catalogue or
history of their own. A face that a feature cannot claim still loses its tag when
its geometry is edited, and the status bar reports it instead of guessing.

### Selection: one ordered list (step 1 of the selection/sketch plan)

`crates/rcad-app/src/selection.rs` holds the single selection list shared by the
viewport and the model tree. Items carry the stable reference (face name, edge
name, feature id), never an index, so a selection can go straight into
`FeatureReferences`. A plain click replaces, Ctrl toggles, Shift extends, and the
order is kept.

- The viewport picks most specific first: edge, then datum, then face. A
  right-click cycles the candidates under the cursor, so a face under an edge,
  or a datum over a face, can still be reached.
- Solid edges are drawn when selected or hovered, faces are highlighted by
  reference (not by triangle) and datums by feature id.
- The selection bar above the status bar shows the ordered list, and its tooltip
  names the seed: a command reads the first item as its placement and the last
  as its seed.
- `new_sketch` already consumes the selection: the sketch is placed on the first
  planar datum selected, and waits in placement mode when there is none.

Next, step 2: tangents and the incident face references on `NamedEdge`, then the
queries (boundary, tangent chain, surface ring), then fillet and chamfer by the
selected edges.

### Screenshots on a Wayland session (fixed)

`--screenshot` on a Wayland session used to hang after a couple of frames with no
PNG: the compositor drives frame callbacks, so a window that is not being
presented - locked screen, another workspace, a headless runner - never redraws,
and the capture waits for a frame that never arrives. The app now drops
`WAYLAND_DISPLAY` for a capture when XWayland is present, so winit uses the X11
backend, which delivers redraws to the client. Interactive runs keep Wayland.
`force_x11_for_capture` is unit tested, and a capture logs "capturing through
X11".

`--select-demo-face` selects the tagged demo face, so a screenshot documents the
selection bar and the highlight. A selected face is *tinted* - the project colour
still shows through - rather than replaced.

Screenshot comparison with the fixed capture: face-colours-a.png against
face-colours-reloaded.png (saved, then reopened by a fresh process) differ in 0
model pixels; against face-colours-b.png they differ only in the two tagged
faces (115 924 px); face-colours-selection.png shows the selection bar.

### Smart selection queries (step 2, in progress)

`rcad_geom::query` holds the Creo queries as pure functions over the mesh
topology: edge geometry plus the face references the rebuild writes on every
edge.

- `boundary_loops(face)` chains a face edges into loops by shared endpoints and
  puts the longest first: the outer boundary, then inner wires.
- `tangent_chain(edge, tolerance)` walks both ways while the next edge continues
  the direction (1 degree by default), which selects the run of edges around a
  boss and stops at a corner.
- `surface_ring(edge)` is every edge of the surfaces the seed bounds.
- `references(edges)` turns the result into the references a feature stores.

`NamedEdge` gained the start and end tangents and the references of the faces it
bounds, filled by the rebuild. The selection bar offers the three queries on the
context menu of a selected edge, and the result is appended to the selection in
order, ready for a fillet.

Kernel-backed tests: every edge of a box knows its two faces, and the boundary of
the top face is its four edges, named as references.

Still to do for step 2: fillet and chamfer by the selected edges - store them in
`FeatureReferences.fillet_edges` and rebuild from them - and the From-To chain.

### Fillet and chamfer by edge set (step 2, done)

A round or a break now stores an `EdgeSet`, not a snapshot of edges, and the
rebuild re-resolves it every time:

- `References`: the edges the user picked, by stable reference.
- `All`: every edge of the body.
- `Outer` / `Inner`: the edges whose material angle is below / above 180 degrees.
  `NamedEdge` carries the dihedral angle, computed per edge in the kernel backend
  by probing along the bisector of the two adjacent face normals.
- `Vertical` / `Horizontal`: the edges parallel / perpendicular to a datum axis,
  within one degree. **Both need that reference**, because horizontal is defined
  as perpendicular to it; without one they resolve to nothing rather than to
  everything, and the menu offers them disabled with that explanation.

The resolver returns edge *signatures*, not kernel ids: an operation works on a
copy of the shape, whose ids are fresh, so ids resolved beforehand would not
match. Selection menu: Round and Break each list the six sets.

Tests: a fillet on two references rounds two edges and adds far fewer faces than
the same fillet on every edge; the rule modes resolve on a synthetic topology
with one edge of each kind; the vertical and horizontal modes resolve to nothing
without the axis; and every box edge reports ninety degrees.

### The cycle is closed: pick, size, round (step 2, done)

- The selection bar carries the size of the next round or break (a drag value in
  mm, 1 mm by default), and Round/Break use it.
- The toolbar Fillet and Chamfer buttons, which were TODO stubs, now create the
  feature: the selected edges when there are any, every edge otherwise. The
  labels in the tree and the icons already followed.
- `selection_edge_set()` is the one place that rule lives.

Tests: the toolbar round stores the selected references in order and the size
from the field; the same button with nothing picked stores `EdgeSet::All` at
1 mm. 57 in rcad-app, 0 warnings.

Next: the sketch. `new_sketch` already places a sketch on the first planar datum
picked; what is missing is the sketch environment itself - the grid and plane in
the viewport, drawing and constraining in it, and then reading its profile as an
extrude input.

### The sketch plane is visible (step 3, first slice)

A sketch now draws its own plane in the viewport: the grid, and its two axes in
colour, centred on the sketch origin (`sketch_basis` + `sketch_grid_lines` +
`draw_sketch_plane`). The plane comes from the selection (`new_sketch` records
it in `sketch_plane`); while the sketch is in placement mode nothing is drawn,
because it has no plane yet. The profile to extrude path already existed:
`extrude_sketch` reads `Sketch::to_profile()` and creates the feature.

Next in the sketch: draw in it (two clicks for a rectangle, snapping to the grid),
constraints and dimensions on screen, and degrees of freedom. Then the profile
closes the cycle plane -> profile -> solid.

### Lost references: to do

When a reference disappears and a later feature used it, the rebuild currently
reports the failure of the whole feature. Creo shows which reference was lost,
where it was, and lets the user substitute it. Planned shape:

- A `LostReference` diagnostic per feature: the reference string, the last
  geometry known for it (midpoint, length, the faces it bounded) kept from the
  last successful build, and the feature that owns it.
- Resolve an edge set into found and lost instead of failing: the feature applies
  to what it found and is marked as needing repair.
- A repair list in the UI: each lost reference with its last known location, and
  a Substitute action that takes the new reference from the current selection
  and rewrites that feature edge set, then rebuilds.

### Sketch: a face is a plane, and the profile extrudes (step 3)

- A sketch can now be placed on a face of the model, not only on a datum:
  `face_plane` takes the triangle centre and the normal from its vertices (not
  from how the tessellation stores normals) and an x in the plane.
- The cycle is proven by test: rectangle, solve, profile, extrude creates an
  Extrude feature, and the profile that reaches the kernel is the sketch one.
- `extrude_sketch` refuses a sketch off the XY plane with an explanation: the
  kernel sweeps the profile along Z, which would put the solid in the wrong
  place. This is a real limitation, not a polish item.

Still missing in the sketch, and not pretended otherwise:

- Drawing by clicking in the viewport (two clicks for a rectangle, snapping to
  the grid). Today the geometry comes from the sketch toolbar and shortcuts.
- Constraints and dimensions on screen, and the degrees of freedom
  (`Sketch::analyze_dof` / `constraint_status` exist in the crate but are not
  shown).
- Extruding along the sketch normal: needs the plane on the feature and a
  backend that extrudes along a direction.

### Lost references (next)

See the section above: found/lost resolution instead of failure, the last known
geometry of each reference, and substitution from the selection in one list.

### The profile carries its plane (step 3, the extruding part)

`Solid::extrude(&edges, Z * height)` sweeps from the XY plane along +Z, and
`FeatureKind::Extrude` had nowhere to say where the profile lay. So a sketch was
only *drawn* anywhere: it extruded from the XY plane. Now:

- `SketchPlacement { origin, normal, x_direction }` on the Extrude feature,
  serde-defaulted so old documents still read as "on the XY plane".
- `extrude_profile_placed` on the backend: the profile is built on the plane
  (`profile_edges_on`, `polygon_on_plane`) and swept along that plane normal,
  with the in-plane x re-orthogonalised against the normal so a sloppy placement
  cannot shear the profile.
- The app passes the sketch plane of the open sketch, and the "not supported yet"
  guard is gone.

Tests: an extrude placed on the XZ plane (normal +Y) produces geometry bounded by
the profile in X and Z and by the height along Y; a sketch off the XY plane
extrudes and the feature records its normal; and a round on the outer edges of a
box is the same feature as a round on all twelve references of it.

### Lost references: detection and memory (first layer)

- `resolve_edge_set` now returns an `EdgeSetResolution { found, lost }`: the rule
  modes can never lose a reference (they are re-evaluated), the picked ones can.
- A round whose references are partly gone applies to the survivors and is
  reported; when every pick is gone the feature is **skipped**, never widened to
  the whole body - which was the dangerous silent behaviour.
- `RebuildResult.lost_references: Vec<LostReference>` carries, per feature, the
  reference that went missing.
- `rcad_geom::remember(mesh)` records what is known about each edge reference
  (midpoint, length, the faces it bounded, label) so a repair list can say where
  the lost one used to be, and `ReferenceMemory::describe()` writes the sentence.

Tests: a partly lost set keeps the surviving pick and reports one lost reference;
a fully lost set is skipped with an explanation and the body stays a plain box.

Still to do here: the app does not yet keep the memory across rebuilds, fill
`last_location`/`last_description`, or offer substitution (replace the lost
reference in the feature with one the user has just selected).

### Reference repair: memory and substitution (second layer, done)

- The app keeps `reference_memory`: `rcad_geom::remember(mesh)` is merged into it
  on every rebuild, so a reference that disappears keeps the last place it was
  seen.
- `describe_lost` fills `last_location` and `last_description` of every lost
  reference from that memory, which is what the repair list shows on hover.
- A repair list (bottom panel, above the selection bar) names each feature and
  reference, and offers `Substitute <lost> with <the selected edge>`. With no
  edge selected the button is disabled and says so.
- `substitute_reference` rewrites the `EdgeSet` of that feature in place, pushes
  one undo state, drops the entry from the list and rebuilds.

Tests: the list holds only what is missing, named by feature; the description
carries the position and the length from the last build that had it; substitution
rewrites the feature and refuses a reference it never had.

Known limit: the memory covers edges (what fillets and chamfers reference today);
lost *face* references, such as a sketch plane, are not listed yet. The list is
also derived in the app from the part and the mesh, while the kernel derives its
own copy to decide whether to skip a feature: same rule in two places.

### Revolve carries its plane too (step 3)

The revolve had exactly the extrude problem, and worse: the profile was built on
the XY plane and spun about the **world Z axis**, which for a flat profile is
degenerate. Now the profile is built on its own plane and spun about that plane
in-plane y axis through the sketch origin, which is what revolving a sketch
means. `SketchPlacement` on `Revolve`, serde-defaulted; `revolve_profile_placed`
on the backend, defaulting to the old world-Z axis for callers that have no
placement.

Test: an offset profile revolved 360 degrees gives a ring whose radius stays
between the profile edges and that reaches out of the original plane. Plus an app
test that a sketch off the XY plane records the plane on the Revolve.

Still missing in the sketch, in order:

1. Drawing with the mouse: two clicks for a rectangle, snapping to the grid.
2. Constraints and dimensions on screen, and the degrees of freedom
   (`Sketch::analyze_dof` / `constraint_status` exist but are not shown).
3. The sketch plane as a persistent reference, not three numbers: today a sketch
   does not follow the face it was drawn on when that face moves.

### The sketch plane is a reference now (step 3, point 3 of 3)

`SketchPlacement` carries `reference: Option<String>`, the stable name of the
datum plane or model face the sketch was placed on. At rebuild time the plane is
resolved again: a datum plane by its feature id, a model face by looking up the
reference on the body the feature consumes (`resolve_placement`). So a feature the
sketch feeds follows the face it was drawn on instead of a snapshot of it.

The stored origin and normal stay as the last known position: when the reference
is gone the feature is still built where it last was, and the reference shows up
in the repair list, reusing the machinery the rounds and breaks use.

Tests: a profile placed by reference sits on the top face of a box and is swept
from it; a reference that is gone is reported and the stored plane is used.

Remaining in the sketch: drawing with the mouse (two clicks, grid snap) and
constraints/dimensions plus degrees of freedom on screen.
