//! Antigravity `state.vscdb` 统一状态同步（UnifiedState Topic/Row）零依赖 Protobuf 编解码器。

use base64::{Engine as _, engine::general_purpose::STANDARD};

pub fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(10);
    while value >= 0x80 {
        buf.push(((value & 0x7F) | 0x80) as u8);
        value >>= 7;
    }
    buf.push(value as u8);
    buf
}

pub fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    let mut result = 0u64;
    let mut shift = 0u32;
    let mut pos = offset;
    loop {
        if pos >= data.len() {
            return Err("protobuf varint 截断".to_string());
        }
        if shift >= 64 {
            return Err("protobuf varint 溢出".to_string());
        }
        let byte = data[pos];
        result |= u64::from(byte & 0x7F) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok((result, pos))
}

pub fn skip_field(data: &[u8], offset: usize, wire_type: u8) -> Result<usize, String> {
    let next = match wire_type {
        0 => read_varint(data, offset)?.1,
        1 => offset.checked_add(8).ok_or("64-bit 字段偏移溢出")?,
        2 => {
            let (len, content_offset) = read_varint(data, offset)?;
            content_offset
                .checked_add(len as usize)
                .ok_or("length-delimited 字段偏移溢出")?
        }
        5 => offset.checked_add(4).ok_or("32-bit 字段偏移溢出")?,
        other => return Err(format!("未知 protobuf wire_type: {other}")),
    };
    if next > data.len() {
        return Err("protobuf 字段超出缓冲区边界".to_string());
    }
    Ok(next)
}

pub fn encode_len_delim_field(field_num: u32, payload: &[u8]) -> Vec<u8> {
    let tag = (u64::from(field_num) << 3) | 2;
    let mut out = encode_varint(tag);
    out.extend(encode_varint(payload.len() as u64));
    out.extend_from_slice(payload);
    out
}

pub fn encode_string_field(field_num: u32, value: &str) -> Vec<u8> {
    encode_len_delim_field(field_num, value.as_bytes())
}

pub fn encode_varint_field(field_num: u32, value: u64) -> Vec<u8> {
    let tag = u64::from(field_num) << 3;
    let mut out = encode_varint(tag);
    out.extend(encode_varint(value));
    out
}

pub fn find_field(data: &[u8], target_field: u32) -> Result<Option<Vec<u8>>, String> {
    let mut offset = 0;
    while offset < data.len() {
        let Ok((tag, next_offset)) = read_varint(data, offset) else {
            break;
        };
        let wire_type = (tag & 0x07) as u8;
        let field_num = (tag >> 3) as u32;
        if field_num == target_field && wire_type == 2 {
            let (len, content_offset) = read_varint(data, next_offset)?;
            let end = content_offset
                .checked_add(len as usize)
                .ok_or("length-delimited 长度溢出")?;
            if end > data.len() {
                return Err("length-delimited 内容截断".to_string());
            }
            return Ok(Some(data[content_offset..end].to_vec()));
        }
        offset = skip_field(data, next_offset, wire_type)?;
    }
    Ok(None)
}

pub fn find_string_field(data: &[u8], target_field: u32) -> Option<String> {
    find_field(data, target_field)
        .ok()
        .flatten()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

pub fn find_all_fields(data: &[u8], target_field: u32) -> Result<Vec<Vec<u8>>, String> {
    let mut items = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let Ok((tag, next_offset)) = read_varint(data, offset) else {
            break;
        };
        let wire_type = (tag & 0x07) as u8;
        let field_num = (tag >> 3) as u32;
        if field_num == target_field && wire_type == 2 {
            let (len, content_offset) = read_varint(data, next_offset)?;
            let end = content_offset
                .checked_add(len as usize)
                .ok_or("length-delimited 长度溢出")?;
            if end > data.len() {
                return Err("length-delimited 列表项截断".to_string());
            }
            items.push(data[content_offset..end].to_vec());
            offset = end;
        } else {
            offset = skip_field(data, next_offset, wire_type)?;
        }
    }
    Ok(items)
}

