//! Import sketch geometry from DXF and SVG files.
//!
//! These are deliberately small readers: they understand the entity types a
//! sketch actually uses (lines, circles, arcs, polylines and points) and ignore
//! everything else instead of failing the whole file. Text, dimensions, hatches,
//! transforms and 3D entities are skipped: the sketch gets the curves, and the
//! user constrains and dimensions them like anything drawn by hand.
//!
//! SVG coordinates are flipped vertically on the way in: SVG's Y axis points
//! down, a sketch's points up.

use super::*;

impl Sketch {
    /// Import a file, choosing the reader from the extension.
    pub fn import_sketch(&mut self, content: &str, extension: &str) -> Result<usize, String> {
        match extension.to_ascii_lowercase().as_str() {
            "dxf" => self.import_dxf(content),
            "svg" => self.import_svg(content),
            other => Err(format!(
                "Cannot import .{other}: only DXF and SVG sketches are read."
            )),
        }
    }

    /// Add connected lines through the points. Returns the number of segments.
    pub fn add_polyline(&mut self, points: &[[f64; 2]], closed: bool) -> usize {
        if points.len() < 2 {
            return 0;
        }
        let ids: Vec<Uuid> = points
            .iter()
            .map(|point| self.add_point(point[0], point[1]))
            .collect();
        let mut segments = 0;
        for pair in ids.windows(2) {
            self.add_line(pair[0], pair[1]);
            segments += 1;
        }
        if closed && ids.len() > 2 {
            self.add_line(*ids.last().expect("non-empty"), ids[0]);
            segments += 1;
        }
        segments
    }
}

// ===== DXF =====

/// The (group code, value) pairs of a DXF file.
///
/// A DXF is a flat stream of pairs: the code on one line, the value on the next.
/// Entities start with a zero code whose value names them.
fn dxf_pairs(content: &str) -> Vec<(i32, String)> {
    let mut pairs = Vec::new();
    let mut lines = content.lines();
    while let (Some(code), Some(value)) = (lines.next(), lines.next()) {
        if let Ok(code) = code.trim().parse::<i32>() {
            pairs.push((code, value.trim().to_string()));
        }
    }
    pairs
}

impl Sketch {
    /// Read the lines, circles, arcs, polylines and points of a DXF.
    pub fn import_dxf(&mut self, content: &str) -> Result<usize, String> {
        let mut entities: Vec<(String, Vec<(i32, String)>)> = Vec::new();
        for (code, value) in dxf_pairs(content) {
            if code == 0 {
                entities.push((value.trim().to_uppercase(), Vec::new()));
            } else if let Some(current) = entities.last_mut() {
                current.1.push((code, value));
            }
        }
        let number = |codes: &[(i32, String)], wanted: i32| -> Option<f64> {
            codes
                .iter()
                .find(|(code, _)| *code == wanted)
                .and_then(|(_, value)| value.parse().ok())
        };
        let mut added = 0;
        for (kind, codes) in entities {
            match kind.as_str() {
                "LINE" => {
                    if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                        number(&codes, 10),
                        number(&codes, 20),
                        number(&codes, 11),
                        number(&codes, 21),
                    ) {
                        let a = self.add_point(x1, y1);
                        let b = self.add_point(x2, y2);
                        self.add_line(a, b);
                        added += 1;
                    }
                }
                "CIRCLE" => {
                    if let (Some(cx), Some(cy), Some(radius)) =
                        (number(&codes, 10), number(&codes, 20), number(&codes, 40))
                    {
                        if radius > 1e-9 {
                            self.add_circle(cx, cy, radius);
                            added += 1;
                        }
                    }
                }
                "ARC" => {
                    if let (Some(cx), Some(cy), Some(radius), Some(start), Some(end)) = (
                        number(&codes, 10),
                        number(&codes, 20),
                        number(&codes, 40),
                        number(&codes, 50),
                        number(&codes, 51),
                    ) {
                        if radius > 1e-9 {
                            self.add_arc(cx, cy, radius, start.to_radians(), end.to_radians());
                            added += 1;
                        }
                    }
                }
                "POINT" => {
                    if let (Some(x), Some(y)) = (number(&codes, 10), number(&codes, 20)) {
                        self.add_point(x, y);
                        added += 1;
                    }
                }
                // LWPOLYLINE stores its vertices as repeated 10/20 pairs.
                "LWPOLYLINE" | "POLYLINE" => {
                    let mut points = Vec::new();
                    let mut pending_x: Option<f64> = None;
                    for (code, value) in &codes {
                        match code {
                            10 => pending_x = value.parse().ok(),
                            20 => {
                                if let (Some(x), Ok(y)) = (pending_x.take(), value.parse::<f64>()) {
                                    points.push([x, y]);
                                }
                            }
                            _ => {}
                        }
                    }
                    let closed = codes.iter().any(|(code, value)| {
                        *code == 70
                            && value
                                .parse::<i32>()
                                .map(|flags| flags & 1 == 1)
                                .unwrap_or(false)
                    });
                    if points.len() >= 2 {
                        added += self.add_polyline(&points, closed);
                    }
                }
                // Sections, tables and everything else are skipped on purpose.
                _ => {}
            }
        }
        if added == 0 {
            return Err(
                "The DXF has no lines, circles, arcs, polylines or points this reader understands."
                    .to_string(),
            );
        }
        Ok(added)
    }
}

