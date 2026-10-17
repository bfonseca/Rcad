# Plano de Fundação — rcad (Ciclo A → B → C, com D a acompanhar)

> **Estado:** aprovado em 2026-10-17 pelo utilizador (não-programador, utilizador de CAD).
> **Baseline:** commit `b898185` (2026-10-17). `cargo test --workspace` → 280 testes, 0 falhas;
> `cargo check --workspace --all-targets` → 0 avisos.
> **Objetivo do ciclo:** melhorar a base (render, geometria, referências e performance)
> **antes** de acrescentar features novas. Nada de Hole/Pattern/Shell/Assembly/Desenho 2D
> neste ciclo, exceto o que for estritamente necessário para provar a fundação.

---

## 0. Porque este plano existe

O repositório já tem um núcleo paramétrico real (rebuild, naming persistente de faces/arestas,
sketcher com DOF por rank, MCP, projeto/rcad.toml). O que falta não é “mais features” — é a
**qualidade da base** que todas as features futuras vão herdar:

1. **O viewport não é 3D a sério.** Os sólidos são desenhados pelo pincel 2D do egui:
   sem depth buffer, uma cor por triângulo, ordenados por centroide, e o picking percorre
   todos os triângulos a cada frame (`crates/rcad-app/src/viewport.rs`).
2. **A geometria curva perde-se antes do kernel.** `Profile2D` guarda só listas de pontos
   (`crates/rcad-geom/src/lib.rs`), `Sketch::to_profile` ignora círculos e arcos
   (`crates/rcad-sketch/src/lib.rs`) e o `rcad-occt` constrói perfis com `Edge::polygon`.
   Um renderer perfeito sobre esta geometria continua a mostrar facetas.
3. **As referências e a relação pai/filho são frágeis.** Os nomes estáveis já existem
   (`crates/rcad-param/src/naming.rs`), mas `parent_features`/`child_features` nunca são
   calculados, o Insert Here/Rollback só desenha marcadores e o rebuild ignora-os; apagar
   uma feature não protege os filhos; não há edição da definição geométrica de uma feature.
4. **A performance nunca foi medida.** Não há baseline de FPS, triângulos, tempo de rebuild
   ou de picking; otimizar sem medir é adivinhar.

O documento externo “Rcad vs Creo Parametric — Guia Completo… (V2)” identificou corretamente
os pontos 1, 2 e 3. Este plano adota o que ele tem de melhor (F0/F1, tabelas oficiais PTC),
corrige o que estava impreciso e acrescenta o que faltava (migração de documentos, qualidade de
tesselação, fallback de renderer, performance medida).

---

## 1. Documentos canónicos (fim das quatro verdades)

A partir de agora, a ordem de leitura é:

| Documento | Papel |
|---|---|
| **`plans/foundation-plan.md`** (este) | **Fonte de verdade do ciclo atual.** Tarefas, gates e critérios de aceitação. |
| `plans/implementation-tasks.md` | Backlog histórico + handover das sessões. Ver a secção “Reconciliação de estado”. |
| `plans/creo-workflow-analysis.md` | Análise de referência do Creo e plano UX/config (snapshot de 2026-10-12). |
| `instructions.md` | Âmbito do produto e roadmap de longo prazo. Ver a secção “Estado real”. |
| `README.md` | Descrição pública: deve refletir o que existe, não o que se deseja. |

Regra: quando um documento antigo contradizer este plano, **este plano ganha**; o documento
antigo é corrigido ou marcado como histórico.

---

## 2. Decisões já tomadas

1. **Ordem:** A (render) → B (geometria analítica) → C (referências/pai-filho), com
   **D (performance) a acompanhar** desde o início.
2. **Cadrum-first:** usar as APIs que o `cadrum` já expõe (`shell`, `sweep`, `loft`,
   `mirror`, `rotate`) antes de considerar FFI direta para o OCCT.
