use anyhow::Context;

pub(crate) fn apply_configured_ca_certificate(
    mut builder: reqwest::ClientBuilder,
) -> anyhow::Result<reqwest::ClientBuilder> {
    let Some(value) = std::env::var_os("CODEX_CA_CERTIFICATE") else {
        return Ok(builder);
    };
    let value = value.to_string_lossy().trim().to_string();
    if value.is_empty() {
        return Ok(builder);
    }
    let pem = if value.contains("BEGIN CERTIFICATE") {
        value.into_bytes()
    } else {
        std::fs::read(&value).with_context(|| {
            format!("读取 CODEX_CA_CERTIFICATE 指定的证书文件失败：{value}")
        })?
    };
    let certificate = reqwest::Certificate::from_pem(&pem)
        .context("CODEX_CA_CERTIFICATE 不是有效的 PEM 证书")?;
    builder = builder.add_root_certificate(certificate);
    Ok(builder)
}

pub fn proxied_client(user_agent: &str) -> anyhow::Result<reqwest::Client> {
    proxied_client_with_timeout(user_agent, std::time::Duration::from_secs(35))
}

pub fn proxied_client_with_timeout(
    user_agent: &str,
    timeout: std::time::Duration,
) -> anyhow::Result<reqwest::Client> {
    proxied_client_with_timeouts(user_agent, std::time::Duration::from_secs(10), timeout)
}

pub fn proxied_client_with_timeouts(
    user_agent: &str,
    connect_timeout: std::time::Duration,
    total_timeout: std::time::Duration,
) -> anyhow::Result<reqwest::Client> {
    let ua = if user_agent.trim().is_empty() {
        format!("Swivox/{}", env!("CARGO_PKG_VERSION"))
    } else {
        user_agent.trim().to_string()
    };
    let mut builder = reqwest::Client::builder()
        .user_agent(ua)
        .connect_timeout(connect_timeout)
        .timeout(total_timeout);
    let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
    if !crate::proxy::has_proxy_environment(&env_map) {
        let configured_proxy = crate::network_proxies::resolve_app_self_proxy_spec();
        if let Some(proxy_spec) = configured_proxy {
            if proxy_spec.eq_ignore_ascii_case("direct") {
                builder = builder.no_proxy();
            } else if let Ok(proxy) = reqwest::Proxy::all(&proxy_spec) {
                let no_proxy = reqwest::NoProxy::from_env()
                    .or_else(|| reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"));
                builder = builder.proxy(proxy.no_proxy(no_proxy));
            }
        }
    }
    Ok(apply_configured_ca_certificate(builder)?.build()?)
}

/// VLM 专用 HTTP client（带超时）。
/// 不复用通用 proxied_client，避免 VLM 服务无响应时永久阻塞整个代理。
pub fn vlm_http_client() -> anyhow::Result<reqwest::Client> {
    vlm_http_client_with_timeout(
        std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(30),
    )
}

pub(crate) fn vlm_http_client_with_timeout(
    connect: std::time::Duration,
    total: std::time::Duration,
) -> anyhow::Result<reqwest::Client> {
    Ok(apply_configured_ca_certificate(
        reqwest::Client::builder()
            .user_agent(format!("Swivox-VLM/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(connect)
            .timeout(total),
    )?
    .build()?)
}
