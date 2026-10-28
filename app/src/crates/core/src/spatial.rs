//! The spatial engine registry: the engine programs a profile can choose.
//!
//! A registry is a folder with one subfolder per engine stage; each stage has
//! its engine program and, usually, a stage manifest with the label the user
//! sees ("Personalized HPP + XM6 BA"). The engine reads its assets next to
//! its program, so a stage folder is a complete engine.
//!
//! A stage can also declare the settings the user may change (its manifest's
//! "controls" block: sliders and choices). The stage reads them from the
//! config file its command names, and only when it starts: the app keeps
//! the user's values in its own copy of that config (in the settings
//! folder; the stage's own files must stay as they are, the stage checks
//! them against its manifest) and starts the stage with that copy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::profiles::Profile;

/// One slider of a stage: a value between `min` and `max`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpatialSlider {
    /// The key the value has in the stage's config
    pub key: String,
    /// The label the UI shows
    pub label: String,
    /// The smallest value the slider takes
    pub min: f64,
    /// The largest one
    pub max: f64,
    /// How much one step of the slider is
    pub step: f64,
    /// The value the stage's manifest declares
    pub default: f64,
    /// The value the stage runs with now
    pub value: f64,
}

/// One choice of a stage: one of its `options`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpatialSelect {
    /// The key the value has in the stage's config
    pub key: String,
    /// The label the UI shows
    pub label: String,
    /// The values the user can choose from
    pub options: Vec<String>,
    /// The value the stage's manifest declares
    pub default: String,
    /// The value the stage runs with now
    pub value: String,
}

/// The controls of a stage, with the values it runs with now
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct SpatialControls {
    /// The stage has controls the user can change
    pub supported: bool,
    /// The sliders, in the order the manifest lists them
    pub sliders: Vec<SpatialSlider>,
    /// The choices, in the order the manifest lists them
    pub selects: Vec<SpatialSelect>,
}

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

impl StageCommand {
    /// The command with its config file replaced: the stage is started with
    /// the app's copy of the config (the user's values) instead of the
    /// stage's own. A command without a config file stays as it is.
    pub fn with_config(&self, config: &Path) -> StageCommand {
        let mut args = self.args.clone();
        if let Some(i) = args.iter().position(|a| a == "--config")
            && let Some(slot) = args.get_mut(i + 1)
        {
            *slot = config.display().to_string();
        }
        StageCommand { program: self.program.clone(), args, cwd: self.cwd.clone() }
    }
}