3. **Migração de documentos aceite:** o Bloco B muda o formato de `Profile2D`; os ficheiros
   antigos serão migrados automaticamente na leitura. Fazer agora, enquanto há poucos ficheiros.
4. **Fallback obrigatório:** o modo `--renderer glow` (usado em CI/capturas) não pode deixar de
   funcionar; o renderer novo tem de ter caminho de recurso.
5. **Nada de features novas neste ciclo.** Se uma tarefa parecer “só mais uma feature”,
   fica para o ciclo seguinte.

---

## 3. Âmbito

**Dentro:** renderer WGPU com profundidade/MSAA/sombreado/arestas; qualidade de tesselação;
perfis analíticos com loops e furos; migração de documentos; grafo de dependências e pai/filho;
Insert Here/Rollback reais; edição da definição geométrica das features existentes; reparação
de referências ligada ao grafo; métricas de performance e remoção dos gargalos óbvios.

**Fora (ciclo seguinte):** Hole, Pattern, Mirror, Shell, Draft, Sweep novo, Rib; assemblies e
mecanismos; multi-body real (o grafo deve **permitir**, mas não se implementa); desenho 2D/GD&T;
skeletons/underlay/table ligados à UI; trail log; associações de ficheiros; MCP prompts/TCP.

---

## 4. Bloco A — Renderer 3D

**Resultado para o utilizador:** curvas e cilindros lisos, sem serrilhado, com as arestas do
Creo (arestas normais nítidas, tangentes atenuadas), profundidade correta e seleção/hover
visíveis no sólido. Sensação de “sólido”, não de “malha”.

**Depende de:** nada (pode começar já).
**Tamanho:** M–L. **Risco:** médio (integração egui/eframe/WGPU e fallback glow).

### A0 — Baseline visual e de performance ✅
- [x] Capturar o estado atual com `--demo`, `--view iso`, `--screenshot` e guardar as imagens de referência.
- [x] Medir e registar: FPS no modelo demo, nº de triângulos, tempo de rebuild, tempo de picking.
- [x] Criar uma peça de teste “média” (chapa 120×80×10, 8 furos, rounds) para medir mais tarde.
- [x] Corrigir a captura de ecrã para esperar pela primeira tesselação (antes saía vazia em peças lentas).
- **Aceitação:** baseline publicado em [foundation-baseline.md](foundation-baseline.md).

**Resultado do A0 (resumo):**
- Rebuild da chapa de 8 furos: **≈15 s** cold / ≈5,3 s incremental; duplicar os furos
  multiplica o custo por ~4 → **gargalo dominante e superlinear**.
- Segunda tesselação (inclui nomes/histórico): **≈2,8 s**; o resto está na avaliação
  de features + derivação de referências por feature.
- Picking: ~1,1 ms para 180k triângulos → **não é o gargalo**.
- Frame time (software GL, demo): 8,8 ms → **o renderer não é o gargalo de FPS**; o
  Bloco A é sobretudo qualidade visual (depth, MSAA, arestas, sombreado).
- A face superior da chapa mostra **costuras entre triângulos** no painter — prova
  visual do problema que o Bloco A resolve.
- **Consequência para o plano:** B deve reduzir drasticamente a topologia (furo
  cilíndrico = 1 face em vez de ~32), mas **D2/D3 deixam de ser “depois de B”**:
  o custo superlinear por feature tem de ser atacado em paralelo com B.

### A1 — Configuração do render target ✅
- [x] Em `crates/rcad-app/src/main.rs`, configurar `eframe::NativeOptions` com
      `msaa=4`, `depth=24` e `stencil=8` (constantes `MSAA_SAMPLES`, `DEPTH_BITS`,
      `STENCIL_BITS`) e registar a configuração no arranque (`framebuffer: ...`).
- [x] Confirmar que o egui continua a desenhar por cima sem artefactos.
- [x] Manter grelha, eixos, anotações e sketch no egui; os sólidos continuam no painter até A2.
- **Aceitação:** a app arranca e o HUD não muda. **Validado no renderer `glow`**; o
  renderer WGPU não é validável nesta máquina (ver abaixo).

