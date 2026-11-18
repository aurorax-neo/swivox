//! Antigravity Google OAuth 登录（自动双栈回环监听 + 手动 URL/Code 回调解析）与 Token 自动刷新。

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;
use uuid::Uuid;

pub const ANTIGRAVITY_OAUTH_CLIENT_ID: &str =
    "";
pub const ANTIGRAVITY_OAUTH_CLIENT_SECRET: &str =
    "";

pub const ANTIGRAVITY_GCP_CLIENT_ID: &str =
    "";
pub const ANTIGRAVITY_GCP_CLIENT_SECRET: &str =
    "";

pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
pub const TOKEN_REFRESH_SKEW_SECONDS: i64 = 900;
pub const OAUTH_USER_AGENT: &str = "vscode/1.X.X (Antigravity/4.3.0)";

/// 与 Antigravity 官方客户端（Antigravity.app 2.32.x `SIGN_IN_CLIENTS` / language_server）逐项对齐：
/// 不请求 `openid`、不带 `include_granted_scopes`，个人账号客户端额外带 `aicode`。
pub const ANTIGRAVITY_OAUTH_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
];
/// 个人账号客户端（1071006060591-…）在通用 scope 之外额外请求的 scope。
pub const ANTIGRAVITY_PERSONAL_EXTRA_SCOPE: &str = "https://www.googleapis.com/auth/aicode";
/// 官方客户端的本地回环回调路径（`SIGN_IN_CALLBACK_PATH`）。
pub const ANTIGRAVITY_OAUTH_CALLBACK_PATH: &str = "/auth/callback";

/// 按客户端返回官方完全一致的 scope 列表（顺序与官方一致，单空格拼接）。
pub fn oauth_scopes_for_client(client_key: &str) -> Vec<&'static str> {
    let mut scopes = ANTIGRAVITY_OAUTH_SCOPES.to_vec();
    if client_key != "gcp" {
        scopes.push(ANTIGRAVITY_PERSONAL_EXTRA_SCOPE);
    }
    scopes
}

/// 兼容旧版 Swivox 发出的 `/oauth-callback` 回调（手动粘贴时可能仍是旧地址）。
fn is_oauth_callback_path(path: &str) -> bool {
    path.starts_with(ANTIGRAVITY_OAUTH_CALLBACK_PATH) || path.starts_with("/oauth-callback")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityOAuthStartResponse {
    pub login_id: String,
    pub verification_uri: String,
    pub callback_url: String,
    pub client_key: String,
}

#[derive(Debug, Clone)]
pub struct ExchangedGoogleTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub expiry_timestamp: i64,
    pub id_token: Option<String>,
    pub client_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleUserInfo {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
}

impl GoogleUserInfo {
    pub fn display_name(&self) -> Option<String> {
        if let Some(n) = self.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            return Some(n.to_string());
        }
        let joined = [
            self.given_name.as_deref().unwrap_or("").trim(),
            self.family_name.as_deref().unwrap_or("").trim(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        (!joined.is_empty()).then_some(joined)
    }
}

struct PendingOAuthSession {
    login_id: String,
    state: String,
    redirect_uri: String,
    client_key: String,
    code_verifier: String,
    code_rx: Option<oneshot::Receiver<Result<String, String>>>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

static PENDING_AG_OAUTH: OnceLock<Mutex<Option<PendingOAuthSession>>> = OnceLock::new();

fn pending_slot() -> &'static Mutex<Option<PendingOAuthSession>> {
    PENDING_AG_OAUTH.get_or_init(|| Mutex::new(None))
}

pub fn client_credentials_for(client_key: Option<&str>) -> (&'static str, &'static str, &'static str) {
    match client_key.unwrap_or("personal").trim() {
        "gcp" | "antigravity_gcp" | "enterprise" => (
            "gcp",
            ANTIGRAVITY_GCP_CLIENT_ID,
            ANTIGRAVITY_GCP_CLIENT_SECRET,
        ),
        _ => (
            "personal",
            ANTIGRAVITY_OAUTH_CLIENT_ID,
            ANTIGRAVITY_OAUTH_CLIENT_SECRET,
        ),
    }
}

fn alternate_client_credentials(primary_key: &str) -> (&'static str, &'static str, &'static str) {
    if primary_key == "gcp" {
        (
            "personal",
            ANTIGRAVITY_OAUTH_CLIENT_ID,
            ANTIGRAVITY_OAUTH_CLIENT_SECRET,
        )
    } else {
        (
            "gcp",
            ANTIGRAVITY_GCP_CLIENT_ID,
            ANTIGRAVITY_GCP_CLIENT_SECRET,
        )
    }
}

pub fn build_http_client_for_proxy(proxy_spec: Option<&str>, timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(5)));
    let fallback_spec = if proxy_spec.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        crate::network_proxies::resolve_client_proxy_spec("antigravity_desktop")
    } else {
        None
    };
    let effective = proxy_spec
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(fallback_spec.as_deref());
    match effective {
        Some(spec) if spec.eq_ignore_ascii_case("direct") => {
            builder = builder.no_proxy();
        }
        Some(spec) => {
            let proxy = reqwest::Proxy::all(spec)
                .with_context(|| format!("无效的代理地址: {spec}"))?
                .no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"));
            builder = builder.proxy(proxy);
        }
        None => {
            builder = builder.no_proxy();
        }
    }
    Ok(builder.user_agent(OAUTH_USER_AGENT).build()?)
}

