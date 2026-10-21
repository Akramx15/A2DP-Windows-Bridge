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
    /// The stage's family ("sony", "upmixer"): what the effect is
    pub family: String,
}

/// How a stage is started: the program, its arguments and the folder it runs
/// in. A stage's manifest carries the command; without one the engine's own
/// convention is used (the program with "- stream 1 1024").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// The command of the stage the engine program belongs to
pub fn command_for(program: &Path) -> StageCommand {
    let dir = program.parent().map(Path::to_path_buf).unwrap_or_default();
    let manifest = std::fs::read_to_string(dir.join("STAGE-MANIFEST.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let declared = manifest
        .as_ref()
        .and_then(|m| m.get("command"))
        .and_then(|c| c.as_array())
        .map(|items| items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect::<Vec<String>>())
        .filter(|items| !items.is_empty());
    let (program, args) = match declared {
        Some(items) => {
            let mut it = items.into_iter();
            let first = it.next().unwrap_or_default();
            let rest: Vec<String> = it.collect();
            (resolve(&dir, &first), rest)
        }
        None => (
            resolve(&dir, "sony_native_stream"),
            vec!["-".to_string(), "stream".to_string(), "1".to_string(), "1024".to_string()],
        ),
    };
    StageCommand { program, args, cwd: dir }
}

/// The program of a stage: relative to the stage folder, and with the
/// Windows executable suffix when the file carries none
fn resolve(dir: &Path, name: &str) -> PathBuf {
    let candidate = Path::new(name);
    let path = if candidate.is_absolute() { candidate.to_path_buf() } else { dir.join(candidate) };
    if path.extension().is_none() {
        let with_exe = path.with_extension("exe");
        if with_exe.is_file() {
            return with_exe;
        }
    }
    path
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
    let manifest = std::fs::read_to_string(dir.join("STAGE-MANIFEST.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    // The program of the stage: the one its manifest names (a stage can be a
    // program of its own, e.g. a Python runtime with a native library), or the
    // engine's own executable.
    let named = manifest
        .as_ref()
        .and_then(|m| m.get("program"))
        .and_then(|p| p.as_str())
        .map(|name| resolve(dir, name))
        .filter(|p| p.is_file() && p.starts_with(dir));
    let program = named.or_else(|| {
        ["sony_native_stream.exe", "sony_native_stream"].iter().map(|n| dir.join(n)).find(|p| p.is_file())
    })?;
    let id = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let text = |key: &str| manifest.as_ref().and_then(|m| m.get(key)).and_then(|v| v.as_str()).map(str::to_string);
    let label = text("label").filter(|l| !l.is_empty()).unwrap_or_else(|| id.clone());
    let family = text("family").unwrap_or_default();
    Some(SpatialEngine { id, label, path: program.to_string_lossy().to_string(), family })
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