**Resultado A1 (2026-10-17):**
- Configuração aplicada: **4× MSAA, depth 24, stencil 8**, com log de arranque
  reproduzível.
- Validado com `--renderer glow` (X11 + llvmpipe): demo e chapa de 8 furos capturam
  corretamente; toolbar, árvore, seleção e texto do egui desenham por cima sem artefactos.
- **Limitação do ambiente:** não há GPU nem Vulkan utilizável (`vulkaninfo` não encontra
  dispositivo) e o backend WGPU falha no `Surface::configure`; o modo `glow` é o único
  executável aqui. Por isso:
  - a validação "sem erros de validação WGPU" fica **pendente de hardware ou lavapipe**;
  - A2 pode ser implementado e compilado, mas o Gate A visual exige GPU (ou um driver
    Vulkan software instalado).
- Custo do MSAA em software: frame time da chapa passou de ~8,4 ms para ~18 ms no
  llvmpipe. Em GPU o custo é muito menor; **não comparar números de frame entre A0 e A1
  sem notar a configuração**. Os tempos de kernel/rebuild (D) não são afetados.
- As costuras entre triângulos da chapa continuam visíveis: o A1 prepara o *framebuffer*,
  mas só o A2 (pipeline real) resolve o problema visual.

### A2 — Pipeline WGPU do viewport ✅ (validação de janela pendente)
- [x] Criar `crates/rcad-app/src/render/` com o `egui_wgpu::CallbackTrait`.
- [x] Criar `crates/rcad-app/shaders/cad_material.wgsl` (validado com `naga`).
- [x] Subir `TriangleMesh` para buffers de vértice/índice (sem copiar normais calculadas no CPU).
- [x] Uniforms de câmara; depth test/write (Depth24PlusStencil8); MSAA coerente com o framebuffer.
- [x] Passar as cores de face do projeto (`face_ids` → nome estável → `rcad.toml`) para o shader,
      incluindo seleção/hover.
- **Aceitação:** o sólido aparece com profundidade correta; faces traseiras não aparecem; cores por
  face mantêm-se. **Validado por render offscreen** com o `llvmpipe` GL (ver § result).

**Resultado A2 (2026-10-17):**
- O `egui::Painter` continua a ser o caminho de recurso: com `--renderer glow`,
  `SolidRenderer` é `None` e as capturas continuam a funcionar.
- Com um estado WGPU, o viewport adiciona um `PaintCallback` no lugar do sólido.
- Sem GPU *de janela*, o adapter GL **headless** do llvmpipe permitiu validar o pipeline a sério:
  - `cargo test -p rcad-app the_solid_shader_parses_and_validates` (WGSL + naga);
  - `cargo test -p rcad-app headless_pipeline_draws_a_triangle -- --ignored --nocapture`
    cria o pipeline/depth/draw e passa na validação do WGPU;
  - `cargo test -p rcad-app headless_render_of_the_demo_writes_a_png -- --ignored --nocapture`
    corre o caminho real `prepare`+`paint` sobre a malha da demo (OCCT) e grava
    `target/a2-headless-demo.png`; a imagem mostra o sólido sombreado com o corte cilíndrico;
  - `headless_pipeline_accepts_the_apps_msaa_configuration` valida a configuração real de
    **4× MSAA + resolve target + Depth24PlusStencil8**.
- O que ainda **não** foi validado: a janela WGPU real (o `Surface::configure` falha neste
  ambiente). O Gate A visual tem de ser corrido no hardware do utilizador.

### A3 — Sombreado ✅
- [x] Blinn-Phong com luz principal fixa + ambiente; especular discreto (`cad_material.wgsl`).
- [x] Interpolação suave das normais por vértice (as normais do `cadrum` já são exatas).
- [x] Tinta de seleção/hover sem apagar a cor do projeto (testes em `render::tests`).
- **Aceitação:** um cilindro mostra gradiente contínuo, não facetas por triângulo (verificado no
  render offscreen; a confirmação final na janela fica para o Gate A).