/// The command of the stage the engine program belongs to
pub fn command_for(program: &Path) -> StageCommand {
    let dir = dir_of(program);
    let manifest = manifest_of(&dir);
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
/// subfolder with one (alphabetically). An empty `dir` is the app's own
/// engines folder ([`default_registry`]).
pub fn list(dir: &Path) -> Vec<SpatialEngine> {
    let dir = registry_dir(dir);
    let mut out = Vec::new();
    if let Some(engine) = engine_in(&dir) {
        out.push(engine);
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
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

/// The folder a registry path names: an empty one is the app's own engines
/// folder. A path copied from a JSON file or a log may carry quotes and the
/// doubled separators of an escaped path: both go.
pub fn registry_dir(dir: &Path) -> PathBuf {
    let clean = clean_path(&dir.to_string_lossy());
    if clean.is_empty() { default_registry() } else { PathBuf::from(clean) }
}

/// The app's own engines folder (`engines\` in the settings folder), for a
/// profile that chose none: the stages are installed there by hand
pub fn default_registry() -> PathBuf {
    crate::paths::config_dir().join("engines")
}

/// A path as it is used: without the quotes a copied one carries and with
/// the doubled separators of an escaped one ("C:\\engines") made single; a
/// UNC path keeps its leading "\\"
fn clean_path(text: &str) -> String {
    let text = text.trim().trim_matches('"');
    let unc = text.starts_with("\\\\");
    let body = if unc { &text[2..] } else { text };
    let mut out = String::with_capacity(text.len());
    if unc {
        out.push_str("\\\\");
    }
    for c in body.chars() {
        if c == '\\' && out.ends_with('\\') {
            continue;
        }
        out.push(c);
    }
    out
}

/// The engine program of one stage folder, with its label
fn engine_in(dir: &Path) -> Option<SpatialEngine> {
    let manifest = manifest_of(dir);
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

/// The stage's folder: the one the program is in
fn dir_of(program: &Path) -> PathBuf {
    program.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The stage's manifest (next to the program), if it has one
fn manifest_of(dir: &Path) -> Option<Value> {
    std::fs::read_to_string(dir.join("STAGE-MANIFEST.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
}

/// The stage's config file: the one its command names after "--config"
/// (relative to the stage folder). None without such a command.
pub fn config_path(program: &Path) -> Option<PathBuf> {
    let command = command_for(program);
    let i = command.args.iter().position(|a| a == "--config")?;
    let name = command.args.get(i + 1)?;
    let path = Path::new(name);
    Some(if path.is_absolute() { path.to_path_buf() } else { dir_of(program).join(path) })
}

/// The stage's config: what its own config file holds (an empty object
/// without one)
pub fn config_of(program: &Path) -> Value {
    config_path(program)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| Value::Object(Map::new()))
}

/// The controls the stage's manifest declares, with the values its own
/// config holds (the manifest's defaults without a config file)
pub fn controls_of(program: &Path) -> SpatialControls {
    let manifest = manifest_of(&dir_of(program));
    let config = config_of(program);
    controls_from(manifest.as_ref(), &config)
}

/// The controls of a stage, from its manifest and its config
fn controls_from(manifest: Option<&Value>, config: &Value) -> SpatialControls {
    let Some(block) = manifest.and_then(|m| m.get("controls")).filter(|c| c.is_object()) else {
        return SpatialControls::default();
    };
    let mut sliders = Vec::new();
    for s in block.get("sliders").and_then(Value::as_array).into_iter().flatten() {
        let Some(key) = s.get("key").and_then(Value::as_str).filter(|k| !k.is_empty()) else {
            continue;
        };
        let min = s.get("min").and_then(Value::as_f64).unwrap_or(0.0);
        let max = s.get("max").and_then(Value::as_f64).unwrap_or(1.0);
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let step = s.get("step").and_then(Value::as_f64).filter(|v| *v > 0.0).unwrap_or((max - min) / 100.0);
        let default = s.get("default").and_then(Value::as_f64).unwrap_or(min).clamp(min, max);
        let value = config_value(config, key).and_then(Value::as_f64).unwrap_or(default).clamp(min, max);
        sliders.push(SpatialSlider {
            key: key.to_string(),
            label: s.get("label").and_then(Value::as_str).unwrap_or(key).to_string(),
            min,
            max,
            step,
            default,
            value,
        });
    }
    let mut selects = Vec::new();
    for s in block.get("selects").and_then(Value::as_array).into_iter().flatten() {
        let Some(key) = s.get("key").and_then(Value::as_str).filter(|k| !k.is_empty()) else {
            continue;
        };
        let options: Vec<String> = s
            .get("options")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|o| o.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if options.is_empty() {
            continue;
        }
        let default = s
            .get("default")
            .and_then(Value::as_str)
            .filter(|d| options.iter().any(|o| o.as_str() == *d))
            .unwrap_or(options[0].as_str())
            .to_string();
        let value = config_value(config, key)
            .and_then(Value::as_str)
            .filter(|v| options.iter().any(|o| o.as_str() == *v))
            .unwrap_or(default.as_str())
            .to_string();
        selects.push(SpatialSelect {
            key: key.to_string(),
            label: s.get("label").and_then(Value::as_str).unwrap_or(key).to_string(),
            options,
            default,
            value,
        });
    }
    SpatialControls { supported: !sliders.is_empty() || !selects.is_empty(), sliders, selects }
}

/// The value a key has in the stage's config: along its path (a key with
/// dots), at the top level, or where the key is in the config's objects
fn config_value<'a>(config: &'a Value, key: &str) -> Option<&'a Value> {
    lookup_path(config, key).or_else(|| find_key(config, key))
}

/// The value at a dotted path ("spatial.center_focus")
fn lookup_path<'a>(config: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = config;
    for part in path.split('.') {
        cur = cur.as_object()?.get(part)?;
    }
    Some(cur)
}

/// The value of a key wherever it is in the config (the first one found)
fn find_key<'a>(config: &'a Value, key: &str) -> Option<&'a Value> {
    match config {
        Value::Object(map) => map.get(key).or_else(|| map.values().find_map(|v| find_key(v, key))),
        Value::Array(items) => items.iter().find_map(|v| find_key(v, key)),
        _ => None,
    }
}

/// Sets a key's value in the config: where the key is (a key that is in the
/// config), along its path (a key with dots), or at the top level
fn set_value(config: &mut Value, key: &str, value: Value) {
    if key.contains('.') {
        set_path(config, key, value);
        return;
    }
    if let Some(slot) = find_slot(config, key) {
        *slot = value;
        return;
    }
    if let Value::Object(map) = config {
        map.insert(key.to_string(), value);
    }
}

/// The place a key's value is in the config, if it is there at all
fn find_slot<'a>(config: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    match config {
        Value::Object(map) => {
            if map.contains_key(key) {
                return map.get_mut(key);
            }
            for v in map.values_mut() {
                if let Some(slot) = find_slot(v, key) {
                    return Some(slot);
                }
            }
            None
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                if let Some(slot) = find_slot(v, key) {
                    return Some(slot);
                }
            }
            None
        }
        _ => None,
    }
}

