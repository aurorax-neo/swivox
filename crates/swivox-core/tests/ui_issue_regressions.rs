use swivox_core::settings::{BackendSettings, SettingsStore};
use serde_json::json;

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