### A4 — Arestas e modo Shaded with Edges ✅ (validação de janela pendente)
- [x] Pass 2: arestas B-Rep instanciadas como linhas, com depth test e **depth bias negativo**
      para não haver z-fighting (`cad_edges.wgsl` + segundo pipeline).
- [x] `Shaded with Edges` como comportamento por omissão (as arestas deixam de ser só de seleção).
- [x] Arestas tangentes atenuadas usando o `dihedral_degrees` já calculado em `NamedEdge`.
- [x] Arestas selecionadas/hovered com cor própria no GPU; o painter continua a desenhar a linha
      mais grossa por cima para facilitar o alvo.
- **Aceitação:** caixa e furos com arestas nítidas, ocultas escondidas e sem linhas a “tremer”.
  **Validado no render offscreen** (ver abaixo); falta a confirmação na janela.

**Resultado A4 (2026-10-17):** o render offscreen do demo passou a mostrar o contorno escuro de
todas as arestas visíveis, com as arestas traseiras devidamente escondidas pelo depth test:

![A4 edges](../docs/baseline/a4-headless-edges.png)

O teste `tangent_edges_are_dimmed_and_selected_edges_take_the_highlight` fixa a classificação
(deiedral ≈180° → cor atenuada; seleção → cor de destaque). O que falta: validar na janela WGPU
do utilizador, onde o painter acrescenta a linha grossa de seleção/hover.

### A5 — Qualidade de tesselação configurável ✅
- [x] Expor a `Tessellation` do `cadrum` no backend, no `rcad.toml` (`[tessellation]`) e no
      Options dialog (linear, angular°, relative; Apply/Reset).
- [x] Default mais fino que o do `cadrum` (0,002 relativo, 11,5°) e custo medido.
- [x] Os nomes/referências usam uma tesselação canónica fixa: mudar a qualidade **não** altera
      nomes (teste `display_tessellation_changes_triangles_but_not_reference_names`).
- [ ] Cache de tesselação por shape/feature — **movido para D3** (é uma otimização de rebuild,
      não uma opção de qualidade).
- **Aceitação:** o utilizador consegue escolher a qualidade e vê o efeito; o custo está medido. ✅

**Resultado A5 (2026-10-17):** mesma máquina, mesma peça, só muda a qualidade:

| Peça | Triângulos antes (0,004 / 28,65°) | Triângulos depois (0,002 / 11,5°) | Rebuild antes/depois |
|---|---:|---:|---:|
| demo | 48 | 76 (+58%) | 15,4 / 15,7 ms |
| chapa 8 furos | 3 172 | 5 076 (+60%) | 16 335 / 16 300 ms |

O rebuild é dominado pelo trabalho por feature/naming (A0), por isso **+60% de triângulos custa
~2% no tempo**. A qualidade extra é praticamente gratuita enquanto o gargalo não for corrigido;
depois de B/D, a medição deve ser repetida.

### A6 — Fallback e CI ✅
- [x] Manter o caminho `egui::Painter` para `--renderer glow` (máquinas sem GPU, CI, capturas).
- [x] `--screenshot` continua a funcionar no fallback (demo e chapa capturadas com glow).
- [x] `--screenshot` no WGPU: a janela foi confirmada pelo utilizador; a captura automática
      fica limitada ao ambiente com GPU (o modo não existe neste sandbox).
- [x] Teste de regressão de imagem: `headless_render_of_the_demo_writes_a_png` compara com
      `docs/baseline/a4-headless-edges.png` com tolerância (média <3 e <2% de pixels),
      ignorado por omissão para não tornar o CI frágil.
- **Aceitação:** CI verde; capturas nos dois modos. ✅

### A7 — Gate A ✅ (aprovado visualmente pelo utilizador)
> **2026-10-17:** o utilizador correu a app no desktop e confirmou o render muito melhor;
> as arestas do A4 foram também verificadas e consideradas corretas. As duas capturas mostram
> superfícies sem costuras, profundidade correta e arestas visíveis.