/// 解析用户手动粘贴的完整回调 URL、查询串或纯授权码（支持 URL 编码的 `4%2F0A...`）。
pub fn parse_manual_oauth_input(raw_input: &str, expected_state: Option<&str>) -> anyhow::Result<String> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        bail!("请输入回调链接或授权码。");
    }

    let as_url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Url::parse(trimmed).ok()
    } else if trimmed.starts_with("localhost:")
        || trimmed.starts_with("127.0.0.1:")
        || trimmed.starts_with("[::1]:")
    {
        Url::parse(&format!("http://{trimmed}")).ok()
    } else if is_oauth_callback_path(trimmed) || trimmed.starts_with('?') {
        let prefix = if trimmed.starts_with('?') {
            format!("http://localhost{ANTIGRAVITY_OAUTH_CALLBACK_PATH}{trimmed}")
        } else {
            format!("http://localhost{trimmed}")
        };
        Url::parse(&prefix).ok()
    } else {
        None
    };

    if let Some(url) = as_url {
        if let Some((_, err)) = url.query_pairs().find(|(k, _)| k == "error") {
            bail!("Google 授权返回错误: {err}");
        }
        if let Some(exp) = expected_state.filter(|s| !s.is_empty())
            && let Some((_, got_state)) = url.query_pairs().find(|(k, _)| k == "state")
            && got_state != exp
        {
            bail!("OAuth state 校验不匹配，请重新发起登录。");
        }
        if let Some((_, code)) = url.query_pairs().find(|(k, _)| k == "code") {
            let code_trimmed = code.trim();
            if !code_trimmed.is_empty() {
                return Ok(code_trimmed.to_string());
            }
        }
        bail!("回调地址中未找到有效的 code 参数。");
    }

    // 纯 code 字符串（兼容 4%2F0A...）
    let decoded = url::form_urlencoded::parse(format!("code={trimmed}").as_bytes())
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| trimmed.to_string());
    Ok(decoded.trim().to_string())
}

fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn callback_state_matches(path: &str, expected_state: &str) -> bool {
    let Ok(url) = Url::parse(&format!("http://localhost{path}")) else {
        return false;
    };
    !expected_state.is_empty()
        && url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .is_some_and(|(_, value)| value == expected_state)
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn cancel_pending_oauth_session() {
    if let Ok(mut guard) = pending_slot().lock()
        && let Some(mut session) = guard.take()
        && let Some(cancel) = session.cancel_tx.take()
    {
        let _ = cancel.send(());
    }
}

/// 启动本地双栈回环 OAuth 监听并生成 Google 授权链接。
pub async fn start_oauth_login(client_key: Option<&str>) -> anyhow::Result<AntigravityOAuthStartResponse> {
    cancel_pending_oauth_session();

    let (normalized_key, client_id, _) = client_credentials_for(client_key);

    // 尝试双栈绑定，使 localhost 在 IPv4 与 IPv6 下均可命中
    let v4_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("绑定本地 OAuth 回环端口失败")?;
    let port = v4_listener.local_addr()?.port();
    let v6_listener = TcpListener::bind(format!("[::1]:{port}")).await.ok();

    let redirect_uri = if v6_listener.is_some() {
        format!("http://localhost:{port}{ANTIGRAVITY_OAUTH_CALLBACK_PATH}")
    } else {
        format!("http://127.0.0.1:{port}{ANTIGRAVITY_OAUTH_CALLBACK_PATH}")
    };

    let state = Uuid::new_v4().to_string();
    let login_id = Uuid::new_v4().to_string();
    // PKCE（RFC 7636，S256）：即使授权码在回调环节被截获，也无法脱离本进程换取令牌。
    let code_verifier = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let code_challenge = pkce_challenge(&code_verifier);

    let mut auth_url = Url::parse(GOOGLE_AUTH_URL)?;
    // 参数集合与官方 golang.org/x/oauth2 AuthCodeURL（含 PKCE）一致：
    // access_type / client_id / code_challenge / code_challenge_method / prompt / redirect_uri /
    // response_type / scope / state，不多不少。
    auth_url
        .query_pairs_mut()
        .append_pair("access_type", "offline")
        .append_pair("client_id", client_id)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "consent")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &oauth_scopes_for_client(normalized_key).join(" "))
        .append_pair("state", &state);

    let (code_tx, code_rx) = oneshot::channel::<Result<String, String>>();
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    let expected_state = state.clone();

    tokio::spawn(async move {
        let accept_once = async {
            loop {
                let accepted = if let Some(ref v6) = v6_listener {
                    tokio::select! {
                        r = v4_listener.accept() => r.ok().map(|(s, _)| s),
                        r = v6.accept() => r.ok().map(|(s, _)| s),
                    }
                } else {
                    v4_listener.accept().await.ok().map(|(s, _)| s)
                };
                let Some(mut stream) = accepted else {
                    break Err("回环监听器已关闭".to_string());
                };
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let req_str = String::from_utf8_lossy(&buf[..n]);
                let first_line = req_str.lines().next().unwrap_or("");
                let path = first_line.split_whitespace().nth(1).unwrap_or("/");
                if !is_oauth_callback_path(path) {
                    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    continue;
                }
                // 回环回调必须携带与发起时一致的 state；缺失即拒绝（防登录 CSRF）。
                // 校验失败不终止监听，避免本机其他网页抢先打一枪就把正常登录搅黄。
                if !callback_state_matches(path, &expected_state) {
                    let html = "<!doctype html><html><head><meta charset='utf-8'><title>授权失败</title></head><body><h3>Antigravity 授权失败</h3><p>OAuth state 缺失或不匹配，请从 Swivox 重新发起登录。</p></body></html>";
                    let resp = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        html.len(),
                        html
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                    continue;
                }
                match parse_manual_oauth_input(path, Some(&expected_state)) {
                    Ok(code) => {
                        let html = "<!doctype html><html><head><meta charset='utf-8'><title>Swivox - Antigravity 授权成功</title><style>body{background:#0b0f19;color:#f3f4f6;font-family:-apple-system,BlinkMacSystemFont,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}.card{background:#131a2a;border:1px solid #26324d;border-radius:14px;padding:32px 40px;text-align:center;box-shadow:0 16px 40px rgba(0,0,0,.4)}h2{margin:0 0 8px;color:#38bdf8}p{margin:0;color:#9ca3af;font-size:14px}</style></head><body><div class='card'><h2>Antigravity 登录成功</h2><p>您可以关闭此浏览器标签页并返回 Swivox。</p></div></body></html>";
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            html.len(),
                            html
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                        let _ = stream.flush().await;
                        break Ok(code);
                    }
                    Err(err) => {
                        let html = format!(
                            "<!doctype html><html><head><meta charset='utf-8'><title>授权失败</title></head><body><h3>Antigravity 授权失败</h3><p>{}</p></body></html>",
                            html_escape(&err.to_string())
                        );
                        let resp = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            html.len(),
                            html
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                        break Err(err.to_string());
                    }
                }
            }
        };

        let outcome = tokio::select! {
            res = accept_once => res,
            _ = &mut cancel_rx => return,
            _ = tokio::time::sleep(Duration::from_secs(300)) => Err("等待浏览器授权超时（5 分钟）。".to_string()),
        };
        let _ = code_tx.send(outcome);
    });

    if let Ok(mut guard) = pending_slot().lock() {
        *guard = Some(PendingOAuthSession {
            login_id: login_id.clone(),
            state,
            redirect_uri: redirect_uri.clone(),
            client_key: normalized_key.to_string(),
            code_verifier,
            code_rx: Some(code_rx),
            cancel_tx: Some(cancel_tx),
        });
    }

    Ok(AntigravityOAuthStartResponse {
        login_id,
        verification_uri: auth_url.to_string(),
        callback_url: redirect_uri,
        client_key: normalized_key.to_string(),
    })
}

