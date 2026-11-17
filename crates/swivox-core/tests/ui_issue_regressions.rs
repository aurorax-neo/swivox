use swivox_core::settings::{BackendSettings, SettingsStore};
use serde_json::json;
use std::process::Command;

#[test]
fn session_share_keeps_legacy_default_and_round_trips_partial_updates() {
    let old: BackendSettings =
        serde_json::from_value(json!({"enhancementsEnabled": true})).unwrap();
    assert_eq!(
        serde_json::to_value(old).unwrap()["codexAppSessionShare"],
        true
    );
    let temp = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(temp.path().join("settings.json"));
    let profiles = serde_json::to_value(store.load().unwrap()).unwrap()["relayProfiles"].clone();
    store
        .update(json!({"codexAppSessionShare": false}))
        .unwrap();
    for invalid in [json!(null), json!("true"), json!(1), json!([]), json!({})] {
        let updated = store
            .update(json!({"codexAppSessionShare": invalid}))
            .unwrap();
        let value = serde_json::to_value(updated).unwrap();
        assert_eq!(value["codexAppSessionShare"], false);
        assert_eq!(value["relayProfiles"], profiles);
    }
    store.update(json!({"enhancementsEnabled": false})).unwrap();
    let updated = store.update(json!({"codexAppSessionShare": true})).unwrap();
    let value = serde_json::to_value(updated).unwrap();
    assert_eq!(value["codexAppSessionShare"], true);
    assert_eq!(value["enhancementsEnabled"], false);
    assert_eq!(serde_json::to_value(store.load().unwrap()).unwrap(), value);
}

#[test]
fn renderer_share_and_manager_fold_behaviors() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manager = root.join("apps/swivox-manager");
    let has_ts_compiler = manager.join("node_modules/typescript").exists();
    let mut test_files: Vec<String> = std::fs::read_dir(manager.join("src"))
        .expect("read swivox-manager/src")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".test.ts") {
                return None;
            }
            if !has_ts_compiler {
                let text = std::fs::read_to_string(e.path()).unwrap_or_default();
                if text.contains("from \"typescript\"") || text.contains("from 'typescript'") {
                    return None;
                }
            }
            Some(format!("src/{name}"))
        })
        .collect();
    test_files.sort();
    let output = Command::new("node")
        .arg("--test")
        .args(&test_files)
        .current_dir(&manager)
        .output()
        .expect("node is required for renderer UI tests");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn session_delete_renderer_behaviors() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("node")
        .args(["--test", "src/session-delete-runtime.test.ts"])
        .current_dir(root.join("apps/swivox-manager"))
        .output()
        .expect("node is required for session deletion UI tests");
    assert!(output.status.success(), "{}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn manager_ui_typecheck() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manager = root.join("apps/swivox-manager");
    let tsc = manager.join("node_modules/typescript/bin/tsc");
    if !tsc.exists() {
        // 已迁移至纯 Rust + GPUI (`apps/swivox-gpui`)，退役前端未安装 node_modules 时跳过 tsc
        return;
    }
    let output = Command::new("node")
        .arg(tsc)
        .args(["--noEmit", "-p", "tsconfig.json"])
        .current_dir(&manager)
        .output()
        .expect("node is required for manager UI checks");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