/// Sets the value at a dotted path, making the objects on the way
fn set_path(config: &mut Value, path: &str, value: Value) {
    let Some((head, rest)) = path.split_once('.') else {
        if let Value::Object(map) = config {
            map.insert(path.to_string(), value);
        }
        return;
    };
    let Value::Object(map) = config else {
        return;
    };
    let entry = map.entry(head.to_string()).or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    set_path(entry, rest, value);
}

/// Where the app keeps a profile's control values: `spatial\<profile>.json`
/// in the settings folder (the stage's config with the user's values)
pub fn values_path(dir: &Path, profile: &str) -> PathBuf {
    crate::paths::spatial_dir(dir).join(format!("{}.json", file_name(profile)))
}

/// The user's values for a profile, as they are kept in the profile
pub fn stored_controls(profile: &Profile) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let Some(values) = profile.other.get(VALUES_KEY).and_then(Value::as_object) else {
        return out;
    };
    for (key, value) in values {
        out.insert(key.clone(), value.clone());
    }
    out
}

/// The controls of the stage the profile uses: the ones its manifest
/// declares, with the values the user set (the stage's own config
/// otherwise)
pub fn controls_for(profile: &Profile, program: &Path) -> SpatialControls {
    let mut controls = controls_of(program);
    apply_values(&mut controls, &stored_controls(profile));
    controls
}

/// Stores the user's values: they go into the app's copy of the stage's
/// config (the stage's own files stay as they are) and into the profile, so
/// the copy can be written again when it goes missing. The new state comes
/// back.
pub fn write_controls(
    dir: &Path,
    profile: &mut Profile,
    program: &Path,
    values: &BTreeMap<String, Value>,
) -> Result<SpatialControls, String> {
    let controls = controls_of(program);
    if !controls.supported {
        return Err("the stage has no controls".to_string());
    }
    // The values the profile had, with the new ones
    let mut merged = stored_controls(profile);
    for (key, value) in values {
        check_value(&controls, key, value)?;
        merged.insert(key.clone(), value.clone());
    }
    // The values the stage does not know any more are dropped
    merged.retain(|key, _| {
        controls.sliders.iter().any(|s| &s.key == key) || controls.selects.iter().any(|s| &s.key == key)
    });
    store_controls(profile, &merged);
    write_config(dir, &profile.name, program, &merged).map_err(|e| e.to_string())?;
    if config_path(program).is_none() {
        // The values have nothing to apply to: the stage reads no config file
        tracing::warn!("spatial: the stage of \"{}\" reads no config file: its values stay unused", profile.name);
    }
    let mut controls = controls;
    apply_values(&mut controls, &merged);
    Ok(controls)
}

