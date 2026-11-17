//! Lamplighter 本地无状态 Streamable HTTP `/mcp` 配额查询服务（默认 `127.0.0.1:58318/mcp`，常量时间 SHA-256 Bearer 鉴权 + 邮箱强制脱敏）。

use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

static MCP_STOP_TX: OnceLock<Mutex<Option<(String, watch::Sender<bool>)>>> = OnceLock::new();

fn constant_time_bearer_eq(got_auth_header: &str, expected_key: &str) -> bool {
    if expected_key.trim().is_empty() {
        return false;
    }
    let expected = format!("Bearer {}", expected_key.trim());
    let h1 = Sha256::digest(got_auth_header.trim().as_bytes());
    let h2 = Sha256::digest(expected.as_bytes());
    let mut diff = 0u8;
    for (a, b) in h1.iter().zip(h2.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn origin_is_safe_loopback(origin: Option<&str>) -> bool {
    let Some(raw) = origin.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let Ok(u) = url::Url::parse(raw) else {
        return false;
    };
    matches!(u.scheme(), "http" | "https")
        && matches!(
            u.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
        )
}

/// 根据最新配置动态启停或热更新本地 `/mcp` HTTP 监听。
pub fn sync_mcp_server(handle: &tokio::runtime::Handle, enabled: bool, listen_addr: &str, mcp_key: &str) {
    let slot = MCP_STOP_TX.get_or_init(|| Mutex::new(None));
    let mut guard = match slot.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    let want_sig = format!("{enabled}|{listen_addr}|{mcp_key}");
    if let Some((ref cur_sig, _)) = *guard
        && cur_sig == &want_sig
    {
        return;
    }
    if let Some((_, prev_stop)) = guard.take() {
        let _ = prev_stop.send(true);
    }
    if !enabled || listen_addr.trim().is_empty() || mcp_key.trim().is_empty() {
        return;
    }

    let addr = listen_addr.trim().to_string();
    let key = mcp_key.trim().to_string();
    let (stop_tx, mut stop_rx) = watch::channel(false);
    *guard = Some((want_sig, stop_tx));

    handle.spawn(async move {
        let Ok(listener) = TcpListener::bind(&addr).await else {
            let _ = crate::diagnostic_log::append_diagnostic_log(
                "lamplighter.mcp.bind_failed",
                json!({ "listen": addr }),
            );
            return;
        };

        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    let Ok((mut stream, _)) = accepted else { continue };
                    let expected_key = key.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 65_536];
                        let n = stream.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        let raw = String::from_utf8_lossy(&buf[..n]);
                        let mut lines = raw.lines();
                        let req_line = lines.next().unwrap_or("");
                        let mut parts = req_line.split_whitespace();
                        let method = parts.next().unwrap_or("");
                        let path = parts.next().unwrap_or("");

                        let mut auth_header = None;
                        let mut origin_header = None;
                        for line in lines {
                            if line.is_empty() {
                                break;
                            }
                            if let Some((k, v)) = line.split_once(':') {
                                if k.eq_ignore_ascii_case("authorization") {
                                    auth_header = Some(v.trim().to_string());
                                } else if k.eq_ignore_ascii_case("origin") {
                                    origin_header = Some(v.trim().to_string());
                                }
                            }
                        }

                        if path != "/mcp" {
                            let _ = write_http_json(&mut stream, 404, "Not Found", &json!({"error": "not found"})).await;
                            return;
                        }
                        if method != "POST" {
                            let _ = write_http_json(&mut stream, 405, "Method Not Allowed", &json!({"error": "POST required"})).await;
                            return;
                        }
                        if !origin_is_safe_loopback(origin_header.as_deref()) {
                            let _ = write_http_json(&mut stream, 403, "Forbidden", &json!({"error": "forbidden origin"})).await;
                            return;
                        }
                        if !constant_time_bearer_eq(auth_header.as_deref().unwrap_or(""), &expected_key) {
                            let resp = "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer\r\nContent-Type: application/json\r\nContent-Length: 26\r\nConnection: close\r\n\r\n{\"error\":\"unauthorized\"}";
                            let _ = stream.write_all(resp.as_bytes()).await;
                            return;
                        }

                        let body_slice = raw.split("\r\n\r\n").nth(1).unwrap_or("{}");
                        let reply = handle_mcp_jsonrpc(body_slice).await;
                        let _ = write_http_json(&mut stream, 200, "OK", &reply).await;
                    });
                }
            }
        }
    });
}

async fn write_http_json(
    stream: &mut tokio::net::TcpStream,
    code: u16,
    reason: &str,
    payload: &Value,
) -> std::io::Result<()> {
    let body = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(header.as_bytes()).await
}

async fn handle_mcp_jsonrpc(raw_body: &str) -> Value {
    let Ok(req) = serde_json::from_str::<Value>(raw_body) else {
        return json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": { "code": -32700, "message": "Parse error" }
        });
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");

    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2025-03-26",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "swivox-lamplighter",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "list_quotas",
                        "description": "查询当前监控的所有已支持客户端（Codex 与 Antigravity）账号脱敏额度与下次自动点火计划",
                        "inputSchema": { "type": "object", "properties": {} }
                    },
                    {
                        "name": "get_quota_events",
                        "description": "获取最近额度重置、耗尽、自动点火与告警事件",
                        "inputSchema": { "type": "object", "properties": {} }
                    },
                    {
                        "name": "refresh_quotas",
                        "description": "触发一次全量额度刷新（限频 60 秒 1 次）",
                        "inputSchema": { "type": "object", "properties": {} }
                    }
                ]
            }
        }),
        "tools/call" => {
            let tool_name = req
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let content_json = match tool_name {
                "list_quotas" => {
                    let snap = super::get_lamplighter_snapshot();
                    // 严格脱敏：移除 email 字段，仅暴露 redacted_account（如 ***rk）
                    let groups: Vec<Value> = snap
                        .groups
                        .iter()
                        .map(|g| {
                            json!({
                                "provider": g.provider,
                                "account": format!("***{}", super::providers::email_last_two_chars(&g.account_email)),
                                "label": g.display_label,
                                "plan": g.plan_name,
                                "weight": g.weight,
                                "windows": g.windows,
                                "nextIgnitionAt": g.next_ignition_at,
                            })
                        })
                        .collect();
                    json!({ "groups": groups })
                }
                "get_quota_events" => {
                    let snap = super::get_lamplighter_snapshot();
                    json!({ "events": snap.events })
                }
                "refresh_quotas" => {
                    let res = super::refresh_all_now().await;
                    json!({ "ok": res.is_ok() })
                }
                _ => json!({ "error": "unknown tool" }),
            };
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&content_json).unwrap_or_default()
                    }]
                }
            })
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "Method not found" }
        }),
    }
}
