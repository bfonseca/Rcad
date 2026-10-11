# Plano de Atualização do Ficheiro `instructions.md`

Este documento sistematiza todas as modificações, refinamentos e adições que serão integrados no ficheiro `instructions.md` para o desenvolvimento do CAD paramétrico **rcad**.

---

## 1. Resumo Executivo das Alterações a Realizar

No ficheiro `instructions.md` serão efetuadas as seguintes grandes alterações:

1. **Revisão da Filosofia UI/UX (Otimização para 16:9):** Remoção de qualquer referência a menus tipo *Ribbon* superior e introdução da barra de ferramentas vertical lateral colapsável e *dockable*.
2. **Nova Árvore de Histórico Híbrida Adaptativa (Creo + CATIA):** Modificação da especificação da árvore para funcionar como um *overlay* transparente que reage ao cursor (transparente e não obstrutivo fora, escurecido com *blur* no *hover*).
3. **Gestão Centrada em Projeto (`rcad.toml`):** Substituição do conceito tradicional de *Working Directory* por um manifesto humano de projeto que armazena tolerâncias, parâmetros e configurações globais.
4. **Mapeamento Dinâmico de Cores de Superfície por Cliente (RGB):** Adição de uma especificação para atrelar códigos de cores RGB aos tipos de superfície conforme as normas do cliente.
5. **Integração de Tabelas Externas Excel/CSV (`rcad-table`):** Inclusão de suporte nativo via Rust (crate `calamine`) para parametrização por catálogos e tabelas de fabricantes.
6. **Aprofundamento do *Intent Manager* e *Skeletons*:** Detalhamento explícito do comportamento de cotas fracas/fortes no Sketcher 2D e do workflow Top-Down.

---

## 2. Detalhe das Alterações e Adições por Secção do `instructions.md`

### 2.1 Secção: "Confirmação do âmbito"

* **O que muda / é adicionado:**
  * Atualização da tabela de decisões para incluir:
    * **Otimização UI:** Lay-out vertical lateral para ecrãs 16:9 / 16:10.
    * **Gestão de Projeto:** Ficheiro manifesto `rcad.toml` na raiz.
    * **Tabelas Externas:** Leitura nativa de ficheiros Excel/CSV.
* **Intenção:** Refletir imediatamente na tabela de âmbito as novas diretrizes de UI e arquitetura de dados acordadas, garantindo alinhamento desde o primeiro nível de leitura.

---

### 2.2 Secção: "Arquitectura modular"

* **O que muda / é adicionado:**
  * **Diagrama Mermaid:** Inclusão da crate `rcad_table` no bloco `core` e estabelecimento da relação de dependência com `rcad_param`.
  * **Tabela de Stack Tecnológica:**
    * Adição do módulo `calamine` (para parsing de ficheiros `.xlsx` / `.csv` sem dependência do MS Excel).
    * Clarificação do suporte a shaders `wgpu` para efeitos visuais na UI (fundo adaptativo com *blur*).
* **Intenção:** Garantir que a arquitetura modular em Rust prevê explicitamente a leitura de tabelas de parâmetros e os requisitos de renderização do ecrã sem quebrar a separação de responsabilidades das crates.

---

### 2.3 Secção: "Modelo de dados (núcleo)"

* **O que muda / é adicionado:**
  * **Subsecção: Gestão de Projeto via `rcad.toml`:**
    * Adição do esquema e exemplo em TOML do manifesto de projeto.
    * Inclusão dos campos de tolerância geral (`general_tolerance`) e parâmetros globais.
  * **Subsecção: Mapeamento de Cores de Superfície por Cliente:**
    * Inclusão de especificação onde os tipos lógicos de superfície (ex: roscas, superfícies de ajuste, zonas de vedação) são associados a valores RGB definidos no `rcad.toml`.
  * **Subsecção: Componentes Guiados por Tabelas Externas:**
    * Especificação da ligação entre a feature da peça e tabelas externas (`.xlsx`/`.csv`) para validação e extração de dimensões.
