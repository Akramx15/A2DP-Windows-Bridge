//! WinUSB assignment and restore through the elevated helper
//! bluecaster-driver.exe (the core itself never runs as administrator).
//!
//! The helper writes its JSON result to a file we name (an elevated process
//! started with ShellExecuteEx gives no stdout). The "previous driver"
//! record of an assignment is kept in the settings folder's
//! `adapters\driver\`, per adapter, for the restore (and the uninstaller).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ipc::{ErrorInfo, Msg};
use serde_json::Value;
use win_sys::bt_radios::WindowsRadio;
use win_sys::elevation::{self, ElevationError};

use crate::state::CoreState;

fn err(code: &str, key: &str) -> ErrorInfo {
    ErrorInfo { code: code.into(), message: Msg::new(key) }
}

fn helper() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("bluecaster-driver.exe")))
        .unwrap_or_else(|| "bluecaster-driver.exe".into())
}

/// File of the assignment record of an adapter
fn record_path(state: &CoreState, instance_id: &str) -> PathBuf {
    bluecaster_core::paths::driver_record_file(&state.config_dir(), instance_id)
}

pub(crate) fn run(config: &Path, args: &[&str]) -> Result<(u32, Value), ErrorInfo> {
    let out = std::env::temp_dir().join(format!("bluecaster-driver-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let mut all: Vec<&str> = args.to_vec();
    let out_s = out.to_string_lossy().into_owned();
    all.extend(["--out", &out_s]);
    // The elevated run's log next to the core's (the helper appends to it,
    // in a folder that must exist)
    let log = bluecaster_core::paths::driver_log_file(config, &bluecaster_core::paths::today());
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let log_s = log.to_string_lossy().into_owned();
    all.extend(["--log", &log_s]);
    if crate::logging::verbose_now().is_some() {
        all.push("--verbose");
    }
    let code = elevation::run_elevated(&helper(), &all).map_err(|e| match e {
        ElevationError::Cancelled => err("adapter.uac_cancelled", "error.uac_cancelled"),
        _ => {
            tracing::error!("driver helper: {e}");
            err("adapter.driver_failed", "error.driver_failed")
        }
    })?;
    let json: Option<Value> =
        std::fs::read_to_string(&out).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok());
    let _ = std::fs::remove_file(&out);
    // windows-keys and restore give link keys back: not for the log
    let shown = json.as_ref().map(without_keys);
    tracing::info!("driver helper {} -> exit {code}: {shown:?}", args.join(" "));
    Ok((code, json.unwrap_or(Value::Null)))
}

/// A helper's result as the log may show it: lists of keys by their number
fn without_keys(result: &Value) -> Value {
    let mut shown = result.clone();
    for field in ["keys", "le_keys", "replaced_keys"] {
        if let Some(list) = result.get(field).and_then(Value::as_array) {
            shown[field] = Value::String(format!("{} key(s)", list.len()));
        }
    }
    shown
}

/// For `restore --keys`: the adapter's address and BlueCaster's link keys,
/// written to the pairing folder (never in a bundle); None without either
fn keys_for_windows(state: &CoreState, instance_id: &str) -> Option<PathBuf> {
    let (_, s) = state.snapshot();
    let radio = s.adapter.adapters.iter().find(|a| a.instance_id == instance_id)?.address.clone()?;
    let config = state.config_dir();
    let store = bt_host::link_keys::FileLinkKeyStore::open(&bluecaster_core::paths::link_keys_file(&config));
    if store.entries().is_empty() {
        return None;
    }
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let keys: Vec<Value> = store
        .entries()
        .iter()
        .map(|e| serde_json::json!({ "address": bt_host::addr_str(&e.addr), "key": hex(&e.key) }))
        .collect();
    let path = bluecaster_core::paths::keys_for_windows_file(&config);
    let text = serde_json::to_vec(&serde_json::json!({ "radio": radio, "keys": keys })).ok()?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::write(&path, text) {
        Ok(()) => Some(path),
        Err(e) => {
            tracing::warn!("cannot write {}: {e}; Windows keeps its own keys", path.display());
            None
        }
    }
}

/// Windows' keys that BlueCaster's replaced, added to the backup in the
/// pairing folder (per device the one replaced last)
fn keep_replaced_keys(config: &Path, instance_id: &str, result: &Value) {
    if let Some(e) = result.get("keys_error").and_then(Value::as_str) {
        tracing::warn!("driver: link keys not written to Windows: {e}");
    }
    let written = result.get("keys_written").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    let same = result.get("keys_same").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    if result.get("keys_written").is_some() {
        tracing::info!("driver: {written} link key(s) written to Windows for {instance_id}, {same} the same already");
    }
    let Some(replaced) = result.get("replaced_keys").and_then(Value::as_array).filter(|r| !r.is_empty()) else {
        return;
    };
    let path = bluecaster_core::paths::replaced_windows_keys_file(config);
    let mut backup: Vec<Value> =
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let date = bluecaster_core::paths::today();
    for k in replaced {
        let address = k.get("address").and_then(Value::as_str).unwrap_or_default();
        backup.retain(|b| b.get("address").and_then(Value::as_str) != Some(address));
        let mut entry = k.clone();
        entry["adapter"] = Value::String(instance_id.to_string());
        entry["date"] = Value::String(date.clone());
        backup.push(entry);
    }
    if let Err(e) = std::fs::write(&path, serde_json::to_vec_pretty(&backup).unwrap_or_default()) {
        tracing::warn!("cannot keep Windows' replaced keys in {}: {e}", path.display());
    }
}

pub fn assign(state: &Arc<CoreState>, instance_id: &str) -> Result<Value, ErrorInfo> {
    // The stack must let go of the adapter (it may be ours already)
    state.service.release_adapter();
    let (code, result) = run(&state.config_dir(), &["assign", instance_id])?;
    if code != 0 {
        state.refresh_adapters();
        // Some environments refuse DiInstallDevice with the inbox winusb.inf;
        // Zadig's signed package is the way out there (docs/setup.md#zadig)
        return Err(err("adapter.driver_failed", "error.driver_assign_failed"));
    }
    let path = record_path(state, instance_id);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, serde_json::to_vec_pretty(&result).unwrap_or_default()) {
        tracing::warn!("cannot keep the driver record {}: {e}", path.display());
    }
    let mut adapters = state.refresh_adapters();
    state.driver_changed();
    // Even after the helper restarted the device, Windows may load WinUSB
    // only after a replug or a restart (issue #81: until then opening the
    // adapter fails with "failed to initialize WinUSB (error 50)")
    if result.get("reboot_required").and_then(Value::as_bool) == Some(true) {
        tracing::warn!(
            "driver: WinUSB takes effect for {instance_id} after the adapter is replugged or Windows restarts"
        );
        for a in adapters.iter_mut().filter(|a| a.instance_id == instance_id) {
            a.restart_needed = true;
        }
    }
    Ok(serde_json::to_value(adapters).unwrap_or(Value::Null))
}