pub async fn wait_for_oauth_callback(
    login_id: &str,
    proxy_spec: Option<&str>,
) -> anyhow::Result<(ExchangedGoogleTokens, GoogleUserInfo)> {
    let (rx, redirect_uri, client_key, code_verifier) = {
        let mut guard = pending_slot()
            .lock()
            .map_err(|_| anyhow::anyhow!("OAuth 会话锁异常"))?;
        let Some(session) = guard.as_mut() else {
            bail!("当前没有进行中的 Antigravity OAuth 登录会话。");
        };
        if session.login_id != login_id {
            bail!("OAuth 登录会话已过期或被替换。");
        }
        let Some(rx) = session.code_rx.take() else {
            bail!("正在等待该 OAuth 会话完成，请勿重复提交。");
        };
        (
            rx,
            session.redirect_uri.clone(),
            session.client_key.clone(),
            session.code_verifier.clone(),
        )
    };

    let code = rx
        .await
        .map_err(|_| anyhow::anyhow!("OAuth 登录已取消。"))?
        .map_err(|e| anyhow::anyhow!(e))?;

    cancel_pending_oauth_session();
    let tokens = exchange_authorization_code_with_verifier(
        &code,
        &redirect_uri,
        Some(&client_key),
        proxy_spec,
        Some(&code_verifier),
    )
    .await?;
    let user = fetch_google_userinfo(&tokens.access_token, proxy_spec).await?;
    Ok((tokens, user))
}