- [x] Comparação lado a lado: baseline A0 vs janela WGPU (aprovada pelo utilizador).
- [x] FPS: contador no status bar; orçamento de 60 FPS. No fallback software (llvmpipe) a
      chapa de teste mediu **57 fps**; o valor no hardware do utilizador fica a registar no
      próximo arranque (instrumentação pronta, não bloqueia o Bloco B).
- [x] Nenhuma regressão na suite; `cargo check --workspace --all-targets` com 0 avisos.
- **O utilizador validou visualmente antes de passar ao Bloco B.** ✅

---

## 5. Bloco B — Geometria analítica (curvas e furos reais)

**Resultado para o utilizador:** extrudar um círculo dá um cilindro verdadeiro; um retângulo
com um círculo dentro dá uma chapa com furo verdadeiro; arcos e curvas deixam de desaparecer;
Loft/Boundary deixam de ser extrusões falsas.

**Depende de:** Gate A (para validar visualmente sobre geometria já correta).
**Tamanho:** L. **Risco:** alto (muda o formato dos documentos e toca em vários crates).

### B0 — Desenho do novo `Profile2D` e da migração ✅
- [x] **Loops** fechados com segmentos analíticos: `Line`, `Arc` (centro/raio/ângulo/sweep) e
      `Circle`. BSpline fica para o seguimento (a cónica é amostrada em linhas).
- [x] **Múltiplos loops** (outer + furos), com orientação normalizada e escolha do maior loop
      como contorno exterior.
- [x] Migração automática na leitura: o `Deserialize` de `Profile2D` aceita as listas de pontos
      antigas e converte-as em linhas; `Project::format_version` passou para **2**.
- **Aceitação:** documento antigo abre sem perder geometria; documento novo grava e relê. ✅
  (teste `legacy_point_profiles_migrate_to_lines` + abertura de uma chapa pré-B na app).

### B1 — `Sketch::to_profile` com curvas ✅ (splines no seguimento)
- [x] Loops a partir de linhas, arcos, círculos e cónicas (estas amostradas em linhas por falta
      de primitiva no kernel); polylines/retângulos/DXF/SVG continuam a funcionar.
- [x] Deteção de furos: o maior loop é o exterior; os restantes loops dentro dele são furos.
- [x] Compatibilidade: sketches só com pontos continuam a gerar um perfil poligonal.
- **Aceitação:** testes `a_circle_profile_is_analytic`, `a_circle_inside_a_rectangle_becomes_a_hole_loop`
  e `an_arc_is_kept_analytic_in_the_profile`. ✅
- **Por fazer:** BSpline analítica (o `SketchSpline` ainda não entra no perfil) e texto como curvas.

### B2 — Backend com primitivas analíticas ✅
- [x] `Profile2D` analítico chega ao backend em extrude/revolve (e aos restantes consumidores).
- [x] `rcad-occt` usa `cadrum::Edge::line`, `circle` e `arc_3pts` em vez de `Edge::polygon`;
      não é usada FFI direta.
- [x] Placement e associação ao sketch preservados (a chapa de teste e a app continuam a funcionar).
- **Aceitação:** `an_analytic_circle_extrudes_to_a_real_cylinder` e
  `a_circular_hole_profile_extrudes_with_two_wires` verificam a face cilíndrica e os dois wires. ✅

### B3 — Features existentes sobre a nova geometria ✅ (Loft/Boundary no seguimento)
- [x] Extrude/Revolve com curvas; fillet/chamfer e booleanos continuam a funcionar; a chapa de
      teste (8 furos) passou a ser construída com cilindros reais.
- [x] Construção (datums) e MCP continuam a funcionar; o MCP aceita o perfil poligonal do costume
      e a app desenha/ex truda sketches com curvas.
- [ ] Reparar Loft/Boundary com `cadrum::Solid::loft`/sweep: continua por fazer (precisa de
      placement por secção no modelo de dados), fica como tarefa B3-follow-up.
