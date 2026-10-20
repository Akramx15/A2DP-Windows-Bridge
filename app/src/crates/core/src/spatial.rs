//! The spatial engine registry: the engine programs a profile can choose.
//!
//! A registry is a folder with one subfolder per engine stage; each stage has
//! its engine program and, usually, a stage manifest with the label the user
//! sees ("Personalized HPP + XM6 BA"). The engine reads its assets next to
//! its program, so a stage folder is a complete engine.

use std::path::{Path, PathBuf};

/// One engine program of a registry
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SpatialEngine {
    /// The stage's id (its folder name)
    pub id: String,
    /// The stage's label (from its manifest, its folder name otherwise)
    pub label: String,
    /// The engine program
    pub path: String,
}

/// The engines in `dir`: the folder itself when it holds a program, then every
/// subfolder with one (alphabetically).
pub fn list(dir: &Path) -> Vec<SpatialEngine> {
    let mut out = Vec::new();
    if let Some(engine) = engine_in(dir) {
        out.push(engine);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        if let Some(engine) = engine_in(&d) {
            out.push(engine);
        }
    }
    out
}

/// The engine program of one stage folder, with its label
fn engine_in(dir: &Path) -> Option<SpatialEngine> {
    let program = ["sony_native_stream.exe", "sony_native_stream"].iter().map(|n| dir.join(n)).find(|p| p.is_file())?;
    let id = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let label = std::fs::read_to_string(dir.join("STAGE-MANIFEST.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("label").and_then(|l| l.as_str()).map(str::to_string))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| id.clone());
    Some(SpatialEngine { id, label, path: program.to_string_lossy().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(dir: &Path, name: &str, label: &str) {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("sony_native_stream.exe"), b"x").unwrap();
        std::fs::write(d.join("STAGE-MANIFEST.json"), format!("{{\"label\": \"{label}\"}}")).unwrap();
    }

    #[test]
    fn a_registry_lists_its_stages() {
        let dir = std::env::temp_dir().join("a2dpwb-spatial-test");
        let _ = std::fs::remove_dir_all(&dir);
        stage(&dir, "personal-hpp-ba", "Personalized HPP + XM6 BA");
        stage(&dir, "sony-native-reference", "Sony native (reference)");
        let found = list(&dir);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "personal-hpp-ba");
        assert_eq!(found[0].label, "Personalized HPP + XM6 BA");
        assert!(found[0].path.ends_with("sony_native_stream.exe"));
        assert_eq!(found[1].id, "sony-native-reference");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_without_engines_is_empty() {
        let dir = std::env::temp_dir().join("a2dpwb-spatial-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(list(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
