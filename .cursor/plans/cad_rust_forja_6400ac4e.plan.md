---
name: CAD Rust rcad
overview: Plano para o CAD paramétrico open-source rcad em Rust (desktop-first), no repositório GitHub já criado, modular, reutilizando kernels e solvers OSS existentes, com MVP utilizável em ~6 meses focado em modelação + skeletons + engenharia reversa por imagem, e roadmap pós-MVP para assemblagens, mecanismos, colisões, timeline e CAE.
todos:
  - id: scaffold-workspace
    content: Clonar https://github.com/bfonseca/Rcad em ~/Projectos/Rcad; workspace Cargo, crates modulares e CI basico
    status: in_progress
  - id: fase-a-viewport-occt
    content: "Fase A: egui+wgpu viewport, rcad-doc, integracao OCCT tessellate + STEP/STL"
    status: pending
  - id: fase-b-parametric-skeleton
    content: "Fase B: sketch+constraints, features solidas/superficies, feature tree, skeletons"
    status: pending
  - id: fase-c-underlay
    content: "Fase C: import imagem com escala, underlay no plano de sketch"
    status: pending
  - id: fase-d-assembly
    content: "Fase D: assemblagens com mates basicos via rust_slvs"
    status: pending
  - id: fase-e-poc-publico
    content: "Fase E: undo/redo, exemplos, README/licencas, labels de contribuicao no repo rcad"
    status: pending
  - id: roadmap-pos-mvp
    content: Reservar APIs/crates para mechanism, collision, animation, CAE (pos-6-meses)
    status: pending
isProject: false
---

# rcad — CAD paramétrico open-source em Rust

> **Histórico (2026-10-12).** Este ficheiro é o plano inicial do Cursor. O plano ativo do
> próximo ciclo é [plans/foundation-plan.md](../../plans/foundation-plan.md) (render →
> curvas analíticas → referências/pai-filho). Ver também a “Reconciliação de estado” em
> [plans/implementation-tasks.md](../../plans/implementation-tasks.md).

## Confirmação do âmbito


