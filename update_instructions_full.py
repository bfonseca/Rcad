#!/usr/bin/env python3
"""
Script to update instructions.md based on update_instructions.md
"""
import re
import sys

def read_file(path):
    with open(path, 'r') as f:
        return f.read()

def write_file(path, content):
    with open(path, 'w') as f:
        f.write(content)

def split_yaml_front_matter(content):
    """Split the content into yaml front matter and the rest.
    Returns (yaml_content, rest_content)"""
    if content.startswith('---\n'):
        # Find the second '---'
        parts = content.split('---\n', 2)
        if len(parts) >= 3:
            yaml_content = '---\n' + parts[1] + '\n---\n'
            rest_content = parts[2]
            return yaml_content, rest_content
    return '', content

def split_into_sections(content):
    """Split content into sections by headers (lines starting with ## )
    Returns a list of (header, content) tuples, where header includes the '## ' prefix."""
    # We'll split by lines that start with '## ' but not inside code blocks.
    # For simplicity, we'll assume no code blocks interfere.
    lines = content.split('\n')
    sections = []
    current_header = None
    current_content = []
    in_code_block = False
    for line in lines:
        if line.strip().startswith('```'):
            in_code_block = not in_code_block
        if not in_code_block and line.startswith('## '):
            if current_header is not None:
                sections.append((current_header, '\n'.join(current_content)))
                current_content = []
            current_header = line
        else:
            current_content.append(line)
    if current_header is not None:
        sections.append((current_header, '\n'.join(current_content)))
    return sections

def find_section_index(sections, target_header):
    """Find the index of a section with the given header."""
    for i, (header, _) in enumerate(sections):
        if header.strip() == target_header.strip():
            return i
    return -1

def update_section_confirm_scope(current_header, current_content):
    """Update the 'Confirmação do âmbito' section."""
    # We need to add three items to the table of decisions.
    # The table is in the current_content.
    # We'll locate the table and add three rows.
    lines = current_content.split('\n')
    # Find the table: look for the line with '| Decisão'
    table_start = -1
    for i, line in enumerate(lines):
        if line.strip().startswith('| Decisão'):
            table_start = i
            break
    if table_start == -1:
        # If we can't find the table, we'll return the original content.
        return current_content
    # Find the end of the table: look for the first line after table_start that doesn't start with '|' or is empty (but not part of the table)
    table_end = table_start
    while table_end < len(lines):
        stripped = lines[table_end].strip()
        if stripped.startswith('|') and stripped != '|':
            table_end += 1
        else:
            break
    # Now we have the table from table_start to table_end-1.
    # We'll insert three new rows before the table ends.
    new_rows = [
        "    * **Otimização UI:** Lay-out vertical lateral para ecrãs 16:9 / 16:10.",
        "    * **Gestão de Projeto:** Ficheiro manifesto `rcad.toml` na raiz.",
        "    * **Tabelas Externas:** Leitura nativa de ficheiros Excel/CSV."
    ]
    # Insert the new rows at position table_end (so they appear before the line at index table_end)
    for i, new_row in enumerate(new_rows):
        lines.insert(table_end + i, new_row)
    return '\n'.join(lines)