* **Intenção:** Resolver pontos críticos de fricção do trabalho diário de engenharia: automação de padrões visuais por cliente e eliminação da introdução manual repetitiva de cotas de peças standard.

---

### 2.4 Secção: "Ambiente Gráfico e UI/UX" (Nova Secção / Expansão)

* **O que muda / é adicionado:**
  * **Barra de Ferramentas Vertical Dockable:**
    * Posição padrão à esquerda com capacidade de transferência para a direita (*dockable*).
    * Alternância de vista: *Apenas Ícones* vs. *Ícones + Texto*.
  * **Árvore de Histórico Híbrida Adaptativa:**
    * Funcionamento em *Overlay* transparente diretamente sobre o Viewport 3D.
    * **Comportamento Interativo:** Quando o cursor do rato entra na zona da árvore, o fundo escurece ligeiramente e aplica um efeito de *blur* via `wgpu` para máxima legibilidade; fora dessa zona, torna-se 100% transparente com texto suavizado, permitindo plena navegação 3D.
    * Estruturação estrita estilo Creo (*Parent/Child*, modo *Insert Here*, *Suppress/Resume*, *Edit Definition*).
* **Intenção:** Aproveitar ao máximo a área vertical dos monitores modernos (16:9), oferecendo uma experiência de trabalho limpa, imersiva e sem distrações visuais, combinando a clareza do Creo com a estética do CATIA.

---

### 2.5 Secção: "Parametrização e Metodologia Creo"

* **O que muda / é adicionado:**
  * Detalhamento do **Intent Manager** no `rcad-sketch`:
    * Implementação de **Cotas e Restrições Fracas** geradas automaticamente pelo solver `rust_slvs` para garantir que nenhum sketch fica sub-restringido.
    * Substituição automática de cotas fracas por **Cotas Fortes** introduzidas pelo utilizador.
  * Detalhamento do workflow de **Skeletons**:
    * Mecanismos explícitos de *Publish Geometry* e *Copy Geometry* entre a peça Skeleton e as peças constituintes do projeto.
* **Intenção:** Assegurar que a modelação paramétrica é previsível e robusta a alterações, evitando falhas em cadeia ao redefinir dimensões.

---

### 2.6 Secção: "MVP a 6 meses (utilizável)"

* **O que muda / é adicionado:**
  * **Fase A (Semanas 1–4):**
    * Inclusão do desenvolvimento do layout 16:9 com barra vertical dockable.
    * Implementação da árvore transparente adaptativa com shader de *blur*.
    * Parser inicial do `rcad.toml` (tolerâncias globais e cores RGB por cliente).
  * **Fase B (Semanas 5–14):**
    * Inclusão da crate `rcad-table` para suporte a tabelas Excel.
    * Implementação do *Intent Manager* no Sketcher (cotas fracas vs. fortes).
* **Intenção:** Realinhar os marcos temporais de desenvolvimento para que a estrutura visual e de projeto (fundamentais para a experiência do utilizador) fiquem consolidadas desde a primeira fase do MVP.

---

### 2.7 Secção: "Riscos e mitigações"

* **O que muda / é adicionado:**
  * Inclusão do risco de performance no efeito de *blur* na árvore de histórico e sua mitigação (uso de shader leve em `wgpu` ativado estritamente durante eventos de *hover*).
  * Inclusão do risco de complexidade na gestão de cotas fracas no solver 2D e sua mitigação (geração de restrições virtuais temporárias geridas pelo `rcad-sketch`).
* **Intenção:** Prever possíveis gargalos de desempenho visual ou lógico e definir estratégias de resolução antes do início da implementação do código.
```

---

O ficheiro `updates.md` foi gerado com sucesso. Como pretendes proceder? Se aprovares o conteúdo deste documento, posso aplicar todas estas alterações diretamente no `instructions.md`.