/// The app's copy of the stage's config the stage is started with: it is
/// written when the profile has values (and again when it went missing),
/// and the stage reads it instead of its own config file
pub fn config_for(dir: &Path, profile: &Profile, program: &Path) -> Option<PathBuf> {
    let values = stored_controls(profile);
    if values.is_empty() {
        return None;
    }
    // The stage reads its settings from a config file: without one the
    // values have nothing to apply to
    config_path(program)?;
    let path = values_path(dir, &profile.name);
    if !path.is_file() {
        // The values are there, the file is not: it is written again
        if let Err(e) = write_config(dir, &profile.name, program, &values) {
            tracing::warn!("spatial: the config of \"{}\" is not written: {e}", profile.name);
            return None;
        }
    }
    Some(path)
}

/// The app's copy of the stage's config of a profile is not needed any more
/// (the profile is gone)
pub fn remove_values(dir: &Path, profile: &str) {
    let path = values_path(dir, profile);
    if let Err(e) = std::fs::remove_file(&path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("spatial: the config of \"{profile}\" is not removed: {e}");
    }
}

/// The app's copy of the stage's config follows the profile's name
pub fn rename_values(dir: &Path, from: &str, to: &str) {
    let old = values_path(dir, from);
    if !old.is_file() {
        return;
    }
    let new = values_path(dir, to);
    if let Err(e) = std::fs::rename(&old, &new) {
        tracing::warn!("spatial: the config of \"{from}\" is not renamed: {e}");
    }
}

/// The app's copy of the stage's config: the stage's own config with the
/// user's values in it
fn write_config(
    dir: &Path,
    profile: &str,
    program: &Path,
    values: &BTreeMap<String, Value>,
) -> std::io::Result<PathBuf> {
    let mut config = config_of(program);
    for (key, value) in values {
        set_value(&mut config, key, value.clone());
    }
    let path = values_path(dir, profile);
    let text = serde_json::to_string_pretty(&config).map_err(std::io::Error::other)?;
    std::fs::write(&path, text)?;
    Ok(path)
}

/// Keeps the user's values in the profile
fn store_controls(profile: &mut Profile, values: &BTreeMap<String, Value>) {
    if values.is_empty() {
        profile.other.remove(VALUES_KEY);
        return;
    }
    let map = values.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Map<String, Value>>();
    profile.other.insert(VALUES_KEY.to_string(), Value::Object(map));
}

/// Puts the values into the controls (the ones the stage does not know are
/// left alone)
fn apply_values(controls: &mut SpatialControls, values: &BTreeMap<String, Value>) {
    for slider in &mut controls.sliders {
        if let Some(value) = values.get(&slider.key).and_then(Value::as_f64) {
            slider.value = value.clamp(slider.min, slider.max);
        }
    }
    for select in &mut controls.selects {
        if let Some(value) = values.get(&select.key).and_then(Value::as_str)
            && select.options.iter().any(|o| o == value)
        {
            select.value = value.to_string();
        }
    }
}

/// Whether a value is one the control takes
fn check_value(controls: &SpatialControls, key: &str, value: &Value) -> Result<(), String> {
    if let Some(slider) = controls.sliders.iter().find(|s| s.key == key) {
        let Some(number) = value.as_f64() else {
            return Err(format!("\"{key}\" is not a number"));
        };
        if !number.is_finite() || number < slider.min || number > slider.max {
            return Err(format!("\"{key}\" is outside {}..{}", slider.min, slider.max));
        }
        return Ok(());
    }
    if let Some(select) = controls.selects.iter().find(|s| s.key == key) {
        let Some(text) = value.as_str() else {
            return Err(format!("\"{key}\" is not one of the choices"));
        };
        if !select.options.iter().any(|o| o == text) {
            return Err(format!("\"{text}\" is not one of the choices of \"{key}\""));
        }
        return Ok(());
    }
    Err(format!("\"{key}\" is not a control of the stage"))
}

/// The key the user's values have in the profile's own keys
const VALUES_KEY: &str = "spatial_controls";

