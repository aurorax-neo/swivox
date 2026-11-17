//! Antigravity 设备指纹（Machine ID / Telemetry ID）生成、`storage.json` 读写与本机基线保护。

use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityDeviceProfile {
    /// 格式: `auth0|user_<32位小写十六进制/字母数字>`
    pub machine_id: String,
    /// 格式: 标准小写 UUID v4
    pub mac_machine_id: String,
    /// 格式: 标准小写 UUID v4（同时同步至 `storage.serviceMachineId`）
    pub dev_device_id: String,
    /// 格式: `{大写 UUID v4}`
    pub sqm_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityDeviceHistoryEntry {
    pub label: String,
    pub created_at: i64,
    pub profile: AntigravityDeviceProfile,
}

pub fn generate_device_profile() -> AntigravityDeviceProfile {
    let raw_hex = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let machine_id = format!("auth0|user_{}", &raw_hex[..32]);
    let mac_machine_id = Uuid::new_v4().to_string();
    let dev_device_id = Uuid::new_v4().to_string();
    let sqm_id = format!("{{{}}}", Uuid::new_v4().to_string().to_ascii_uppercase());
    AntigravityDeviceProfile {
        machine_id,
        mac_machine_id,
        dev_device_id,
        sqm_id,
    }
}

fn pick_str(root: &Map<String, Value>, nested_key: &str, flat_key: &str) -> Option<String> {
    if let Some(val) = root
        .get("telemetry")
        .and_then(Value::as_object)
        .and_then(|tel| tel.get(nested_key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(val.to_string());
    }
    root.get(flat_key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// 从 `storage.json` 读取当前设备指纹（兼容嵌套 `telemetry.*` 与扁平 `"telemetry.*"` 键）。
pub fn read_device_profile_from_storage_json(
    storage_json_path: &Path,
) -> Option<AntigravityDeviceProfile> {
    let raw = std::fs::read_to_string(storage_json_path).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let obj = parsed.as_object()?;

    let machine_id = pick_str(obj, "machineId", "telemetry.machineId")?;
    let mac_machine_id = pick_str(obj, "macMachineId", "telemetry.macMachineId")
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let dev_device_id = pick_str(obj, "devDeviceId", "telemetry.devDeviceId")
        .or_else(|| {
            obj.get("storage.serviceMachineId")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let sqm_id = pick_str(obj, "sqmId", "telemetry.sqmId")
        .unwrap_or_else(|| format!("{{{}}}", Uuid::new_v4().to_string().to_ascii_uppercase()));

    Some(AntigravityDeviceProfile {
        machine_id,
        mac_machine_id,
        dev_device_id,
        sqm_id,
    })
}

/// 首次观测到本机真实 `storage.json` 指纹时保存一份不可覆盖的原始基线到 `~/.swivox/antigravity-device-original.json`。
pub fn ensure_device_baseline_saved(storage_json_path: &Path) -> Option<AntigravityDeviceProfile> {
    let baseline_path = crate::paths::default_antigravity_device_baseline_path();
    if let Ok(raw) = std::fs::read_to_string(&baseline_path)
        && let Ok(existing) = serde_json::from_str::<AntigravityDeviceProfile>(&raw)
    {
        return Some(existing);
    }
    let current = read_device_profile_from_storage_json(storage_json_path)?;
    if let Some(parent) = baseline_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(&current) {
        let _ = std::fs::write(&baseline_path, serialized);
    }
    Some(current)
}

pub fn load_device_baseline() -> Option<AntigravityDeviceProfile> {
    let baseline_path = crate::paths::default_antigravity_device_baseline_path();
    let raw = std::fs::read_to_string(baseline_path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 将设备指纹原子写入 `storage.json`（同时更新嵌套与扁平键以及 `storage.serviceMachineId`）。
pub fn write_device_profile_to_storage_json(
    storage_json_path: &Path,
    profile: &AntigravityDeviceProfile,
) -> anyhow::Result<()> {
    let _ = ensure_device_baseline_saved(storage_json_path);

    if let Some(parent) = storage_json_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }

    let mut root_val: Value = if storage_json_path.exists() {
        let raw = std::fs::read_to_string(storage_json_path).unwrap_or_default();
        serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    if !root_val.is_object() {
        root_val = json!({});
    }
    let obj = root_val.as_object_mut().expect("object");

    let mut tel_obj = obj
        .get("telemetry")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    tel_obj.insert(
        "machineId".to_string(),
        Value::String(profile.machine_id.clone()),
    );
    tel_obj.insert(
        "macMachineId".to_string(),
        Value::String(profile.mac_machine_id.clone()),
    );
    tel_obj.insert(
        "devDeviceId".to_string(),
        Value::String(profile.dev_device_id.clone()),
    );
    tel_obj.insert("sqmId".to_string(), Value::String(profile.sqm_id.clone()));
    obj.insert("telemetry".to_string(), Value::Object(tel_obj));

    obj.insert(
        "telemetry.machineId".to_string(),
        Value::String(profile.machine_id.clone()),
    );
    obj.insert(
        "telemetry.macMachineId".to_string(),
        Value::String(profile.mac_machine_id.clone()),
    );
    obj.insert(
        "telemetry.devDeviceId".to_string(),
        Value::String(profile.dev_device_id.clone()),
    );
    obj.insert(
        "telemetry.sqmId".to_string(),
        Value::String(profile.sqm_id.clone()),
    );
    obj.insert(
        "storage.serviceMachineId".to_string(),
        Value::String(profile.dev_device_id.clone()),
    );

    let tmp_path = storage_json_path.with_extension("json.swivox-tmp");
    let serialized = serde_json::to_string_pretty(&root_val)?;
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(serialized.as_bytes())?;
        let _ = f.sync_all();
    }
    std::fs::rename(&tmp_path, storage_json_path)?;
    Ok(())
}