pub fn find_varint_field(data: &[u8], target_field: u32) -> Result<Option<u64>, String> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, next_offset) = read_varint(data, offset)?;
        let wire_type = (tag & 0x07) as u8;
        let field_num = (tag >> 3) as u32;
        if field_num == target_field && wire_type == 0 {
            let (val, _) = read_varint(data, next_offset)?;
            return Ok(Some(val));
        }
        offset = skip_field(data, next_offset, wire_type)?;
    }
    Ok(None)
}

fn is_personal_mailbox(email: &str) -> bool {
    let lower = email.trim().to_ascii_lowercase();
    [
        "@gmail.com",
        "@outlook.com",
        "@hotmail.com",
        "@qq.com",
        "@163.com",
        "@icloud.com",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

/// 构造 `OAuthTokenInfo` 内层 Protobuf 字节（个人邮箱强制省略 `is_gcp_tos` 以保持 IDE 刷新兼容）。
pub fn create_oauth_info(
    access_token: &str,
    refresh_token: &str,
    expiry_unix_secs: i64,
    mut is_gcp_tos: bool,
    id_token: Option<&str>,
    email: Option<&str>,
) -> Vec<u8> {
    if email.is_some_and(is_personal_mailbox) {
        is_gcp_tos = false;
    }

    let mut timestamp = encode_varint_field(1, expiry_unix_secs.max(0) as u64);
    timestamp.extend(encode_varint_field(2, 0));

    let mut out = Vec::new();
    out.extend(encode_string_field(1, access_token));
    out.extend(encode_string_field(2, "Bearer"));
    out.extend(encode_string_field(3, refresh_token));
    out.extend(encode_len_delim_field(4, &timestamp));
    if let Some(id_tok) = id_token.filter(|s| !s.trim().is_empty()) {
        out.extend(encode_string_field(5, id_tok));
    }
    if is_gcp_tos {
        out.extend(encode_varint_field(6, 1));
    }
    out
}

/// 构造最小可用 `UserStatus` 载荷（field 3 = email, field 7 = email）。
pub fn create_minimal_user_status_payload(email: &str) -> Vec<u8> {
    let mut out = encode_string_field(3, email);
    out.extend(encode_string_field(7, email));
    out
}

/// 构造 `EnterprisePreferences` 等 `stringValue` 载荷（field 3 = value）。
pub fn create_string_value_payload(value: &str) -> Vec<u8> {
    encode_string_field(3, value)
}

/// 构造单个 `Topic.data` map entry：
/// `Topic { repeated DataEntry data = 1 }`, `DataEntry { string key = 1; Row row = 2 }`, `Row { string base64_payload = 1 }`.
pub fn create_unified_topic_entry(sentinel_key: &str, inner_payload: &[u8]) -> Vec<u8> {
    let row = encode_string_field(1, &STANDARD.encode(inner_payload));
    let mut entry = encode_string_field(1, sentinel_key);
    entry.extend(encode_len_delim_field(2, &row));
    encode_len_delim_field(1, &entry)
}

/// 构造单条目完整 Base64 Topic 字符串。
pub fn create_unified_state_entry(sentinel_key: &str, inner_payload: &[u8]) -> String {
    STANDARD.encode(create_unified_topic_entry(sentinel_key, inner_payload))
}

fn topic_entry_key(entry: &[u8]) -> Option<&str> {
    let mut offset = 0;
    while offset < entry.len() {
        let (tag, next_offset) = read_varint(entry, offset).ok()?;
        let wire_type = (tag & 0x07) as u8;
        let field_num = (tag >> 3) as u32;
        if field_num == 1 && wire_type == 2 {
            let (len, content_offset) = read_varint(entry, next_offset).ok()?;
            let end = content_offset.checked_add(len as usize)?;
            return std::str::from_utf8(entry.get(content_offset..end)?).ok();
        }
        offset = skip_field(entry, next_offset, wire_type).ok()?;
    }
    None
}

/// 移除 `Topic.data` 中指定 `sentinel_key` 的条目，保留同 Topic 下的其他条目。
pub fn remove_unified_topic_entry(topic_blob: &[u8], target_key: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < topic_blob.len() {
        let start = offset;
        let (tag, next_offset) = read_varint(topic_blob, offset)?;
        let wire_type = (tag & 0x07) as u8;
        let field_num = (tag >> 3) as u32;
        let end = skip_field(topic_blob, next_offset, wire_type)?;
        let remove = if field_num == 1 && wire_type == 2 {
            let (len, content_offset) = read_varint(topic_blob, next_offset)?;
            let entry_end = content_offset
                .checked_add(len as usize)
                .ok_or("Topic entry 长度溢出")?;
            let entry = topic_blob
                .get(content_offset..entry_end)
                .ok_or("Topic entry 数据不完整")?;
            topic_entry_key(entry) == Some(target_key)
        } else {
            false
        };
        if !remove {
            out.extend_from_slice(&topic_blob[start..end]);
        }
        offset = end;
    }
    Ok(out)
}

/// 解码 `ItemTable` 中的 UnifiedState Base64 字符串，返回 `(sentinel_key, inner_payload_bytes)`。
pub fn decode_unified_state_entry_with_target(
    outer_b64: &str,
    preferred_key: Option<&str>,
) -> Result<(String, Vec<u8>), String> {
    let outer_blob = STANDARD
        .decode(outer_b64.trim())
        .map_err(|e| format!("外层 Base64 解码失败: {e}"))?;

    let entries = find_all_fields(&outer_blob, 1)?;
    let mut first_valid: Option<(String, Vec<u8>)> = None;
    for entry in entries {
        let Some(key_bytes) = find_field(&entry, 1)? else {
            continue;
        };
        let Ok(sentinel_key) = String::from_utf8(key_bytes) else {
            continue;
        };
        let Some(row_blob) = find_field(&entry, 2)? else {
            continue;
        };
        let payload = if let Some(b64_bytes) = find_field(&row_blob, 1)? {
            if let Ok(b64_str) = std::str::from_utf8(&b64_bytes) {
                STANDARD
                    .decode(b64_str.trim())
                    .unwrap_or_else(|_| b64_bytes)
            } else {
                b64_bytes
            }
        } else {
            row_blob
        };

        if preferred_key.is_some_and(|pref| pref == sentinel_key) {
            return Ok((sentinel_key, payload));
        }
        if first_valid.is_none() {
            first_valid = Some((sentinel_key, payload));
        }
    }

    first_valid.ok_or_else(|| "未找到有效的 UnifiedState Topic 条目".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_topic_round_trips_and_preserves_siblings() {
        let sibling = create_unified_topic_entry("otherSentinel", b"hello");
        let oauth = create_oauth_info(
            "ya29.access",
            "1//0g.refresh",
            1_800_000_000,
            true,
            Some("eyJ.id"),
            Some("dev@gmail.com"),
        );
        let mut combined = sibling;
        combined.extend(create_unified_topic_entry(
            "oauthTokenInfoSentinelKey",
            &oauth,
        ));
        let encoded = STANDARD.encode(&combined);

        let (key, decoded_oauth) =
            decode_unified_state_entry_with_target(&encoded, Some("oauthTokenInfoSentinelKey"))
                .expect("decode oauth");
        assert_eq!(key, "oauthTokenInfoSentinelKey");
        assert_eq!(
            find_string_field(&decoded_oauth, 1).as_deref(),
            Some("ya29.access")
        );
        assert_eq!(
            find_string_field(&decoded_oauth, 3).as_deref(),
            Some("1//0g.refresh")
        );
        // 个人邮箱自动清除 is_gcp_tos (field 6)
        assert_eq!(find_varint_field(&decoded_oauth, 6).unwrap(), None);

        // 移除 oauth 后仍保留 sibling
        let stripped =
            remove_unified_topic_entry(&combined, "oauthTokenInfoSentinelKey").expect("strip");
        let stripped_b64 = STANDARD.encode(&stripped);
        let (sib_key, sib_payload) =
            decode_unified_state_entry_with_target(&stripped_b64, Some("otherSentinel"))
                .expect("decode sibling");
        assert_eq!(sib_key, "otherSentinel");
        assert_eq!(sib_payload, b"hello");
    }
}