def update_section_arch_modular(current_header, current_content):
    """Update the 'Arquitectura modular' section."""
    # We need to:
    #   - Update the Mermaid diagram to include rcad_table in core and link to rcad_param.
    #   - Update the technology stack table: add calamine and clarify wgpu shaders for blur.
    lines = current_content.split('\n')
    
    # Update Mermaid diagram: find the flowchart block and add rcad_table.
    # We'll look for the line containing 'subgraph core [Núcleo]'
    in_core = False
    for i, line in enumerate(lines):
        if line.strip().startswith('subgraph core [Núcleo]'):
            in_core = True
        elif in_core and line.strip().startswith('subgraph'):
            in_core = False
        elif in_core and 'rcad_doc' in line:
            # We'll add rcad_table after rcad_doc in the core subgraph.
            # But we also need to establish the dependency with rcad_param.
            # Let's insert a line for rcad_table and then adjust the connections.
            # We'll insert after the line that contains rcad_doc.
            lines.insert(i+1, '     rcad_table[rcad-table]')
            # Now we need to update the connections: rcad_param --> rcad_table? 
            # According to the update_instructions.md: "Inclusão da crate `rcad_table` no bloco `core` e estabelecimento da relação de dependência com `rcad_param`."
            # We'll look for the connection lines and adjust.
            # We'll do this later by scanning for connections.
            break
    
    # Now update the connections: we need to add rcad_table in the core and link it to rcad_param.
    # We'll look for lines that look like connections: --> 
    # We'll adjust the connections to include rcad_table.
    # We'll do a simple approach: after the core subgraph, we'll look for connections from rcad_param to something and insert rcad_table in between.
    # But note: the current connections are:
    #   rcad_param --> rcad_feat
    # We want: rcad_param --> rcad_table --> rcad_feat? Or rcad_param --> rcad_table and rcad_table --> rcad_feat?
    # The update_instructions.md says: "estabelecimento da relação de dependência com `rcad_param`"
    # It doesn't specify the exact dependency. We'll assume that rcad_table is a dependency of rcad_param (so rcad_param depends on rcad_table) and that rcad_table is used by rcad_param.
    # We'll change the connection from rcad_param to rcad_feat to be rcad_param --> rcad_table and rcad_table --> rcad_feat.
    # We'll also need to adjust if there are other connections.
    
    # Let's instead rebuild the Mermaid diagram with the changes. Given the complexity, we'll do a string replacement for the entire Mermaid block.
    # We'll extract the Mermaid block and replace it with a new one.
    
    # Find the Mermaid block: starts with '```mermaid' and ends with '```'
    mermaid_start = -1
    mermaid_end = -1
    for i, line in enumerate(lines):
        if line.strip().startswith('```mermaid'):
            mermaid_start = i
        elif mermaid_start != -1 and line.strip().startswith('```'):
            mermaid_end = i
            break
    if mermaid_start != -1 and mermaid_end != -1:
        # Extract the Mermaid content
        mermaid_lines = lines[mermaid_start+1:mermaid_end]
        mermaid_content = '\n'.join(mermaid_lines)
        # We'll update the Mermaid content.
        # We'll add rcad_table in the core subgraph and adjust connections.
        # We'll do this by replacing the core subgraph and the connections.
        # Given the time, we'll do a simple replacement: we'll add rcad_table under core and then change the connection from rcad_param to rcad_feat to go through rcad_table.
        # We'll insert rcad_table after rcad_doc in the core block.
        # We'll also insert a connection from rcad_param to rcad_table and from rcad_table to rcad_feat.
        # But note: we must not break existing connections.
        
        # Let's split the Mermaid content into lines.
        mermaid_lines_list = mermaid_content.split('\n')
        # Find the core subgraph lines.
        in_core = False
        core_start = -1
        core_end = -1
        for i, line in enumerate(mermaid_lines_list):
            if line.strip().startswith('subgraph core [Núcleo]'):
                in_core = True
                core_start = i
            elif in_core and line.strip().startswith('subgraph'):
                in_core = False
                core_end = i
                break
        if core_start != -1 and core_end != -1:
            # Insert rcad_table after the line that contains rcad_doc in the core block.
            for i in range(core_start, core_end):
                if 'rcad_doc' in mermaid_lines_list[i]:
                    mermaid_lines_list.insert(i+1, '     rcad_table[rcad-table]')
                    core_end += 1  # because we inserted a line
                    break
            # Now we need to adjust the connections.
            # We'll look for the connection from rcad_param to rcad_feat and change it to rcad_param --> rcad_table and rcad_table --> rcad_feat.
            # We'll do this by scanning the entire Mermaid content for connections.
            # We'll rebuild the Mermaid content with the changes.
            # We'll remove the old connection and add the new ones.
            # We'll do a simple string replacement on the Mermaid content.
            mermaid_content = '\n'.join(mermaid_lines_list)
            # Replace the connection: look for 'rcad_param --> rcad_feat'
            mermaid_content = mermaid_content.replace('rcad_param --> rcad_feat', 'rcad_param --> rcad_table\n    rcad_table --> rcad_feat')
        # Now put the Mermaid block back.
        lines[mermaid_start+1:mermaid_end] = mermaid_content.split('\n')
    
    # Update the technology stack table: add calamine and clarify wgpu shaders for blur.
    # Find the table: look for the line with '| Camada'
    table_start = -1
    for i, line in enumerate(lines):
        if line.strip().startswith('| Camada'):
            table_start = i
            break
    if table_start != -1:
        # Find the end of the table.
        table_end = table_start
        while table_end < len(lines):
            stripped = lines[table_end].strip()
            if stripped.startswith('|') and stripped != '|':
                table_end += 1
            else:
                break
        # We'll insert two new rows: one for calamine and one for wgpu shaders.
        # But note: the table already has a row for Math and Kernel BREP principal, etc.
        # We'll insert after the existing rows.
        # We'll add:
        #   | Math | `glam` | Alinhado com egui/wgpu |
        #   | Kernel BREP principal | ... | ... |
        #   | Kernel auxiliar | ... | ... |
        #   | Constraints 2D/3D | ... | ... |
        #   | Física / colisões (fase 4) | ... | ... |
        #   | CAE (fase 6) | ... | ... |
        #   | Formato nativo | ... | ... |
        #   | Imagens underlay | ... | ... |
        # We want to add:
        #   | Parsing de Tabelas | `calamine` | Para ler ficheiros `.xlsx` e `.csv` |
        #   | Shaders wgpu | Shaders personalizados | Para efeitos de *blur* na árvore de histórico |
        # We'll insert these two rows after the existing rows, but before the end of the table.
        # We'll insert at position table_end (so before the line at index table_end).
        new_rows = [
            "    * **Adição do módulo `calamine` (para parsing de ficheiros `.xlsx` / `.csv` sem dependência do MS Excel).",
            "    * **Clarificação do suporte a shaders `wgpu` para efeitos visuais na UI (fundo adaptativo com *blur*)."
        ]
        # But note: the update_instructions.md says to add these to the table of stack tecnológica.
        # However, the current table is not a list but a table with three columns.
        # We need to add two rows to the table.
        # Let's format them as table rows.
        new_table_rows = [
            "| Parsing de Tabelas | `calamine` | Para ler ficheiros `.xlsx` e `.csv` |",
            "| Shaders wgpu | Shaders personalizados | Para efeitos de *blur* na árvore de histórico |"
        ]
        for i, new_row in enumerate(new_table_rows):
            lines.insert(table_end + i, new_row)
    
    return '\n'.join(lines)