| Decisão          | Valor                                                                                                      |
| ---------------- | ---------------------------------------------------------------------------------------------------------- |
| Horizonte de uso | ~6 meses (MVP utilizável, não “paridade Creo/Fusion”)                                                      |
| Equipa           | Nós primeiro; open-source modular assim que houver POC                                                     |
| Plataforma       | Desktop Linux/Windows/macOS agora; WASM depois                                                             |
| Stack            | Preferir OSS existente (wgpu, egui, OCCT, solvers, etc.)                                                   |
| Prioridades      | 1 modelação+skeletons → 1.1 reverse eng. → 2 assemblagens → 3 mecanismos → 4 colisões → 5 timeline → 6 CAE |
| Repositório      | **[bfonseca/Rcad](https://github.com/bfonseca/Rcad)** já criado — não criar repo novo; trabalhar nesse remote |


**Expectativa realista:** em 6 meses não haverá paridade com Creo/Fusion. O objectivo é uma aplicação desktop com a qual já se possa modelar peças paramétricas, usar skeletons, importar imagens com escala, e ter a base modular para a comunidade. Assemblagens entram no final do semestre se o núcleo estiver estável; mecanismos/CAE ficam desenhados na arquitectura mas implementados depois.

**Projecto:** `rcad` (repo GitHub: [bfonseca/Rcad](https://github.com/bfonseca/Rcad)). Clone/trabalho local em [`/home/bruno/Projectos/Rcad`](/home/bruno/Projectos/Rcad) apontando para esse remote. Licença recomendada: **Apache-2.0 OR MIT** no código nosso; respeitar **LGPL-2.1** do OCCT (distribuição com possibilidade de relink).

---

## Arquitectura modular

Workspace Cargo com crates independentes (cada um publicável depois no crates.io / contribuição separada):

```mermaid
flowchart TB
  subgraph app [Desktop]
    rcad_app[rcad-app egui+wgpu]
  end
  subgraph core [Núcleo]
    rcad_doc[rcad-doc]
    rcad_param[rcad-param]
    rcad_sketch[rcad-sketch]
    rcad_feat[rcad-features]
    rcad_skel[rcad-skeleton]
    rcad_underlay[rcad-underlay]
  end
  subgraph geom [Geometria]
    rcad_geom[rcad-geom trait]
    occt_backend[rcad-occt]
    truck_backend[rcad-truck optional]
  end
  subgraph later [Pós-MVP]
    rcad_asm[rcad-assembly]
    rcad_mech[rcad-mechanism]
    rcad_coll[rcad-collision]
    rcad_anim[rcad-animation]
    rcad_cae[rcad-cae]
  end
  rcad_app --> rcad_doc
  rcad_doc --> rcad_param
  rcad_param --> rcad_feat
  rcad_feat --> rcad_geom
  rcad_sketch --> rcad_geom
  rcad_skel --> rcad_doc
  rcad_underlay --> rcad_app
  rcad_geom --> occt_backend
  rcad_geom --> truck_backend
  rcad_asm --> rcad_doc
  rcad_mech --> rcad_asm
  rcad_coll --> rcad_mech
  rcad_anim --> rcad_mech
  rcad_cae --> rcad_doc
```

### Stack fechada (sem alternativas em aberto)


| Camada                     | Escolha                                                                                                                                      | Motivo                                                          |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| UI                         | `eframe` + `egui`                                                                                                                            | Rápido para POC desktop; comunidade Rust forte                  |
| Viewport 3D                | `wgpu` (WebGPU)                                                                                                                              | Desktop agora, caminho natural para WASM depois                 |
| Math                       | `glam`                                                                                                                                       | Alinhado com egui/wgpu                                          |
| Kernel BREP principal      | **OCCT** via crate Rust (`cadrum` como 1.ª opção de integração; fallback `opencascade`/`opencascade-rs` se a API do cadrum for insuficiente) | Sólidos/superfícies, sew/thicken, STEP, tessellation industrial |
| Kernel auxiliar            | **Truck** (`truck-modeling` / meshalgo) atrás do mesmo trait `rcad-geom`, opcional                                                           | Experimentos pure-Rust / malhas; não bloqueia o MVP             |
| Constraints 2D/3D          | `rust_slvs` (SolveSpace)                                                                                                                     | Sketches e mates futuros                                        |
| Física / colisões (fase 4) | `rapier3d`                                                                                                                                   | Tempo real; Nexus só se surgir necessidade clara                |
| CAE (fase 6)               | CalculiX / OpenFOAM como **processos externos** + adaptadores de mesh/BC                                                                     | Não reimplementar solvers FEA/CFD                               |
| Formato nativo             | Documento versionado (MessagePack ou CBOR + schema) + export STEP/STL via OCCT                                                               | Interop industrial desde cedo                                   |
| Imagens underlay           | `image` + textura wgpu; calibração de escala no UI                                                                                           | Prioridade 1.1                                                  |


Trait `GeometryBackend` em `rcad-geom`: criar sólido/superfície, booleanos, extrude/revolve/loft/sweep, fillet/chamfer, sew → solid, thicken, tessellate, STEP I/O. O app só fala com o trait; o backend OCCT é o default.

---

## Modelo de dados (núcleo)

- **Documento:** projecto → peças / assemblagens / skeletons.
- **Peça:** feature tree (DAG paramétrico) → bodies (sólido / superfície / construção).
- **Feature:** tipo + parâmetros + referências a entities (faces/edges/vertices estáveis via IDs de histórico, o melhor possível com OCCT naming/history).
- **Skeleton:** peça especial com planos, eixos, pontos, curvas e solids de referência; bodies de produção referenciam o skeleton (top-down design estilo Creo).
- **Rebuild:** `rcad-param` reavalia a árvore; cache de shapes por feature; dirty propagation.

Isto é o “coração” do MVP — sem feature tree estável o resto não escala.

---

## MVP a 6 meses (utilizável)

### Fase A — Fundação (semanas 1–4)

- Scaffold workspace no repo `rcad` + CI (build Linux, clippy, testes básicos).
- `rcad-app`: janela egui, viewport wgpu (orbit/pan/zoom), grelha, seleção por picking de mesh.
- `rcad-doc` + serialização mínima (abrir/guardar projecto).
- Integração OCCT: primitiva → tessellate → mostrar no viewport; export STL/STEP.

### Fase B — Modelação paramétrica + skeletons (semanas 5–14) — prioridade 1

- Sketch 2D: linhas/arcos/círculos, cotas e constraints via `rust_slvs`, planos de sketch.
- Features sólidas mínimas: Extrude, Revolve, Boolean (union/cut/intersect), Fillet/Chamfer básicos, planos/eixos/pontos de construção.
- Multi-body numa peça; tree UI (reordenar / suprimir / editar parâmetros → rebuild).
- Superfícies básicas (ex.: extrude surface, boundary/loft simples se OCCT permitir na API exposta) + **Sew** e **Thicken** (superfície → sólido).
- **Skeletons:** criar skeleton part, publicar geometria de referência, criar peça filha que consome essas referências.

### Fase C — Engenharia reversa por imagem (semanas 10–16, em paralelo com B) — prioridade 1.1

- Import PNG/JPEG/SVG rasterizado como underlay num plano.
- Definição de escala (dois pontos + distância real).
- Opacidade, bloqueio, alinhamento ao sketch activo; tracing manual sobre a imagem (não OCR/auto-mesh 3D nesta fase).

### Fase D — Assemblagens mínimas (semanas 15–22) — prioridade 2

- Inserir instâncias de peças; hierarquia; mates básicos (coincident / concentric / distance) com solver.
- Exploded view simples opcional; não exigir skeletons de movimento ainda.

### Fase E — Fecho “usável” (semanas 20–26)

- Estabilidade de rebuild, undo/redo, atalhos, documentação de contribuição em [bfonseca/Rcad](https://github.com/bfonseca/Rcad).
- POC anunciável: README, licenças, roadmap, issues etiquetadas `good first issue` por crate.
- Smoke tests e 2–3 modelos de exemplo (peça + skeleton + underlay + assembly simples).

**Critério de sucesso aos 6 meses:** modelar uma peça paramétrica multi-feature, usar skeleton + underlay com escala, montar 2–3 peças com mates básicos, gravar/reabrir e exportar STEP/STL.

---

## Roadmap pós-MVP (arquitectura reservada, implementação depois)


| Prioridade | Módulo           | Conteúdo                                                                                                             |
| ---------- | ---------------- | -------------------------------------------------------------------------------------------------------------------- |
| 3          | `rcad-mechanism` | Joints (revolute, slider, cylindrical…), motores, actuação manual (drag) vs prescrita                                |
| 4          | `rcad-collision` | Broad/narrow phase Rapier sobre meshes de colisão derivadas da tessellation; feedback em tempo real                  |
| 5          | `rcad-animation` | Timeline + keyframes editáveis; em qualquer frame disparar pedidos de análise (API async)                            |
| 6          | `rcad-cae`       | Adaptadores: malha → CalculiX (estrutural/fadiga/térmico) e OpenFOAM (fluidos); resultados como overlays no viewport |


WASM: após API `rcad-geom` estável; alvo secundário com backend OCCT-wasm ou Truck conforme maturidade — **não** no caminho crítico dos 6 meses.

---

## Organização open-source

- Monorepo em [bfonseca/Rcad](https://github.com/bfonseca/Rcad) (velocidade); crates com `README` e limites claros de API.
- Contribuição por módulo: labels `crate:geom`, `crate:sketch`, etc.
- POC anunciável à comunidade quando Fase A+B tiverem demo gravável (o remote já existe; não esperar assemblagens para o primeiro anúncio).
- Política: features grandes só via trait/adapters; sem acoplar UI a OCCT directamente.

---

## Riscos e mitigações


| Risco                                                   | Mitigação                                                                     |
| ------------------------------------------------------- | ----------------------------------------------------------------------------- |
| API Rust do OCCT incompleta (fillet, history, surfaces) | Camada `rcad-occt` com FFI/cxx pontual onde o wrapper falhar; issues upstream |
| Rebuild paramétrico / naming de faces frágil            | IDs estáveis próprios + testes de regressão em modelos golden                 |
| Scope creep “paridade Fusion”                           | Freeze de features do MVP; backlog estrito por prioridade                     |
| LGPL OCCT                                               | Documentar obrigações; manter OCCT relinkável                                 |
| 1–2 pessoas / 6 meses                                   | Cortar superfícies avançadas, sheet metal, drawings 2D, CAM                   |


---

## Primeiros passos de execução (após aprovação do plano)

1. Clonar [bfonseca/Rcad](https://github.com/bfonseca/Rcad) para [`/home/bruno/Projectos/Rcad`](/home/bruno/Projectos/Rcad) (se ainda não existir localmente) e alinhar com o remote — **não** fazer `git init` de um repo novo.
2. `move_agent_to_root` para esse directório e, se o repo estiver vazio ou só com README, criar o workspace Cargo com as crates `rcad-*`.
3. Implementar Fase A (viewport + OCCT hello-solid) e iterar Fase B/C conforme o critério de sucesso acima.