/// A file name for a profile: its name, with what a file name cannot hold
/// replaced, and a hash so that two names that differ only there do not
/// share a file
fn file_name(profile: &str) -> String {
    let clean: String = profile
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        return format!("profile-{:016x}", hash(profile));
    }
    // The names Windows keeps for itself cannot be files
    let upper = clean.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper[3..].parse::<u8>().is_ok_and(|n| (1..=9).contains(&n)));
    if reserved {
        return format!("_{clean}-{:016x}", hash(profile));
    }
    if clean == profile {
        return clean.to_string();
    }
    format!("{clean}-{:016x}", hash(profile))
}

/// A short hash of a name (so that two names that cannot be file names do
/// not share a file)
fn hash(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stage with an engine program and a manifest
    fn stage(dir: &Path, name: &str, label: &str) {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("sony_native_stream.exe"), b"x").unwrap();
        std::fs::write(d.join("STAGE-MANIFEST.json"), format!("{{\"label\": \"{label}\"}}")).unwrap();
    }

    /// A stage with a slider and a choice, and a config file of its own
    fn stage_with_controls(dir: &Path) -> PathBuf {
        let d = dir.join("upmixer");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("upmixer.exe"), b"x").unwrap();
        std::fs::write(
            d.join("STAGE-MANIFEST.json"),
            r#"{
                "label": "Upmixer",
                "command": ["upmixer.exe", "--config", "config.json", "-", "stream", "1", "1024"],
                "controls": {
                    "sliders": [ { "key": "center_focus", "label": "Center focus",
                        "min": 0.0, "max": 1.0, "step": 0.01, "default": 0.5 } ],
                    "selects": [ { "key": "correction", "label": "Bass correction",
                        "options": ["none", "sony-xm6-ba"], "default": "none" } ]
                }
            }"#,
        )
        .unwrap();
        std::fs::write(d.join("config.json"), r#"{"center_focus": 0.25, "correction": "sony-xm6-ba"}"#).unwrap();
        d.join("upmixer.exe")
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

    #[test]
    fn an_empty_registry_is_the_apps_own_folder() {
        // A profile that chose no folder: `engines\` in the settings folder
        assert_eq!(registry_dir(Path::new("")), default_registry());
        assert_eq!(registry_dir(Path::new("   ")), default_registry());
        assert!(default_registry().ends_with("engines"));
    }

    #[test]
    fn a_copied_registry_path_loses_its_quotes_and_doubled_separators() {
        assert_eq!(registry_dir(Path::new(r"C:\\Engines\\sony")), PathBuf::from(r"C:\Engines\sony"));
        assert_eq!(registry_dir(Path::new("\"C:\\\\Engines\"")), PathBuf::from(r"C:\Engines"));
        // A UNC path keeps its two leading separators
        assert_eq!(registry_dir(Path::new(r"\\server\share\engines")), PathBuf::from(r"\\server\share\engines"));
    }

    #[test]
    fn a_stage_says_which_controls_it_has() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-spatial-controls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let program = stage_with_controls(&dir);
        let controls = controls_of(&program);
        assert!(controls.supported);
        assert_eq!(controls.sliders.len(), 1);
        let slider = &controls.sliders[0];
        assert_eq!(slider.key, "center_focus");
        assert_eq!(slider.label, "Center focus");
        assert_eq!(slider.min, 0.0);
        assert_eq!(slider.max, 1.0);
        assert_eq!(slider.step, 0.01);
        assert_eq!(slider.default, 0.5);
        // The value the stage's own config holds
        assert_eq!(slider.value, 0.25);
        assert_eq!(controls.selects.len(), 1);
        let select = &controls.selects[0];
        assert_eq!(select.key, "correction");
        assert_eq!(select.options, ["none", "sony-xm6-ba"]);
        assert_eq!(select.default, "none");
        assert_eq!(select.value, "sony-xm6-ba");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stage_without_controls_has_none() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-spatial-nocontrols-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let d = dir.join("sony-native-reference");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("sony_native_stream.exe"), b"x").unwrap();
        std::fs::write(d.join("STAGE-MANIFEST.json"), r#"{"label": "Sony native"}"#).unwrap();
        let controls = controls_of(&d.join("sony_native_stream.exe"));
        assert!(!controls.supported);
        assert!(controls.sliders.is_empty());
        assert!(controls.selects.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_users_values_go_into_the_apps_own_config() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-spatial-values-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let program = stage_with_controls(&dir);
        let mut profile = Profile { name: "WH LDAC".into(), ..Profile::default() };
        let mut values = BTreeMap::new();
        values.insert("center_focus".to_string(), serde_json::json!(0.75));
        let controls = write_controls(&dir, &mut profile, &program, &values).unwrap();
        assert_eq!(controls.sliders[0].value, 0.75);
        // The other control keeps the value the stage's config had
        assert_eq!(controls.selects[0].value, "sony-xm6-ba");
        // The app's copy of the config holds the stage's config with the
        // user's value in it
        let file = values_path(&dir, "WH LDAC");
        assert!(file.is_file(), "{} is missing", file.display());
        let text = std::fs::read_to_string(&file).unwrap();
        let config: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(config["center_focus"], serde_json::json!(0.75));
        assert_eq!(config["correction"], serde_json::json!("sony-xm6-ba"));
        // The stage's own files are left alone
        let own = std::fs::read_to_string(program.parent().unwrap().join("config.json")).unwrap();
        assert!(own.contains("0.25"), "{own}");
        // The values are kept in the profile, so the file can be written
        // again when it goes missing
        assert_eq!(stored_controls(&profile).get("center_focus"), Some(&serde_json::json!(0.75)));
        assert_eq!(controls_for(&profile, &program).sliders[0].value, 0.75);
        // The file follows the profile's name, and goes with it
        rename_values(&dir, "WH LDAC", "WH");
        assert!(!file.is_file());
        assert!(values_path(&dir, "WH").is_file());
        remove_values(&dir, "WH");
        assert!(!values_path(&dir, "WH").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_value_the_control_does_not_take_is_refused() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-spatial-badvalue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let program = stage_with_controls(&dir);
        let mut profile = Profile { name: "WH".into(), ..Profile::default() };
        let mut values = BTreeMap::new();
        values.insert("center_focus".to_string(), serde_json::json!(2.0));
        assert!(write_controls(&dir, &mut profile, &program, &values).is_err());
        values.insert("center_focus".to_string(), serde_json::json!("loud"));
        assert!(write_controls(&dir, &mut profile, &program, &values).is_err());
        values.insert("correction".to_string(), serde_json::json!("loudness"));
        assert!(write_controls(&dir, &mut profile, &program, &values).is_err());
        values.insert("nonsense".to_string(), serde_json::json!(1));
        assert!(write_controls(&dir, &mut profile, &program, &values).is_err());
        // Nothing was written
        assert!(!values_path(&dir, "WH").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_stage_is_started_with_the_apps_config() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-spatial-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let program = stage_with_controls(&dir);
        let command = command_for(&program);
        assert!(command.args.windows(2).any(|w| w == ["--config", "config.json"]));
        let mine = dir.join("mine.json");
        let started = command.with_config(&mine);
        assert!(started.args.windows(2).any(|w| w[0] == "--config" && w[1] == mine.display().to_string()));
        // The rest of the command is as it was
        assert!(started.program.ends_with("upmixer.exe"));
        assert_eq!(started.args.last(), Some(&"1024".to_string()));
        // A command without a config file stays as it is
        let plain = StageCommand { program: program.clone(), args: vec!["-".into()], cwd: dir.clone() };
        assert_eq!(plain.with_config(&mine).args, plain.args);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_profile_names_its_own_file() {
        assert_eq!(file_name("WH LDAC"), "WH LDAC");
        // A name a file cannot hold gets one that can, and stays its own
        assert_ne!(file_name("a/b"), file_name("a\\b"));
        assert!(file_name("a/b").starts_with("a_b-"));
        assert!(file_name("CON").starts_with("_CON-"));
        assert!(file_name("").starts_with("profile-"));
    }
}
