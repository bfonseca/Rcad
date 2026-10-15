# rcad icon set

197 original icons + logo for **rcad**, a parametric CAD application.

Categories: Sketch 28, Sketch constraints 12, Solid features 27, Surfaces 12, Curves 7, Datum 10, Assembly 15, Edit & patterns 17, View & display 31, General 38.

## Contents
- `svg/light/<category>/<name>.svg` and `svg/dark/...` - vector icons, 32x32 viewBox (use on white / on dark UI)
- `png/32/`, `png/64/` - raster exports of every icon, per theme
- `sprite-light.svg`, `sprite-dark.svg` - all icons as `<symbol id="<category>-<name>">` for inline use
- `icons.json` - id / category / label for every icon (for building menus and toolbars)
- `palette.json` - colour tokens for both themes
- `logo/` - app icon, mark, horizontal and stacked lockups (light/dark), wordmark, PNGs (16-1024), `rcad.ico`, `favicon.ico`
- `index.html` - open in a browser to browse everything; `preview-light.png` / `preview-dark.png` - contact sheets
- `source/` - the Python generator (edit and rebuild)

## Visual language
- 32 px grid, ~1.6 px outlines, isometric 3D for solid features (light from top-left, three face shades)
- Orange = the thing being created or the active direction/profile; teal = datum; violet = surfaces; red = material removal / delete
- Every icon is checked to sit inside the grid (nothing clipped) in both themes

## Regenerate
```
cd source
pip install cairosvg pillow fonttools numpy
python3 build.py dist ../out        # svg + png deliverable tree
python3 build.py sprites ..         # sprite-light.svg + sprite-dark.svg (REQUIRED)
python3 build.py sheet all light sheet.png
python3 logo.py export ../logo      # logo files
```

> `python3 build.py sprites ..` writes `sprite-light.svg` and `sprite-dark.svg` next
> to this README. They are embedded by `crates/rcad-app` with `include_str!`, so
> they must be regenerated and committed whenever an icon changes - otherwise the
> desktop app does not build.
Add an icon: create a function decorated with `@icon("<category>", "<name>", "<Label>")` in `source/icons/`.

## Notes
- All artwork is original; it is not copied from PTC Creo or any other product. rcad is not affiliated with PTC.
- The wordmark is outlined from Inter Display Bold (SIL Open Font License 1.1), so no font install is needed.
- Icons keep their own colours (they are not single-colour glyphs). For monochrome toolbars a single-colour pass would be a separate set.