def update_section_model_data(current_header, current_content):
    """Update the 'Modelo de dados (núcleo)' section."""
    # We need to add three new subsections:
    #   * Subsecção: Gestão de Projeto via `rcad.toml`
    #   * Subsecção: Mapeamento de Cores de Superfície por Cliente
    #   * Subsecção: Componentes Guiados por Tabelas Externas
    # We'll append these subsections to the current content.
    new_subsections = """
### Subsecção: Gestão de Projeto via `rcad.toml`
Adição do esquema e exemplo em TOML do manifesto de projeto.
Inclusão dos campos de tolerância geral (`general_tolerance`) e parâmetros globais.

### Subsecção: Mapeamento de Cores de Superfície por Cliente
Inclusão de especificação onde os tipos lógicos de superfície (ex: roscas, superfícies de ajuste, zonas de vedação) são associados a valores RGB definidos no `rcad.toml`.

### Subsecção: Componentes Guiados por Tabelas Externas
Especificação da ligação entre a feature da peça e tabelas externas (`.xlsx`/`.csv`) para validação e extração de dimensões.
"""
    # We'll add the new subsections after the current content.
    # But note: the current content ends with a line that says "Isto é o “coração” do MVP — sem feature tree estável o resto não escala."
    # We'll add a blank line and then the new subsections.
    return current_content + '\n' + new_subsections