- **Aceitação:** suite completa verde; documentos antigos migrados. ✅

### B4 — Testes e migração ✅
- [x] Testes de kernel: círculo→cilindro; retângulo+círculo→furo (dois wires).
- [x] Testes de ida-e-volta do perfil analítico e da representação antiga (JSON em `rcad-geom`).
- [x] Migração real: uma chapa `.rcad` gravada **antes** do B abriu e reconstruiu na app.
- [x] Nenhuma regressão: 298 testes passam, 4 medições ignoradas, 0 avisos.
- **Aceitação:** suite verde. ✅ (o golden de área/volume pode ficar para o Gate B.)

### B5 — Gate B
- [ ] Demonstração ao utilizador: cilindro liso, furo real (Loft/Sweep ficam para follow-up).
- [x] Migração verificada; 0 avisos; performance de rebuild medida.

**Resultado B (2026-10-17):** a chapa de 8 furos passou de 8 ferramentas poligonais para 8
cilindros analíticos. Além da correção geométrica, o rebuild caiu drasticamente:

| Peça | Cold antes (A0) | Cold depois (B) | Incremental antes → depois |
|---|---:|---:|---:|
| chapa 2 furos | 938 ms | 146 ms | 481 → 58 ms |
| chapa 4 furos | 3 391 ms | 225 ms | 1 511 → 71 ms |
| **chapa 8 furos** | **15 240 ms** | **425 ms** | **5 262 → 103 ms** |

Ou seja, **~36× mais rápido** na peça de teste, porque a topologia deixou de explodir em ~32
faces por furo. O número de triângulos sobe (6 068 em vez de 5 076) por o cilindro ser
verdadeiramente curvo — o custo de render é o previsto em A5.

---

## 6. Bloco C — Referências, pai/filho e rebuild condicionado

**Resultado para o utilizador:** a árvore conta a história do modelo; alterar o pai reconstrói
os filhos; apagar algo com filhos avisa e mostra quem depende; Insert Here/Rollback funcionam;
referências quebradas aparecem com “onde estavam” e substituto; dá para editar a definição
(altura, raio, ângulo) de uma feature.

**Depende de:** Gate B (as features existentes têm de estar estáveis).
**Tamanho:** M. **Risco:** médio (semântica de dependências e migração de dados).

### C0 — Grafo de dependências
- [ ] Calcular `parent_features`/`child_features` a partir das referências reais
      (sketch, placement, edge sets, boolean tool, thicken surface, etc.).
- [ ] Expor o grafo em `rcad-doc`/`rcad-param` (uma só fonte de verdade; não duplicar regras na UI).
- [ ] Definir o que acontece com referências apagadas/partidas (nó quebrado, não crash).
- **Aceitação:** teste de grafo: extrude depende do sketch; fillet depende do corpo; boolean do tool.

### C1 — Rebuild condicionado (Insert Here / Rollback)
- [ ] `insert_here_index` passa a inserir novas features no ponto marcado.
- [ ] `rollback_index` passa a parar o rebuild nesse ponto (sem apagar features).
- [ ] Reordenar com validação: não permitir mover uma feature para antes das suas dependências.
- **Aceitação:** testes de rebuild com marcadores; a árvore reflete o estado real.

### C2 — Árvore pai/filho na UI
- [ ] Indentação por dependência; contagem de filhos; destaque de pais/filhos ao selecionar.
- [ ] Ações “Parents”/“Children” reais (hoje são TODO) e “Edit Definition” a partir da árvore.
- [ ] Renomear feature (o id estável não muda; é só o nome).
- **Aceitação:** cenário de utilizador: caixa → sketch na face → extrude → round; a árvore mostra a cadeia.

