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

### A1 — Configuração do render target
- [ ] Em `crates/rcad-app/src/main.rs`, configurar `eframe::NativeOptions` com
      `depth_buffer: 24`, `stencil_buffer: 8` e `multisampling: 4` (valor a confirmar na A0/A7).
- [ ] Confirmar que o egui continua a desenhar por cima sem artefactos.
- [ ] Limitar o pipeline novo aos sólidos; grelha, eixos, anotações e sketch continuam no egui.
- **Aceitação:** a app arranca sem erros de validação WGPU e o HUD do egui não muda.

### A2 — Pipeline WGPU do viewport
- [ ] Criar `crates/rcad-app/src/render/` com o `egui_wgpu::CallbackTrait`.
- [ ] Criar `crates/rcad-app/shaders/cad_material.wgsl`.
- [ ] Subir `TriangleMesh` para buffers de vértice/índice (sem copiar normais calculadas no CPU).
- [ ] Uniforms de câmara (ver `crates/rcad-app/src/camera.rs`); depth test/write; back-face culling.
- [ ] Passar as cores de face do projeto (`face_ids` → nome estável → `rcad.toml`) para o shader.
- **Aceitação:** o sólido aparece com profundidade correta; faces traseiras não aparecem; cores por face mantêm-se.

### A3 — Sombreado
- [ ] Blinn-Phong com luz principal fixa à câmara + ambiente hemisférico; especular discreto.
- [ ] Interpolação suave das normais por vértice (as normais do `cadrum` já são exatas e por vértice).
- [ ] Tinta de seleção/hover sem apagar a cor do projeto (comportamento atual a preservar).
- **Aceitação:** um cilindro mostra gradiente contínuo, não facetas por triângulo.

### A4 — Arestas e modo Shaded with Edges
- [ ] Pass 2: arestas B-Rep instanciadas como linhas finas, com depth bias (evitar z-fighting).
- [ ] `Shaded with Edges` como modo por omissão.
- [ ] Arestas tangentes atenuadas usando o ângulo diedral já calculado em `NamedEdge`
      (`crates/rcad-occt/src/lib.rs`, `edge_dihedral`).
- [ ] Arestas selecionadas/hovered com espessura/cor própria.
- **Aceitação:** caixa e furos com arestas nítidas; tangências discretas; sem linhas a “tremer”.

### A5 — Qualidade de tesselação configurável
- [ ] Expor a `Tessellation` do `cadrum` (deflexão linear/angular) no backend e no `rcad.toml`.
- [ ] Escolher um default mais fino que o atual (0,5 rad angular) e medir o custo em A0/D0.
- [ ] Cache de tesselação por shape/feature (evitar re-tesselar o que não mudou).
- **Aceitação:** o utilizador consegue escolher “qualidade de visualização” e vê o efeito; o custo está medido.

### A6 — Fallback e CI
- [ ] Manter o caminho `egui::Painter` para `--renderer glow` (máquinas sem GPU, CI, capturas).
- [ ] Garantir que `--screenshot` continua a produzir imagens com o renderer novo e com o fallback.
- [ ] Adicionar/ajustar teste de regressão de imagem (antes/depois) sem tornar o CI frágil.
- **Aceitação:** CI verde; capturas de ecrã disponíveis nos dois modos.

### A7 — Gate A (aceitação visual + números)
- [ ] Comparação lado a lado com as imagens A0 (sem facetas, com arestas, sem serrilhado).
- [ ] FPS e tempo de picking dentro do orçamento definido em D.
- [ ] Nenhuma regressão nos 280 testes; `cargo check --workspace --all-targets` com 0 avisos.
- **O utilizador valida visualmente antes de passar ao Bloco B.**

---

## 5. Bloco B — Geometria analítica (curvas e furos reais)

**Resultado para o utilizador:** extrudar um círculo dá um cilindro verdadeiro; um retângulo
com um círculo dentro dá uma chapa com furo verdadeiro; arcos e curvas deixam de desaparecer;
Loft/Boundary deixam de ser extrusões falsas.

**Depende de:** Gate A (para validar visualmente sobre geometria já correta).
**Tamanho:** L. **Risco:** alto (muda o formato dos documentos e toca em vários crates).

### B0 — Desenho do novo `Profile2D` e da migração
- [ ] Definir o novo modelo: **loops** fechados, cada um com segmentos analíticos
      (`Line`, `Arc`, `Circle`, `BSpline`; cónica e texto numa fase seguinte se necessário).
- [ ] Suportar **múltiplos loops** (outer + furos) com orientação e validação de fecho.
- [ ] Definir a migração dos documentos antigos (listas de pontos → sequências de linhas) e o
      bump de `Project::format_version`; `Project::load()` passa a migrar.
- **Aceitação:** documento antigo abre sem perder geometria; documento novo grava e relê.

### B1 — `Sketch::to_profile` com curvas
- [ ] Construir loops a partir de linhas **e** arcos/círculos/splines/cónicas (hoje só linhas/pontos).
- [ ] Detetar regiões fechadas e furos (ex.: círculo dentro de retângulo → loop interior).
- [ ] Manter compatibilidade com o que já funciona (retângulos, polylines, DXF/SVG).
- **Aceitação:** testes de sketch provam outer + furo; perfil inválido continua a falhar com mensagem.

### B2 — Backend com primitivas analíticas
- [ ] `GeometryBackend` passa a receber o perfil analítico (extrude/revolve/loft/sweep).
- [ ] `rcad-occt` usa `cadrum::Edge::line`, `circle`, `arc_3pts`, `bspline` em vez de `Edge::polygon`.
- [ ] Preservar o placement (`SketchPlacement`) e a associação ao sketch já existentes.
- **Aceitação:** teste de kernel verifica que um círculo extrudado produz **superfície cilíndrica**
      (não um conjunto de faces planas), e que um perfil com furo produz os dois wires.

### B3 — Features existentes sobre a nova geometria
- [ ] Extrude/Revolve com curvas; fillet/chamfer e booleanos continuam a funcionar.
- [ ] Reparar Loft/Boundary com `cadrum::Solid::loft`/`sweep` (hoje são extrusões falsas) — só depois do núcleo B1/B2 estável.
- [ ] Construção (datums) e MCP continuam a funcionar; `add_extrude` do MCP aceita o novo perfil.
- **Aceitação:** suite completa verde; MCP testado; documentos antigos migrados.

### B4 — Testes e migração
- [ ] Testes de kernel: círculo→cilindro; arco→superfície; retângulo+círculo→furo; loft/sweep reais.
- [ ] Teste de ida-e-volta de documento (gravar, reler, rebuild).
- [ ] Teste de migração de um ficheiro antigo real (guardado antes do B).
- **Aceitação:** nenhuma regressão nos testes existentes; golden model novo (área/volume) fixado.

### B5 — Gate B
- [ ] Demonstração ao utilizador: cilindro liso, furo real, Loft/Sweep corretos.
- [ ] Migração verificada; 0 avisos; performance de rebuild medida em D.

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

**A1** — configurar `eframe::NativeOptions` com depth/stencil/MSAA e começar o
pipeline WGPU do viewport. O A0 está concluído e o “antes” está congelado em
[foundation-baseline.md](foundation-baseline.md).

*Documento criado em 2026-10-17. Atualizar o estado das tarefas à medida que avançam.*