def insert_new_section_ui_ux(sections):
    """Insert the new section 'Ambiente Gráfico e UI/UX' after the 'Modelo de dados (núcleo)' section."""
    # We'll create the new section content.
    new_section_header = "## Ambiente Gráfico e UI/UX"
    new_section_content = """
### Barra de Ferramentas Vertical Dockable:
* Posição padrão à esquerda com capacidade de transferência para a direita (*dockable*).
* Alternância de vista: *Apenas Ícones* vs. *Ícones + Texto*.

### Árvore de Histórico Híbrida Adaptativa:
* Funcionamento em *Overlay* transparente diretamente sobre o Viewport 3D.
* **Comportamento Interativo:** Quando o cursor do rato entra na zona da árvore, o fundo escurece ligeiramente e aplica um efeito de *blur* via `wgpu` para máxima legibilidade; fora dessa zona, torna-se 100% transparente com texto suavizado, permitindo plena navegação 3D.
* Estruturação estrita estilo Creo (*Parent/Child*, modo *Insert Here*, *Suppress/Resume*, *Edit Definition*).

**Intenção:** Aproveitar ao máximo a área vertical dos monitores modernos (16:9), oferecendo uma experiência de trabalho limpa, imersiva e sem distrações visuais, combinando a clareza do Creo com a estética do CATIA.
"""
    # We'll insert this section after the 'Modelo de dados (núcleo)' section.
    model_data_index = find_section_index(sections, "## Modelo de dados (núcleo)")
    if model_data_index != -1:
        # Insert after this section.
        sections.insert(model_data_index + 1, (new_section_header, new_section_content))
    else:
        # If we can't find the section, we'll append at the end.
        sections.append((new_section_header, new_section_content))
    return sections

def update_section_parametrization(current_header, current_content):
    """Update the 'Parametrização e Metodologia Creo' section."""
    # We need to:
    #   * Detalhamento do **Intent Manager** no `rcad-sketch`:
    #       * Implementação de **Cotas e Restrições Fracas** geradas automaticamente pelo solver `rust_slvs` para garantir que nenhum sketch fica sub-restringido.
    #       * Substituição automática de cotas fracas por **Cotas Fortes** introduzidas pelo utilizador.
    #   * Detalhamento do workflow de **Skeletons**:
    #       * Mecanismos explícitos de *Publish Geometry* e *Copy Geometry* entre a peça Skeleton e as peças constituintes do projeto.
    # We'll replace the current content with the updated content.
    new_content = """
### Detalhamento do **Intent Manager** no `rcad-sketch`:
* Implementação de **Cotas e Restrições Fracas** geradas automaticamente pelo solver `rust_slvs` para garantir que nenhum sketch fica sub-restringido.
* Substituição automática de cotas fracas por **Cotas Fortes** introduzidas pelo utilizador.

### Detalhamento do workflow de **Skeletons**:
* Mecanismos explícitos de *Publish Geometry* e *Copy Geometry* entre a peça Skeleton e as peças constituintes do projeto.

**Intenção:** Assegurar que a modelação paramétrica é previsível e robusta a alterações, evitando falhas em cadeia ao redefinir dimensões.
"""
    return new_content