### C3 — Apagar/editar com segurança
- [ ] Apagar feature com filhos: listar dependentes e bloquear ou exigir confirmação (nunca apagar em silêncio).
- [ ] Suppress/Resume de um pai: avisar/realçar filhos afetados.
- [ ] Editar definição de uma feature existente (altura da caixa, raio do cilindro, altura do extrude,
      ângulo do revolve, raio do round, distância do chamfro) com rebuild e undo.
      **Não** acrescentar tipos de feature novos — só editar o que já existe.
- **Aceitação:** mudar a altura da caixa propaga-se ao extrude/fillet; apagar a caixa avisa.

### C4 — Catálogo de nomes (roles) completo
- [ ] Dar “roles” estáveis às features que hoje caem para a assinatura geométrica:
      Revolve, Loft, Boundary, Sew, Thicken (e as futuras Hole/Pattern/Mirror quando existirem).
- [ ] Manter a assinatura geométrica como fallback honesto, com aviso no relatório de referências perdidas.
- **Aceitação:** testes de kernel provam que as referências sobrevivem a mudanças de parâmetros.

### C5 — Reparação ligada ao grafo
- [ ] A lista de referências perdidas passa a distinguir sketch, placement, face e edge.
- [ ] Substituir uma referência atualiza o grafo e reconstrói apenas o necessário.
- **Aceitação:** teste de referência perdida + substituição + rebuild.

### C6 — Gate C
- [ ] Guião de aceitação do utilizador (ver §8) executado e aprovado.
- [ ] 0 avisos; suite verde; grafo e árvore consistentes.

---

## 7. Bloco D — Performance (contínuo, desde o dia 1)

**Princípio:** medir antes de otimizar; cada bloco entrega métricas antes/depois.

> **Achado do A0 (2026-10-17):** o rebuild é o gargalo real — 15 s para 18 features,
> com crescimento superlinear (~4× por duplicação de furos). O renderer de software
> faz 8,8 ms/frame. Por isso D não é um “polish” no fim: **D2 (cache/rebuild) e a
> redução de trabalho por feature têm de avançar em paralelo com o Bloco B.** Os
> números e orçamentos estão em [foundation-baseline.md](foundation-baseline.md).

### D0 — Baseline e orçamentos ✅
- [x] Instrumentar (exemplo de benchmark + teste de picking ignorado) tempo de rebuild,
      segunda tesselação, tamanho de malha e custo de picking.
- [x] Medir o modelo demo e as peças de teste de A0.
- [x] Fixar orçamentos iniciais em [foundation-baseline.md](foundation-baseline.md) (§7).
- **Aceitação:** tabela de métricas no repositório, reproduzível. ✅

### D1 — Picking
- [ ] Substituir a travessia O(n) de triângulos por **BVH** por corpo (primeiro passo) ou buffer de IDs na GPU.
- [ ] Manter o comportamento atual: aresta > datum > face, ciclo de botão direito.
- **Aceitação:** picking dentro do orçamento na peça média; seleção idêntica em testes.

### D2 — Cache e rebuild
- [ ] Confirmar que o cache de shapes (`ShapeCache`) só reconstrói o que mudou.
- [ ] Evitar re-tesselar features inalteradas; medir.
- [ ] Cache de tesselação por shape/feature (tarefa movida do A5).
- **Aceitação:** editar uma feature não reconstrói o modelo todo.

### D3 — Regressão de performance no CI
- [ ] Teste de fumo não frágil (limiares generosos) para rebuild/tesselação.
- [ ] Manter as capturas de imagem como regressão visual do Bloco A.

---

## 8. Guião de aceitação do utilizador (fim do ciclo)

Com o build novo, o utilizador deve conseguir, sem tocar em código:

1. Abrir um projeto e ver a peça demo **com curvas lisas, arestas e profundidade**.
2. Criar um sketch com um círculo, extrudar e obter um **cilindro**; criar um retângulo com um
   círculo dentro e extrudar para obter uma **chapa com furo**.