pub async fn complete_manual_oauth_callback(
    raw_input: &str,
    proxy_spec: Option<&str>,
) -> anyhow::Result<(ExchangedGoogleTokens, GoogleUserInfo)> {
    let (expected_state, redirect_uri, client_key, code_verifier) = {
        let guard = pending_slot()
            .lock()
            .map_err(|_| anyhow::anyhow!("OAuth 会话锁异常"))?;
        let Some(session) = guard.as_ref() else {
            bail!("请先点击「OAuth 浏览器登录」生成授权会话后再粘贴回调地址。");
        };
        (
            session.state.clone(),
            session.redirect_uri.clone(),
            session.client_key.clone(),
            session.code_verifier.clone(),
        )
    };

    let code = parse_manual_oauth_input(raw_input, Some(&expected_state))?;
    cancel_pending_oauth_session();
    let tokens = exchange_authorization_code_with_verifier(
        &code,
        &redirect_uri,
        Some(&client_key),
        proxy_spec,
        Some(&code_verifier),
    )
    .await?;
    let user = fetch_google_userinfo(&tokens.access_token, proxy_spec).await?;
    Ok((tokens, user))
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

pub async fn exchange_authorization_code(
    code: &str,
    redirect_uri: &str,
    client_key: Option<&str>,
    proxy_spec: Option<&str>,
) -> anyhow::Result<ExchangedGoogleTokens> {
    exchange_authorization_code_with_verifier(code, redirect_uri, client_key, proxy_spec, None).await
}

async fn exchange_authorization_code_with_verifier(
    code: &str,
    redirect_uri: &str,
    client_key: Option<&str>,
    proxy_spec: Option<&str>,
    code_verifier: Option<&str>,
) -> anyhow::Result<ExchangedGoogleTokens> {
    let (norm_key, client_id, client_secret) = client_credentials_for(client_key);
    let client = build_http_client_for_proxy(proxy_spec, 25)?;
    let mut form = vec![
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    if let Some(verifier) = code_verifier.filter(|value| !value.is_empty()) {
        form.push(("code_verifier", verifier));
    }
    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .header("User-Agent", OAUTH_USER_AGENT)
        .form(&form)
        .send()
        .await
        .context("请求 Google OAuth Token 接口失败")?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("Google OAuth 换取令牌失败 (HTTP {}): {}", status.as_u16(), text);
    }
    let parsed: TokenEndpointResponse =
        serde_json::from_str(&text).context("解析 Google Token 响应失败")?;
    let refresh_token = parsed
        .refresh_token
        .filter(|s| !s.trim().is_empty())
        .context("Google OAuth 未返回 refresh_token，请重试并确认授予离线访问权限")?;
    let expires_in = parsed.expires_in.unwrap_or(3599).max(60);
    let expiry_timestamp = Utc::now().timestamp() + expires_in;
    Ok(ExchangedGoogleTokens {
        access_token: parsed.access_token,
        refresh_token,
        expires_in,
        expiry_timestamp,
        id_token: parsed.id_token,
        client_key: norm_key.to_string(),
    })
}

/// 使用 `refresh_token` 换取新的 `access_token`（带瞬时 `invalid_grant` 500ms 重试确认与个人/GCP 双客户端回退）。
pub async fn refresh_google_access_token(
    refresh_token: &str,
    preferred_client_key: Option<&str>,
    proxy_spec: Option<&str>,
) -> anyhow::Result<ExchangedGoogleTokens> {
    let (primary_key, primary_id, primary_secret) = client_credentials_for(preferred_client_key);
    let (alt_key, alt_id, alt_secret) = alternate_client_credentials(primary_key);
    let client = build_http_client_for_proxy(proxy_spec, 25)?;

    let candidates = [
        (primary_key, primary_id, primary_secret),
        (alt_key, alt_id, alt_secret),
    ];

    let mut last_err = String::new();
    for (ckey, cid, csecret) in candidates {
        for attempt in 0..2 {
            let resp = client
                .post(GOOGLE_TOKEN_URL)
                .header("User-Agent", OAUTH_USER_AGENT)
                .form(&[
                    ("client_id", cid),
                    ("client_secret", csecret),
                    ("refresh_token", refresh_token.trim()),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await
                .context("请求 Google Token 刷新接口失败")?;

            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let parsed: TokenEndpointResponse =
                    serde_json::from_str(&body).context("解析刷新后的 Google Token 失败")?;
                let expires_in = parsed.expires_in.unwrap_or(3599).max(60);
                return Ok(ExchangedGoogleTokens {
                    access_token: parsed.access_token,
                    refresh_token: parsed
                        .refresh_token
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| refresh_token.trim().to_string()),
                    expires_in,
                    expiry_timestamp: Utc::now().timestamp() + expires_in,
                    id_token: parsed.id_token,
                    client_key: ckey.to_string(),
                });
            }

            last_err = format!("HTTP {}: {}", status.as_u16(), body);
            let lower = body.to_ascii_lowercase();
            if attempt == 0 && lower.contains("invalid_grant") {
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            // 若是客户端不匹配（unauthorized_client / invalid_client / invalid_grant），尝试备用内置客户端
            break;
        }
    }

    bail!("刷新 Antigravity Token 失败: {last_err}")
}

pub async fn fetch_google_userinfo(
    access_token: &str,
    proxy_spec: Option<&str>,
) -> anyhow::Result<GoogleUserInfo> {
    let client = build_http_client_for_proxy(proxy_spec, 20)?;
    let resp = client
        .get(GOOGLE_USERINFO_URL)
        .bearer_auth(access_token.trim())
        .send()
        .await
        .context("请求 Google UserInfo 失败")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("获取 Google 账号资料失败 (HTTP {}): {}", status.as_u16(), body);
    }
    let info: GoogleUserInfo =
        serde_json::from_str(&body).context("解析 Google 账号资料响应失败")?;
    if info.email.trim().is_empty() {
        bail!("Google UserInfo 未返回有效邮箱地址。");
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_callback_requires_matching_state() {
        assert!(callback_state_matches("/auth/callback?state=s1&code=c", "s1"));
        assert!(!callback_state_matches("/auth/callback?code=attacker", "s1"));
        assert!(!callback_state_matches("/auth/callback?state=other&code=c", "s1"));
    }

    #[test]
    fn scopes_match_official_clients_exactly() {
        let personal = oauth_scopes_for_client("personal");
        assert_eq!(personal.len(), 6);
        assert_eq!(personal.last(), Some(&ANTIGRAVITY_PERSONAL_EXTRA_SCOPE));
        assert!(!personal.contains(&"openid"));
        let gcp = oauth_scopes_for_client("gcp");
        assert_eq!(gcp, ANTIGRAVITY_OAUTH_SCOPES.to_vec());
        assert!(is_oauth_callback_path("/auth/callback?code=x"));
        assert!(is_oauth_callback_path("/oauth-callback?code=x"));
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(html_escape("<b>&\"'"), "&lt;b&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn parses_manual_callback_url_and_encoded_code() {
        let code = parse_manual_oauth_input(
            "http://localhost:54321/oauth-callback?state=abc-123&code=4%2F0AfJohXl_test&scope=email",
            Some("abc-123"),
        )
        .unwrap();
        assert_eq!(code, "4/0AfJohXl_test");

        let bare = parse_manual_oauth_input("4%2F0AfJohXl_bare", None).unwrap();
        assert_eq!(bare, "4/0AfJohXl_bare");
    }
}