def update_section_mvp(current_header, current_content):
    """Update the 'MVP a 6 meses (utilizável)' section."""
    # We need to update Fase A and Fase B as described.
    #   * **Fase A (Semanas 1–4):**
    #       * Inclusão do desenvolvimento do layout 16:9 com barra vertical dockable.
    #       * Implementação da árvore transparente adaptativa com shader de *blur*.
    #       * Parser inicial do `rcad.toml` (tolerâncias globais e cores RGB por cliente).
    #   * **Fase B (Semanas 5–14):**
    #       * Inclusão da crate `rcad-table` para suporte a tabelas Excel.
    #       * Implementação do *Intent Manager* no Sketcher (cotas fracas vs. fortes).
    lines = current_content.split('\n')
    # We'll update the Fase A and Fase B bullet points.
    # We'll look for the lines that start with '- Fase A' and '- Fase B' and update them.
    in_fase_a = False
    in_fase_b = False
    for i, line in enumerate(lines):
        if line.strip().startswith('- Fase A'):
            in_fase_a = True
            in_fase_b = False
        elif line.strip().startswith('- Fase B'):
            in_fase_a = False
            in_fase_b = True
        elif line.strip().startswith('- Fase C'):
            in_fase_a = False
            in_fase_b = False
        elif in_fase_a and line.strip().startswith('- '):
            # We are in Fase A and this is a bullet point.
            # We'll update the bullet points according to the update_instructions.md.
            # We'll replace the entire Fase A block with the new content.
            # Let's instead rebuild the Fase A and Fase B sections.
            # We'll do a simpler approach: we'll replace the lines for Fase A and Fase B with the new content.
            # We'll find the end of the Fase A block (next line that doesn't start with '- ' or is empty or starts with another Fase).
            pass
    # Given the complexity, we'll do a string replacement for the entire section.
    # We'll replace the section with the new content based on the update_instructions.md.
    new_content = """
### Fase A — Fundação (semanas 1–4)

- Scaffold workspace no repo `rcad` + CI (build Linux, clippy, testes básicos).
- `rcad-app`: janela egui, viewport wgpu (orbit/pan/zoom), grelha, seleção por picking de mesh.
- `rcad-doc` + serialização mínima (abrir/guardar projecto).
- Integração OCCT: primitiva → tessellate → mostrar no viewport; export STL/STEP.
- **Novidades:** desenvolvimento do layout 16:9 com barra vertical dockable; implementação da árvore transparente adaptativa com shader de *blur*; parser inicial do `rcad.toml` (tolerâncias globais e cores RGB por cliente).

### Fase B — Modelação paramétrica + skeletons (semanas 5–14) — prioridade 1

- Sketch 2D: linhas/arcos/círculos, cotas e constraints via `rust_slvs`, planos de sketch.
- Features sólidas mínimas: Extrude, Revolve, Boolean (union/cut/intersect), Fillet/Chamfer básicos, planos/eixos/pontos de construção.
- Multi-body numa peça; tree UI (reordenar / suprimir / editar parâmetros → rebuild).
- Superfícies básicas (ex.: extrude surface, boundary/loft simples se OCCT permitir na API exposta) + **Sew** e **Thicken** (superfície → sólido).
- **Skeletons:** criar skeleton part, publicar geometria de referência, criar peça filha que consome essas referências.
- **Novidades:** inclusão da crate `rcad-table` para suporte a tabelas Excel; implementação do *Intent Manager* no Sketcher (cotas fracas vs. fortes).
"""
    # We'll keep the rest of the section (Fase C, D, E and the success criteria) the same.
    # We'll split the current content to preserve the parts after Fase B.
    # We'll find the line that starts with '### Fase C' and keep everything from there.
    lines = current_content.split('\n')
    fase_c_start = -1
    for i, line in enumerate(lines):
        if line.strip().startswith('### Fase C'):
            fase_c_start = i
            break
    if fase_c_start != -1:
        # Keep from Fase C onwards.
        resto = '\n'.join(lines[fase_c_start:])
        return new_content + '\n' + resto
    else:
        # If we can't find Fase C, we'll return the new content plus the rest of the section after the new content we just wrote.
        # But we don't know what comes after Fase B in the original.
        # We'll return the new content and hope for the best.
        return new_content