pub fn restore(state: &Arc<CoreState>, instance_id: &str) -> Result<Value, ErrorInfo> {
    state.service.release_adapter();
    let path = record_path(state, instance_id);
    let previous = path.exists().then(|| format!("@{}", path.display()));
    let mut args = vec!["restore", instance_id];
    if let Some(p) = previous.as_deref() {
        args.extend(["--previous", p]);
    }
    // BlueCaster's pairings go to Windows too: the devices it paired since
    // gave up Windows' older keys (issue #86)
    let keys = keys_for_windows(state, instance_id);
    let keys_arg = keys.as_ref().map(|p| p.to_string_lossy().into_owned());
    if let Some(k) = keys_arg.as_deref() {
        args.extend(["--keys", k]);
    }
    let ran = run(&state.config_dir(), &args);
    if let Some(k) = &keys {
        let _ = std::fs::remove_file(k);
    }
    let (code, result) = ran?;
    keep_replaced_keys(&state.config_dir(), instance_id, &result);
    state.refresh_adapters();
    match code {
        0 => {
            let _ = std::fs::remove_file(&path);
            state.driver_changed();
            if let Some(other) = other_radio_in_use(instance_id) {
                return Err(ErrorInfo {
                    code: "adapter.restore_other_radio".into(),
                    message: Msg { key: "error.driver_restore_other_radio".into(), args: vec![other] },
                });
            }
            Ok(serde_json::to_value(state.snapshot().1.adapter.adapters).unwrap_or(Value::Null))
        }
        // Done, but not on the recorded driver (e.g. Zadig's WinUSB package
        // still in the driver store outranks Windows' Bluetooth driver)
        5 => Err(err("adapter.restore_unverified", "error.driver_restore_unverified")),
        _ => Err(err("adapter.driver_failed", "error.driver_failed")),
    }
}

