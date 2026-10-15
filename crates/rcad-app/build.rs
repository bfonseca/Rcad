//! Embeds the generated rcad-icons PNG set so the toolbar can render real icons.
//!
//! The icons are authored and rasterised in `rcad-icons/`. Embedding the 64px
//! PNGs (instead of rasterising the SVGs at runtime) keeps the dependency tree
//! small and works offline. If the assets are missing the table is simply empty
//! and the UI falls back to its text/emoji icons.
//!
//! The icon id used by the toolbar is `<category>-<name>` (for example
//! `features-extrude`), derived here from the file path
//! `png/64/<theme>/<category>/<name>.png`.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Icon size and theme root, relative to the crate manifest directory.
const ICON_DIR: &str = "../../rcad-icons/png/64";

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join(ICON_DIR);
    println!("cargo:rerun-if-changed={ICON_DIR}");
    for theme in ["light", "dark"] {
        println!("cargo:rerun-if-changed={ICON_DIR}/{theme}");
    }

    let light = collect_theme(&root.join("light"));
    let dark = collect_theme(&root.join("dark"));

    let mut generated = String::new();
    generated.push_str("/// One embedded icon, available in both themes.\n");
    generated.push_str("pub struct EmbeddedIcon {\n");
    generated.push_str("    pub id: &'static str,\n");
    generated.push_str("    pub light: &'static [u8],\n");
    generated.push_str("    pub dark: &'static [u8],\n");
    generated.push_str("}\n\n");
    generated.push_str("/// Every icon shipped by rcad-icons, keyed by theme-neutral id.\n");
    generated.push_str("pub static EMBEDDED_ICONS: &[EmbeddedIcon] = &[\n");

    let mut count = 0usize;
    for (id, light_rel) in &light {
        // Only include icons that exist in both themes so switching theme
        // never makes an icon disappear.
        let Some(dark_rel) = dark.get(id) else { continue };
        let light_path = format!("{ICON_DIR}/light/{light_rel}");
        let dark_path = format!("{ICON_DIR}/dark/{dark_rel}");
        let _ = writeln!(generated, "    EmbeddedIcon {{");
        let _ = writeln!(generated, "        id: \"{id}\",");
        let _ = writeln!(
            generated,
            "        light: include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{light_path}\")),"
        );
        let _ = writeln!(
            generated,
            "        dark: include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{dark_path}\")),"
        );
        generated.push_str("    },\n");
        count += 1;
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("icon_pngs.rs"), generated).expect("write icon table");
    // Kept out of the build log: the count is asserted by the icon tests instead.
    let _ = count;
}

/// Map `<category>/<name>.png` to `<category>-<name>` for every PNG below `base`.
fn collect_theme(base: &Path) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    walk(base, base, &mut found);
    found
}

fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(base, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("png") {
            let Ok(relative) = path.strip_prefix(base) else {
                continue;
            };
            let id = relative
                .with_extension("")
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("-");
            out.insert(id, relative.to_string_lossy().replace('\\', "/"));
        }
    }
}