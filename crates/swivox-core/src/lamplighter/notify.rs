//! Lamplighter 通知文案渲染、Bark 推送（RFC3986 `%20` 编码）、通用 Webhook（飞书 / 钉钉 / Telegram / ntfy / 自定义）与 Did Codex Reset 信号源。

use std::time::Duration;

use anyhow::{Context, bail};
use chrono::{Local, TimeZone};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use super::config::LamplighterConfig;
use super::store::CodexResetTrackState;

#[derive(Debug, Clone)]
pub struct OutboundNotification {
    pub kind: String,
    pub priority: &'static str, // "normal" | "high"
    pub title: String,
    pub body: String,
    pub jump_url: Option<String>,
}

/// RFC 3986 路径段百分号编码（空格严格编码为 `%20` 而非 `+`，避免 Bark 标题出现加号）。
pub fn pct_encode_rfc3986(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn compact_duration_label(remaining_secs: i64, english: bool) -> String {
    if remaining_secs <= 0 {
        return if english {
            "due".to_string()
        } else {
            "可刷新".to_string()
        };
    }
    if remaining_secs >= 86_400 {
        let days = ((remaining_secs as f64 / 86_400.0) + 0.5).floor() as i64;
        format!("{:02}d", days.max(1))
    } else if remaining_secs >= 3600 {
        let hours = ((remaining_secs as f64 / 3600.0) + 0.5).floor() as i64;
        format!("{:02}h", hours.max(1))
    } else {
        let mins = ((remaining_secs as f64 / 60.0) + 0.5).floor() as i64;
        format!("{:02}m", mins.max(1))
    }
}

pub fn format_local_mmdd_hhmm(epoch_secs: i64) -> String {
    Local
        .timestamp_opt(epoch_secs, 0)
        .single()
        .map(|dt| dt.format("%m/%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}

pub fn format_window_summary_line(
    short_win: &str,
    remaining_pct: f64,
    reset_at: Option<i64>,
    now: i64,
    english: bool,
) -> String {
    let pct = remaining_pct.round().clamp(0.0, 100.0) as i64;
    let sep = if english { ": " } else { "：" };
    if let Some(rst) = reset_at.filter(|v| *v > 0) {
        let dur = compact_duration_label(rst - now, english);
        let clock = format_local_mmdd_hhmm(rst);
        format!("{short_win}{sep}{pct}% | {dur} | {clock}")
    } else {
        format!("{short_win}{sep}{pct}%")
    }
}

fn json_escape_inner(raw: &str) -> String {
    // 只剥掉 serde_json 输出最外层的一对引号；trim_matches 会把内容末尾转义后的 `\"` 一起吃掉，
    // 留下悬空反斜杠破坏模板 JSON。
    let quoted = serde_json::to_string(raw).unwrap_or_else(|_| "\"\"".to_string());
    quoted
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or("")
        .to_string()
}

fn render_webhook_template(tpl: &str, msg: &OutboundNotification) -> String {
    let combined = if msg.body.is_empty() {
        msg.title.clone()
    } else {
        format!("{}\n{}", msg.title, msg.body)
    };
    let jump = msg.jump_url.as_deref().unwrap_or("");
    tpl.replace("{{title}}", &json_escape_inner(&msg.title))
        .replace("{{body}}", &json_escape_inner(&msg.body))
        .replace("{{text}}", &json_escape_inner(&combined))
        .replace("{{url}}", &json_escape_inner(jump))
        .replace("{{priority}}", msg.priority)
        .replace("{{kind}}", &json_escape_inner(&msg.kind))
}

/// 并发投递 Bark 与通用 Webhook 通知。
pub async fn send_notification(
    cfg: &LamplighterConfig,
    msg: &OutboundNotification,
) -> anyhow::Result<()> {
    let has_bark = !cfg.bark_url.trim().is_empty();
    let has_webhook = !cfg.webhook.url.trim().is_empty();
    if !has_bark && !has_webhook {
        return Ok(());
    }

    let client = crate::http_client::proxied_client_with_timeout(
        "Swivox-Lamplighter/1.0",
        Duration::from_secs(cfg.request_timeout_seconds.clamp(5, 60)),
    )?;

    let mut errors = Vec::new();
    let mut any_ok = false;

    if has_bark {
        match send_bark(&client, cfg, msg).await {
            Ok(()) => any_ok = true,
            Err(e) => errors.push(format!("Bark 推送失败: {e}")),
        }
    }

    if has_webhook {
        match send_webhook(&client, cfg, msg).await {
            Ok(()) => any_ok = true,
            Err(e) => errors.push(format!("Webhook 推送失败: {e}")),
        }
    }

    if any_ok || errors.is_empty() {
        Ok(())
    } else {
        bail!("{}", errors.join("；"))
    }
}

async fn send_bark(
    client: &reqwest::Client,
    cfg: &LamplighterConfig,
    msg: &OutboundNotification,
) -> anyhow::Result<()> {
    let base = cfg.bark_url.trim().trim_end_matches('/');
    let encoded_title = pct_encode_rfc3986(&msg.title);
    let encoded_body = pct_encode_rfc3986(if msg.body.is_empty() { " " } else { &msg.body });
    let raw_url = format!("{base}/{encoded_title}/{encoded_body}");
    let mut url = Url::parse(&raw_url).context("Bark 地址格式无效")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("group", cfg.bark_group.trim());
        qp.append_pair(
            "level",
            if msg.priority == "high" {
                "timeSensitive"
            } else {
                "active"
            },
        );
        if !cfg.bark_icon.trim().is_empty() {
            qp.append_pair("icon", cfg.bark_icon.trim());
        }
        if let Some(ref jump) = msg.jump_url
            && !jump.trim().is_empty()
        {
            qp.append_pair("url", jump.trim());
        }
    }

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("网络连接异常（已隐去设备密钥）"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("HTTP {}", status.as_u16());
    }
    if let Ok(val) = serde_json::from_str::<Value>(&body)
        && let Some(code) = val.get("code").and_then(Value::as_i64)
        && code != 200
    {
        let m = val
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Bark 返回非 200 状态码");
        bail!("code={code}: {m}");
    }
    Ok(())
}

async fn send_webhook(
    client: &reqwest::Client,
    cfg: &LamplighterConfig,
    msg: &OutboundNotification,
) -> anyhow::Result<()> {
    let wh = &cfg.webhook;
    let url = wh.url.trim();
    let combined = if msg.body.is_empty() {
        msg.title.clone()
    } else {
        format!("{}\n\n{}", msg.title, msg.body)
    };

    if wh.preset.trim().eq_ignore_ascii_case("ntfy") {
        let mut req = client
            .post(url)
            .header("Title", &msg.title)
            .header(
                "Priority",
                if msg.priority == "high" { "high" } else { "default" },
            )
            .body(msg.body.clone());
        if let Some(ref jump) = msg.jump_url {
            req = req.header("Click", jump);
        }
        for (k, v) in &wh.headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.context("请求 ntfy Webhook 失败")?;
        if !resp.status().is_success() {
            bail!("HTTP {}", resp.status().as_u16());
        }
        return Ok(());
    }

    let payload_str = if !wh.body_template.trim().is_empty() {
        render_webhook_template(&wh.body_template, msg)
    } else {
        match wh.preset.trim().to_ascii_lowercase().as_str() {
            "dingtalk" => json!({
                "msgtype": "text",
                "text": { "content": combined }
            })
            .to_string(),
            "telegram" => json!({
                "text": combined
            })
            .to_string(),
            _ => json!({
                "msg_type": "text",
                "content": { "text": combined }
            })
            .to_string(),
        }
    };

    let method = match wh.method.as_str() {
        "GET" => reqwest::Method::GET,
        "PUT" => reqwest::Method::PUT,
        _ => reqwest::Method::POST,
    };

    let mut req = client.request(method.clone(), url);
    if method != reqwest::Method::GET {
        req = req
            .header("Content-Type", "application/json")
            .body(payload_str);
    }
    for (k, v) in &wh.headers {
        req = req.header(k, v);
    }

    let resp = req.send().await.context("请求通用 Webhook 失败")?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("HTTP {}", status.as_u16());
    }
    if let Ok(val) = serde_json::from_str::<Value>(&text) {
        // 校验飞书/钉钉常见错误码
        if let Some(errcode) = val
            .get("code")
            .or_else(|| val.get("errcode"))
            .and_then(Value::as_i64)
            && errcode != 0
            && errcode != 200
        {
            let m = val
                .get("msg")
                .or_else(|| val.get("errmsg"))
                .and_then(Value::as_str)
                .unwrap_or("Webhook 业务返回错误");
            bail!("errcode={errcode}: {m}");
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DidCodexResetSignal {
    #[serde(default, rename = "announcedAt")]
    announced_at: i64,
    #[serde(default)]
    status: String,
    #[serde(default, rename = "scheduledFor")]
    scheduled_for: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    scope: Option<String>,
}

/// 轮询 `didcodexreset.com` 公开信号并在出现新排期或排期完成时生成提醒。
pub async fn poll_did_codex_reset(
    cfg: &LamplighterConfig,
    track: &mut CodexResetTrackState,
) -> Vec<OutboundNotification> {
    if !cfg.codex_reset_updates.enabled {
        return Vec::new();
    }
    let Ok(client) = crate::http_client::proxied_client_with_timeout(
        "Swivox-Lamplighter/1.0",
        Duration::from_secs(15),
    ) else {
        return Vec::new();
    };
    let Ok(resp) = client
        .get("https://didcodexreset.com/api/signals.json")
        .send()
        .await
    else {
        return Vec::new();
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let Ok(signals) = resp.json::<Vec<DidCodexResetSignal>>().await else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let first_run = !track.initialized;
    track.initialized = true;

    for sig in signals {
        if sig.announced_at <= 0 {
            continue;
        }
        let id = sig.announced_at.to_string();
        let fingerprint = format!(
            "{}|{:?}|{:?}|{:?}",
            sig.status, sig.scheduled_for, sig.confidence, sig.scope
        );
        let prev = track.seen_announcements.insert(id.clone(), fingerprint.clone());
        if prev.as_deref() == Some(fingerprint.as_str()) {
            continue;
        }
        if first_run && !cfg.codex_reset_updates.notify_current_pending {
            continue;
        }
        let title = if sig.status == "completed" {
            "✅ Codex 全局额度重置已完成".to_string()
        } else if prev.is_some() {
            "📅 Codex 额度重置排期已更新".to_string()
        } else {
            "📅 Codex 额度重置已排期".to_string()
        };
        let mut lines = Vec::new();
        if let Some(when) = sig.scheduled_for {
            lines.push(format!("预计：{when}"));
        }
        if let Some(conf) = sig.confidence {
            let pct = if conf <= 1.0 { conf * 100.0 } else { conf };
            lines.push(format!("置信度：{:.0}%", pct));
        }
        if let Some(scope) = sig.scope {
            lines.push(format!("范围：{scope}"));
        }
        out.push(OutboundNotification {
            kind: "codex_reset".to_string(),
            priority: "normal",
            title,
            body: lines.join("\n"),
            jump_url: Some(format!("https://didcodexreset.com/zh/history/{id}.html")),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_spaces_as_percent_20_for_bark() {
        assert_eq!(
            pct_encode_rfc3986("🕯️ GPT#主力 · 5h 48%"),
            "%F0%9F%95%AF%EF%B8%8F%20GPT%23%E4%B8%BB%E5%8A%9B%20%C2%B7%205h%2048%25"
        );
    }
}