/// After a restore that left the adapter on its driver but not working:
/// the name of another radio Windows' Bluetooth runs. Windows runs one
/// radio at a time (BTHUSB event 6, the adapter fails with Code 31), so
/// the restored one stays off until that one is disabled.
fn other_radio_in_use(instance_id: &str) -> Option<String> {
    let state = win_sys::driver::driver_state(instance_id).ok()?;
    let name = blocking_radio(instance_id, state.problem_code, &win_sys::bt_radios::radios())?;
    tracing::warn!(
        "driver: {instance_id} is back on {} but has problem code {:?}; Windows runs {name:?}",
        state.driver.service,
        state.problem_code
    );
    Some(name)
}

/// The radio that keeps a failed adapter off: one not on that adapter
fn blocking_radio(instance_id: &str, problem: Option<u32>, radios: &[WindowsRadio]) -> Option<String> {
    problem?;
    let other = radios.iter().find(|r| !r.on_device(instance_id))?;
    Some(if other.name.is_empty() {
        other.instance_ids.first().cloned().unwrap_or_default()
    } else {
        other.name.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn radio(name: &str, ids: &[&str]) -> WindowsRadio {
        WindowsRadio {
            instance_ids: ids.iter().map(|s| s.to_string()).collect(),
            name: name.into(),
            address: None,
            hci_version: None,
            lmp_version: None,
        }
    }

    const TPLINK: &str = r"USB\VID_2357&PID_0604\E848B8C82000";
    const INTEL: &str = r"USB\VID_8087&PID_0033\5&3B72E4CF&0&14";

    #[test]
    fn replaced_windows_keys_are_kept_but_never_logged() {
        let config = std::env::temp_dir().join(format!("bluecaster-replaced-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&config);
        std::fs::create_dir_all(bluecaster_core::paths::pairing_dir(&config)).unwrap();
        let result = |key: &str| {
            serde_json::json!({
                "ok": true,
                "keys_written": ["AA:BB:CC:DD:EE:FF"],
                "keys_same": [],
                "replaced_keys": [{ "address": "AA:BB:CC:DD:EE:FF", "key": key }],
            })
        };
        let shown = without_keys(&result("11").clone()).to_string();
        assert!(!shown.contains("\"11\""), "{shown}");
        assert!(shown.contains("1 key(s)"));
        keep_replaced_keys(&config, TPLINK, &result("11"));
        // Replaced again: the one replaced last is kept
        keep_replaced_keys(&config, TPLINK, &result("22"));
        // Nothing replaced: the backup stays
        keep_replaced_keys(&config, TPLINK, &serde_json::json!({ "keys_written": [] }));
        let path = bluecaster_core::paths::replaced_windows_keys_file(&config);
        let backup: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&config);
        assert_eq!(backup.len(), 1);
        assert_eq!(backup[0]["key"], "22");
        assert_eq!(backup[0]["adapter"], TPLINK);
    }

    #[test]
    fn another_radio_keeps_the_restored_adapter_off() {
        let intel = [radio("Intel Wireless Bluetooth", &[INTEL, r"USB\ROOT_HUB30\4&1"])];
        assert_eq!(blocking_radio(TPLINK, Some(31), &intel).as_deref(), Some("Intel Wireless Bluetooth"));
        // Working after the restore: nothing to say
        assert_eq!(blocking_radio(TPLINK, None, &intel), None);
        // Failed, but Windows runs no other radio: not this cause
        assert_eq!(blocking_radio(TPLINK, Some(31), &[radio("TP-Link", &[TPLINK])]), None);
        assert_eq!(blocking_radio(TPLINK, Some(31), &[]), None);
        // A radio without a name is named by its device
        assert_eq!(blocking_radio(TPLINK, Some(31), &[radio("", &[INTEL])]).as_deref(), Some(INTEL));
    }
}