// ===== SVG =====

/// The value of one attribute in an element's text, quoted or not.
fn svg_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=");
    let mut search = tag;
    loop {
        let index = search.find(&pattern)?;
        let whole_name = search[..index]
            .chars()
            .next_back()
            .map(|character| character.is_whitespace())
            .unwrap_or(index == 0);
        if whole_name {
            let after = &search[index + pattern.len()..];
            let quote = after.chars().next()?;
            if quote == '"' || quote == '\'' {
                let rest = &after[1..];
                let end = rest.find(quote)?;
                return Some(&rest[..end]);
            }
            let end = after
                .find(|character: char| character.is_whitespace() || character == '/')
                .unwrap_or(after.len());
            return Some(&after[..end]);
        }
        search = &search[index + pattern.len()..];
    }
}

/// Every number in a string, in order.
fn svg_numbers(text: &str) -> Vec<f64> {
    let mut numbers = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        let part_of_number = character.is_ascii_digit()
            || character == '.'
            || character == '-'
            || character == '+'
            || character == 'e'
            || character == 'E';
        if part_of_number {
            current.push(character);
        } else if !current.is_empty() {
            if let Ok(value) = current.parse::<f64>() {
                numbers.push(value);
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        if let Ok(value) = current.parse::<f64>() {
            numbers.push(value);
        }
    }
    numbers
}

/// The points of a polyline or polygon element.
fn svg_points(tag: &str) -> Option<Vec<[f64; 2]>> {
    let numbers = svg_numbers(svg_attr(tag, "points")?);
    let mut points = Vec::new();
    for pair in numbers.chunks_exact(2) {
        points.push([pair[0], -pair[1]]);
    }
    (points.len() >= 2).then_some(points)
}

/// One token of an SVG path's d attribute.
#[derive(Debug, Clone, Copy)]
enum SvgToken {
    Number(f64),
    Command(char),
}

fn svg_tokens(data: &str) -> Vec<SvgToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in data.chars() {
        if character.is_ascii_alphabetic() {
            if !current.is_empty() {
                if let Ok(value) = current.parse::<f64>() {
                    tokens.push(SvgToken::Number(value));
                }
                current.clear();
            }
            tokens.push(SvgToken::Command(character));
        } else if character.is_ascii_digit()
            || character == '.'
            || character == '-'
            || character == '+'
            || character == 'e'
            || character == 'E'
        {
            current.push(character);
        } else if !current.is_empty() {
            if let Ok(value) = current.parse::<f64>() {
                tokens.push(SvgToken::Number(value));
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        if let Ok(value) = current.parse::<f64>() {
            tokens.push(SvgToken::Number(value));
        }
    }
    tokens
}

/// Read one number from a path's token stream.
fn next_number(tokens: &[SvgToken], index: &mut usize) -> Option<f64> {
    match tokens.get(*index) {
        Some(SvgToken::Number(value)) => {
            *index += 1;
            Some(*value)
        }
        _ => None,
    }
}

/// Points along a cubic Bezier, excluding the start.
fn cubic_samples(
    from: [f64; 2],
    control_a: [f64; 2],
    control_b: [f64; 2],
    to: [f64; 2],
    steps: usize,
) -> Vec<[f64; 2]> {
    (1..=steps)
        .map(|step| {
            let t = step as f64 / steps as f64;
            let u = 1.0 - t;
            [
                u * u * u * from[0]
                    + 3.0 * u * u * t * control_a[0]
                    + 3.0 * u * t * t * control_b[0]
                    + t * t * t * to[0],
                u * u * u * from[1]
                    + 3.0 * u * u * t * control_a[1]
                    + 3.0 * u * t * t * control_b[1]
                    + t * t * t * to[1],
            ]
        })
        .collect()
}

/// Points along a quadratic Bezier, excluding the start.
fn quadratic_samples(
    from: [f64; 2],
    control: [f64; 2],
    to: [f64; 2],
    steps: usize,
) -> Vec<[f64; 2]> {
    (1..=steps)
        .map(|step| {
            let t = step as f64 / steps as f64;
            let u = 1.0 - t;
            [
                u * u * from[0] + 2.0 * u * t * control[0] + t * t * to[0],
                u * u * from[1] + 2.0 * u * t * control[1] + t * t * to[1],
            ]
        })
        .collect()
}

impl Sketch {
    /// Read the lines, circles, rectangles, polylines and paths of an SVG.
    pub fn import_svg(&mut self, content: &str) -> Result<usize, String> {
        // Comments can hide anything, including a closing angle bracket that
        // would end an element early, so drop them before scanning.
        let mut source = String::with_capacity(content.len());
        let mut rest = content;
        while let Some(start) = rest.find("<!--") {
            source.push_str(&rest[..start]);
            match rest[start..].find("-->") {
                Some(end) => rest = &rest[start + end + 3..],
                None => rest = "",
            }
        }
        source.push_str(rest);

        let flip = |y: f64| -y;
        let mut added = 0;
        let mut position = 0usize;
        while let Some(start) = source[position..].find('<') {
            let open = position + start;
            let Some(end) = source[open..].find('>') else {
                break;
            };
            let end = open + end;
            let tag = &source[open + 1..end];
            position = end + 1;
            if tag.starts_with('/') || tag.starts_with('?') || tag.starts_with('!') {
                continue;
            }
            let name_end = tag
                .find(|character: char| character.is_whitespace() || character == '/')
                .unwrap_or(tag.len());
            let name = tag[..name_end].to_ascii_lowercase();
            let body = &tag[name_end..];
            let attr = |wanted: &str| svg_attr(body, wanted).and_then(|value| value.parse().ok());
            match name.as_str() {
                "line" => {
                    if let (Some(x1), Some(y1), Some(x2), Some(y2)) =
                        (attr("x1"), attr("y1"), attr("x2"), attr("y2"))
                    {
                        let a = self.add_point(x1, flip(y1));
                        let b = self.add_point(x2, flip(y2));
                        self.add_line(a, b);
                        added += 1;
                    }
                }
                "circle" => {
                    if let (Some(cx), Some(cy), Some(radius)) = (attr("cx"), attr("cy"), attr("r"))
                    {
                        if radius > 1e-9 {
                            self.add_circle(cx, flip(cy), radius);
                            added += 1;
                        }
                    }
                }
                "ellipse" => {
                    if let (Some(cx), Some(cy), Some(rx), Some(ry)) =
                        (attr("cx"), attr("cy"), attr("rx"), attr("ry"))
                    {
                        if rx > 1e-9 && ry > 1e-9 {
                            if (rx - ry).abs() < 1e-9 {
                                self.add_circle(cx, flip(cy), rx);
                                added += 1;
                            } else {
                                // An ellipse has no sketch entity yet: its axes
                                // make the box it fits in.
                                let points = [
                                    [cx - rx, flip(cy - ry)],
                                    [cx + rx, flip(cy - ry)],
                                    [cx + rx, flip(cy + ry)],
                                    [cx - rx, flip(cy + ry)],
                                ];
                                added += self.add_polyline(&points, true);
                            }
                        }
                    }
                }
                "rect" => {
                    let x = attr("x").unwrap_or(0.0);
                    let y = attr("y").unwrap_or(0.0);
                    let width = attr("width").unwrap_or(0.0);
                    let height = attr("height").unwrap_or(0.0);
                    if width > 1e-9 && height > 1e-9 {
                        let points = [
                            [x, flip(y)],
                            [x + width, flip(y)],
                            [x + width, flip(y + height)],
                            [x, flip(y + height)],
                        ];
                        added += self.add_polyline(&points, true);
                    }
                }
                "polyline" | "polygon" => {
                    if let Some(points) = svg_points(body) {
                        added += self.add_polyline(&points, name == "polygon");
                    }
                }
                "path" => {
                    if let Some(data) = svg_attr(body, "d") {
                        added += self.add_svg_path(data);
                    }
                }
                _ => {}
            }
        }
        if added == 0 {
            return Err(
                "The SVG has no lines, circles, rectangles, polylines or paths this reader understands."
                    .to_string(),
            );
        }
        Ok(added)
    }

    /// Add the segments of an SVG path's d attribute. Returns the segments.
    ///
    /// Cubic and quadratic curves are sampled into short segments; elliptical
    /// arcs (A/a) are taken as straight lines to their end point.
    pub fn add_svg_path(&mut self, data: &str) -> usize {
        let tokens = svg_tokens(data);
        let mut index = 0usize;
        let mut command: Option<char> = None;
        let mut cursor = [0.0, 0.0];
        let mut subpath_start = [0.0, 0.0];
        let mut points: Vec<[f64; 2]> = Vec::new();
        let mut subpaths: Vec<(Vec<[f64; 2]>, bool)> = Vec::new();

        let flush = |points: &mut Vec<[f64; 2]>,
                     subpaths: &mut Vec<(Vec<[f64; 2]>, bool)>,
                     closed: bool| {
            if points.len() >= 2 {
                subpaths.push((std::mem::take(points), closed));
            } else {
                points.clear();
            }
        };

        while index < tokens.len() {
            if let SvgToken::Command(letter) = tokens[index] {
                command = Some(letter);
                index += 1;
                if letter == 'Z' || letter == 'z' {
                    flush(&mut points, &mut subpaths, true);
                    cursor = subpath_start;
                }
                continue;
            }
            let Some(letter) = command else {
                index += 1;
                continue;
            };
            let relative = letter.is_ascii_lowercase();
            let base = if relative { cursor } else { [0.0, 0.0] };
            match letter.to_ascii_uppercase() {
                'M' => {
                    let (Some(x), Some(y)) = (
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                    ) else {
                        break;
                    };
                    let target = [base[0] + x, base[1] + y];
                    flush(&mut points, &mut subpaths, false);
                    cursor = target;
                    subpath_start = target;
                    points.push(target);
                    // Further pairs after a move are line-to commands.
                    command = Some(if relative { 'l' } else { 'L' });
                }
                'L' => {
                    let (Some(x), Some(y)) = (
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                    ) else {
                        break;
                    };
                    cursor = [base[0] + x, base[1] + y];
                    points.push(cursor);
                }
                'H' => {
                    let Some(x) = next_number(&tokens, &mut index) else {
                        break;
                    };
                    cursor = [base[0] + x, cursor[1]];
                    points.push(cursor);
                }
                'V' => {
                    let Some(y) = next_number(&tokens, &mut index) else {
                        break;
                    };
                    cursor = [cursor[0], base[1] + y];
                    points.push(cursor);
                }
                'C' => {
                    let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                    ) else {
                        break;
                    };
                    let target = [base[0] + x, base[1] + y];
                    let control_a = [base[0] + x1, base[1] + y1];
                    let control_b = [base[0] + x2, base[1] + y2];
                    points.extend(cubic_samples(cursor, control_a, control_b, target, 16));
                    cursor = target;
                }
                'S' => {
                    // Smooth cubic: the first control is the current point.
                    let (Some(x2), Some(y2), Some(x), Some(y)) = (
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                    ) else {
                        break;
                    };
                    let target = [base[0] + x, base[1] + y];
                    let control_b = [base[0] + x2, base[1] + y2];
                    points.extend(cubic_samples(cursor, cursor, control_b, target, 16));
                    cursor = target;
                }
                'Q' => {
                    let (Some(x1), Some(y1), Some(x), Some(y)) = (
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                    ) else {
                        break;
                    };
                    let target = [base[0] + x, base[1] + y];
                    let control = [base[0] + x1, base[1] + y1];
                    points.extend(quadratic_samples(cursor, control, target, 12));
                    cursor = target;
                }
                'T' => {
                    let (Some(x), Some(y)) = (
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                    ) else {
                        break;
                    };
                    let target = [base[0] + x, base[1] + y];
                    points.extend(quadratic_samples(cursor, cursor, target, 12));
                    cursor = target;
                }
                'A' => {
                    // rx ry rotation large-arc sweep x y: taken as a line.
                    let (
                        Some(_rx),
                        Some(_ry),
                        Some(_rotation),
                        Some(_large),
                        Some(_sweep),
                        Some(x),
                        Some(y),
                    ) = (
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                        next_number(&tokens, &mut index),
                    )
                    else {
                        break;
                    };
                    cursor = [base[0] + x, base[1] + y];
                    points.push(cursor);
                }
                _ => {
                    // An unknown command: stop rather than spin on its numbers.
                    break;
                }
            }
        }
        flush(&mut points, &mut subpaths, false);

        let mut segments = 0;
        for (mut points, closed) in subpaths {
            // The whole path was flipped vertically on the way in, like every
            // other SVG coordinate.
            for point in &mut points {
                point[1] = -point[1];
            }
            segments += self.add_polyline(&points, closed);
        }
        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DXF: &str = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n20.0\n21\n0.0\n0\nCIRCLE\n10\n10.0\n20\n10.0\n40\n5.0\n0\nARC\n10\n0.0\n20\n0.0\n40\n10.0\n50\n0.0\n51\n90.0\n0\nLWPOLYLINE\n90\n3\n70\n1\n10\n0.0\n20\n0.0\n10\n5.0\n20\n0.0\n10\n5.0\n20\n5.0\n0\nENDSEC\n0\nEOF\n";

    #[test]
    fn a_dxf_imports_its_entities() {
        let mut sketch = Sketch::new("import");
        let added = sketch.import_dxf(DXF).expect("a readable dxf");
        assert!(added >= 6, "{added}");
        assert_eq!(sketch.lines.len(), 1 + 3);
        assert_eq!(sketch.circles.len(), 1);
        assert_eq!(sketch.arcs.len(), 1);
        assert!((sketch.circles[0].radius - 5.0).abs() < 1e-9);
    }

    #[test]
    fn an_empty_dxf_explains_itself() {
        let mut sketch = Sketch::new("import");
        let error = sketch.import_dxf("0\nSECTION\n2\nENTITIES\n").unwrap_err();
        assert!(error.contains("no lines"), "{error}");
    }

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg">
        <!-- a comment -->
        <line x1="0" y1="0" x2="20" y2="0"/>
        <circle cx="10" cy="10" r="5"/>
        <rect x="0" y="0" width="4" height="2"/>
        <polyline points="0,0 5,0 5,5"/>
        <polygon points="0,0 5,0 5,5"/>
        <path d="M 0 0 L 10 0 L 10 10 Z"/>
        <path d="M 0 0 C 5 5 10 5 15 0"/>
        <path d="M 0 0 Q 5 5 10 0"/>
      </svg>"#;

    #[test]
    fn an_svg_imports_its_entities() {
        let mut sketch = Sketch::new("import");
        let added = sketch.import_svg(SVG).expect("a readable svg");
        assert!(added >= 10, "{added}");
        assert_eq!(sketch.circles.len(), 1);
        // The line, the rect, two polylines, a closed path, a cubic and a quad.
        assert!(sketch.lines.len() >= 10, "{}", sketch.lines.len());
        // SVG Y grows down: the point under the origin is above it here.
        assert!(sketch.points.iter().any(|point| point.y < -0.5));
    }

    #[test]
    fn an_svg_path_with_relative_and_axis_commands() {
        let mut sketch = Sketch::new("import");
        let segments = sketch.add_svg_path("m 0 0 h 10 v 10 h -10 z");
        assert_eq!(segments, 4);
        assert_eq!(sketch.points.len(), 4);
    }

    #[test]
    fn a_file_with_no_geometry_is_refused() {
        let mut sketch = Sketch::new("import");
        let error = sketch
            .import_svg("<svg><text>hello</text></svg>")
            .unwrap_err();
        assert!(error.contains("no lines"), "{error}");
        let error = sketch.import_sketch("x", "step").unwrap_err();
        assert!(error.contains("only DXF and SVG"), "{error}");
    }

    #[test]
    fn an_attribute_is_matched_by_its_whole_name() {
        let tag = r#" x="1" id="2" width="3""#;
        assert_eq!(svg_attr(tag, "id"), Some("2"));
        assert_eq!(svg_attr(tag, "d"), None);
        assert_eq!(svg_attr(tag, "width"), Some("3"));
    }
}