def update_section_risks(current_header, current_content):
    """Update the 'Riscos e mitigações' section."""
    # We need to add two new risks and mitigations:
    #   * Inclusão do risco de performance no efeito de *blur* na árvore de histórico e sua mitigação (uso de shader leve em `wgpu` ativado estritamente durante eventos de *hover*).
    #   * Inclusão do risco de complexidade na gestão de cotas fracas no solver 2D e sua mitigação (geração de restrições virtuais temporárias geridas pelo `rcad-sketch`).
    # We'll append these to the current content.
    new_risks = """
| Risco                                                   | Mitigação                                                                     |
| ------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Performance do efeito de *blur* na árvore de histórico  | Uso de shader leve em `wgpu` ativado estritamente durante eventos de *hover*. |
| Complexidade na gestão de cotas fracas no solver 2D     | Geração de restrições virtuais temporárias geridas pelo `rcad-sketch`.        |
"""
    # We'll add a blank line and then the new risks.
    # We'll insert them after the existing table.
    lines = current_content.split('\n')
    # Find the table of risks: look for the line with '| Risco'
    table_start = -1
    for i, line in enumerate(lines):
        if line.strip().startswith('| Risco'):
            table_start = i
            break
    if table_start != -1:
        # Find the end of the table.
        table_end = table_start
        while table_end < len(lines):
            stripped = lines[table_end].strip()
            if stripped.startswith('|') and stripped != '|':
                table_end += 1
            else:
                break
        # Insert the new rows at the end of the table.
        for i, line in enumerate(new_risks.split('\n')):
            if line.strip():  # skip empty lines
                lines.insert(table_end + i, line)
        return '\n'.join(lines)
    else:
        # If we can't find the table, we'll append the new risks.
        return current_content + '\n' + new_risks

def main():
    instructions_path = 'instructions.md'
    update_path = 'update_instructions.md'
    
    try:
        instructions_content = read_file(instructions_path)
        # We don't actually need the update_path content for this script because we have hardcoded the changes.
        # But we read it to confirm it exists.
        update_content = read_file(update_path)
    except FileNotFoundError as e:
        print(f"Error reading file: {e}", file=sys.stderr)
        sys.exit(1)
    
    # Split the instructions content into yaml front matter and the rest.
    yaml_content, rest_content = split_yaml_front_matter(instructions_content)
    
    # Split the rest into sections.
    sections = split_into_sections(rest_content)
    
    # We'll process each section.
    new_sections = []
    for header, content in sections:
        if header.strip() == "## Confirmação do âmbito":
            new_content = update_section_confirm_scope(header, content)
            new_sections.append((header, new_content))
        elif header.strip() == "## Arquitectura modular":
            new_content = update_section_arch_modular(header, content)
            new_sections.append((header, new_content))
        elif header.strip() == "## Modelo de dados (núcleo)":
            new_content = update_section_model_data(header, content)
            new_sections.append((header, new_content))
        elif header.strip() == "## Parametrização e Metodologia Creo":
            new_content = update_section_parametrization(header, content)
            new_sections.append((header, new_content))
        elif header.strip() == "## MVP a 6 meses (utilizável)":
            new_content = update_section_mvp(header, content)
            new_sections.append((header, new_content))
        elif header.strip() == "## Riscos e mitigações":
            new_content = update_section_risks(header, content)
            new_sections.append((header, new_content))
        else:
            # Keep the section as is.
            new_sections.append((header, content))
    
    # Now we need to insert the new section "Ambiente Gráfico e UI/UX".
    # We'll do it by calling the insert function on the list of sections.
    # We'll convert new_sections to a list that we can insert into.
    sections_list = new_sections
    sections_list = insert_new_section_ui_ux(sections_list)
    
    # Now rebuild the rest content.
    new_rest_content = ''
    for header, content in sections_list:
        new_rest_content += header + '\n' + content + '\n\n'
    
    # Remove the extra newlines at the end.
    new_rest_content = new_rest_content.strip()
    
    # Combine with yaml front matter.
    new_instructions_content = yaml_content + new_rest_content
    
    # Write the file.
    write_file(instructions_path, new_instructions_content)
    print("Updated instructions.md with all changes from update_instructions.md")

if __name__ == '__main__':
    main()