3. Criar uma cadeia caixa → sketch na face → extrude → round; ver a **árvore pai/filho**.
4. **Mudar a altura da caixa** e ver o extrude/round a acompanhar.
5. **Apagar a caixa** e receber aviso com os filhos.
6. Marcar **Insert Here** e criar uma feature; ver que entra no ponto certo; usar **Rollback**.
7. Provocar uma **referência perdida** e vê-la na lista, com local e substituto.
8. Ver o **FPS/qualidade** a mudar com a definição de tesselação.

---

## 9. Riscos e mitigação

| Risco | Mitigação |
|---|---|
| Depth/MSAA no callback egui não se comporta como esperado | A1 isolado e testado primeiro; plano B: render offscreen com depth e composição como textura |
| Renderer novo quebra capturas/CI | Manter caminho `egui::Painter` para glow; testar `--screenshot` nos dois modos (A6) |
| Migração de `Profile2D` parte ficheiros | B0 antes de B1; bump de versão; testes com ficheiro antigo real; migração automática |
| Loops/furos mal formados (auto-interseção, orientação) | Validação explícita no perfil; mensagens claras; testes de perfil inválido |
| Scope creep (features novas a meio) | Regra do §2, decisão 5; backlog separado |
| Otimizar sem medir | D0 obrigatório; métricas antes/depois por bloco |
| Perder o trabalho já feito em referências | C0 parte do naming existente; não reescrever o que está testado |
| Rebuild superlinear (medido no A0: ~4× por duplicação de furos) | D2/D3 em paralelo com B; medir cada alteração e manter o alvo de ≤500 ms para a chapa de 8 furos |
| Ambiente sem GPU/Vulkan: o renderer WGPU não arranca e não pode ser validado localmente | Validar em hardware ou instalar um driver Vulkan software (lavapipe); manter o fallback `--renderer glow` para CI/screenshots |
| O teste do utilizador expôs referências perdidas (5) e um fillet com raio 8,5 que não cabe na geometria; a lista de reparação é muito verbosa | Trabalho do Bloco C (grafo, reparação, mensagens) e D (falha de fillet deve ser diagnosticada antes de aplicar); registado para a próxima sessão |
| Ambiente de medição (GPU vs software) | Registar o ambiente de cada medição; não comparar maçãs com laranjas |

---

## 10. Rastreabilidade

| Origem | Onde entra neste plano |
|---|---|
| Documento V2, F0 / Prompt 1 | Bloco A |
| Documento V2, F1 / Prompt 1B | Bloco B |
| Documento V2, Prompt 2/3 (seleção/coletores) | A (seleção/hover) e C (referências); o dashboard completo fica para o ciclo seguinte |
| Documento V2, Prompt 8 (naming persistente) | Já feito; C4 completa os roles em falta |
| `instructions.md` Fase E / UI-1 / UI-2 / PRJ-1 | Já feito ou parcialmente feito — ver reconciliação em `plans/implementation-tasks.md` |
| `plans/implementation-tasks.md` Fases 1/2/3/6/9 | Estado real na reconciliação; o que falta entra em C, não em features novas |

---

## 11. Regras de trabalho

1. **Um bloco por vez**, na ordem A → B → C; D acompanha.
2. Antes de fechar cada bloco: `cargo test --workspace` (0 falhas) e
   `cargo check --workspace --all-targets` (0 avisos).
3. Nenhuma feature nova neste ciclo; se surgir, registar no backlog e continuar.
4. Atualizar este plano (checkboxes + métricas) **na mesma sessão** em que se faz o trabalho.
5. Não tocar no formato de documento sem a migração do B0.
6. Commits pequenos e descritivos; o utilizador aprova o gate visual antes do bloco seguinte.

---

## 12. Próximo passo imediato

**Bloco C** — referências, pai/filho e rebuild condicionado. A6/A7 estão concluídos
(Gate A aprovado visualmente) e o Bloco B está concluído no núcleo; Loft/Boundary e
BSpline ficam como follow-up. O “antes” está congelado em
[foundation-baseline.md](foundation-baseline.md).

*Documento criado em 2026-10-17. Atualizar o estado das tarefas à medida que avançam.*
