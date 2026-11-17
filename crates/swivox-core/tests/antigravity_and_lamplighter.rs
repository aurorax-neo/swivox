use std::collections::BTreeMap;

use rusqlite::{Connection, params};
use serde_json::{Value, json};
use swivox_core::antigravity::{
    db::{
        read_token_from_state_vscdb, state_vscdb_path, storage_json_path,
        sync_device_profile_to_state_vscdb, write_credentials_to_state_vscdb,
    },
    device::{
        generate_device_profile, read_device_profile_from_storage_json,
        write_device_profile_to_storage_json,
    },
    protobuf::{
        create_oauth_info, create_unified_state_entry, create_unified_topic_entry,
        decode_unified_state_entry_with_target, find_string_field, find_varint_field,
        remove_unified_topic_entry,
    },
    quota::normalize_subscription_tier,
};
use swivox_core::lamplighter::{
    config::{
        LamplighterConfig, canonical_provider_id, generate_mcp_bearer_key,
        plan_display_and_default_weight, provider_supports_ignition,
    },
    mcp::sync_mcp_server,
    providers::{parse_passive_response_headers, resolve_account_plan_and_weight},
    store::history_bucket_seconds,
};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[test]
fn antigravity_state_vscdb_three_layer_write_reads_back_and_purges_legacy_key() {
    let dir = tempdir().expect("create tempdir");
    let user_data = dir.path().join("Antigravity");
    let db_path = state_vscdb_path(&user_data);
    let storage_path = storage_json_path(&user_data);

    // 预埋旧版残留键与同 Topic 下的无关条目，验证写入后旧键被清除且同 Topic 邻居保留
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);")
            .unwrap();
        let sibling_b64 = create_unified_state_entry("customThemeSentinelKey", b"keep-me");
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            params!["antigravityUnifiedStateSync.oauthToken", sibling_b64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('jetskiStateSync.agentManagerInitState', 'stale-legacy-state')",
            [],
        )
        .unwrap();
    }

    let device = generate_device_profile();
    write_device_profile_to_storage_json(&storage_path, &device).expect("write storage.json");
    let read_device =
        read_device_profile_from_storage_json(&storage_path).expect("device profile present");
    assert_eq!(read_device.machine_id, device.machine_id);
    assert_eq!(read_device.mac_machine_id, device.mac_machine_id);
    assert_eq!(read_device.dev_device_id, device.dev_device_id);
    assert_eq!(read_device.sqm_id, device.sqm_id);

    write_credentials_to_state_vscdb(
        &db_path,
        "architect@corp.example.com",
        "ya29.enterprise-access-token",
        "1//0g-enterprise-refresh-token",
        1_890_000_000,
        Some("eyJ.id.token"),
        Some("gcp-enterprise-proj-42"),
        true,
        Some(&device),
    )
    .expect("write state.vscdb");

    // 自动备份 `.vscdb.backup` 已创建
    assert!(db_path.with_extension("vscdb.backup").is_file());

    let discovered =
        read_token_from_state_vscdb(&db_path, "Desktop").expect("discover written token");
    assert_eq!(discovered.email.as_deref(), Some("architect@corp.example.com"));
    assert_eq!(
        discovered.access_token.as_deref(),
        Some("ya29.enterprise-access-token")
    );
    assert_eq!(discovered.refresh_token, "1//0g-enterprise-refresh-token");
    assert_eq!(discovered.id_token.as_deref(), Some("eyJ.id.token"));
    assert_eq!(
        discovered.project_id.as_deref(),
        Some("gcp-enterprise-proj-42")
    );
    assert!(discovered.is_gcp_tos);

    // 验证旧键被删除、同 Topic 的 customThemeSentinelKey 完好无损、设备指纹双键写入到位
    let conn = Connection::open(&db_path).unwrap();
    let legacy_left: Option<String> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'jetskiStateSync.agentManagerInitState'",
            [],
            |r| r.get(0),
        )
        .ok();
    assert!(legacy_left.is_none());

    let raw_topic: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.oauthToken'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (_, kept_sibling) =
        decode_unified_state_entry_with_target(&raw_topic, Some("customThemeSentinelKey"))
            .expect("sibling topic preserved");
    assert_eq!(kept_sibling, b"keep-me");

    let service_mid: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'storage.serviceMachineId'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(service_mid, device.dev_device_id);

    // 验证单独刷新设备指纹同步
    let rotated = generate_device_profile();
    sync_device_profile_to_state_vscdb(&db_path, &rotated).expect("sync rotated device");
    let rotated_mid: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'telemetry.serviceMachineId'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rotated_mid, rotated.mac_machine_id);
}

#[test]
fn antigravity_personal_account_clears_gcp_tos_and_normalizes_tiers() {
    let personal_oauth = create_oauth_info(
        "ya29.personal",
        "1//0g-personal",
        1_850_000_000,
        true,
        None,
        Some("alice@gmail.com"),
    );
    // 个人 @gmail.com 账号即使传入 is_gcp_tos=true 也必须自动清除 field 6，防止路由至企业 Cloud Code 报错
    assert_eq!(find_varint_field(&personal_oauth, 6).unwrap(), None);
    assert_eq!(
        find_string_field(&personal_oauth, 3).as_deref(),
        Some("1//0g-personal")
    );

    let topic = create_unified_topic_entry("oauthTokenInfoSentinelKey", &personal_oauth);
    let stripped = remove_unified_topic_entry(&topic, "oauthTokenInfoSentinelKey").unwrap();
    assert!(stripped.is_empty());

    assert_eq!(normalize_subscription_tier(Some("g1-ultra-tier")), "ULTRA");
    assert_eq!(normalize_subscription_tier(Some("gemini-code-assist-pro")), "PRO");
    assert_eq!(normalize_subscription_tier(Some("enterprise-standard")), "ENTERPRISE");
    assert_eq!(normalize_subscription_tier(Some("free-tier")), "FREE");
    assert_eq!(normalize_subscription_tier(None), "FREE");
}

#[test]
fn lamplighter_config_normalization_and_passive_header_hooks_work() {
    let mut cfg = LamplighterConfig {
        // 故意倒置阈值，验证 normalize 自动修复为严格递减 notice > low > critical
        notice_threshold: 10.0,
        low_threshold: 55.0,
        critical_threshold: 35.0,
        mcp_enabled: true,
        mcp_key: String::new(),
        ..LamplighterConfig::default()
    };
    cfg.account_weights
        .insert("swivox_codex:acc-pro".to_string(), 8.5);
    cfg.normalize();

    assert!(cfg.notice_threshold >= cfg.low_threshold);
    assert!(cfg.low_threshold >= cfg.critical_threshold);
    assert!(!cfg.mcp_key.trim().is_empty());
    assert!(generate_mcp_bearer_key().len() >= 32);

    assert_eq!(canonical_provider_id("openai"), "codex");
    assert_eq!(canonical_provider_id("grok"), "xai");
    assert!(provider_supports_ignition("codex"));
    assert!(provider_supports_ignition("claude"));
    assert!(provider_supports_ignition("antigravity"));
    assert!(!provider_supports_ignition("kimi"));

    let (default_plan, default_w) = plan_display_and_default_weight("codex", Some("pro"));
    assert_eq!(default_plan, "Pro $200");
    assert!(default_w > 1.0);

    let (custom_plan, custom_w, is_custom) =
        resolve_account_plan_and_weight(&cfg, "swivox_codex:acc-pro", "codex", Some("pro"));
    assert_eq!(custom_plan, "Pro $200");
    assert_eq!(custom_w, 8.5);
    assert!(is_custom);

    // 验证 Codex 上游响应头被动额度解析
    let mut codex_headers = BTreeMap::new();
    codex_headers.insert("X-Codex-Primary-Used-Percent".to_string(), "37.5".to_string());
    codex_headers.insert("X-Codex-Primary-Window-Minutes".to_string(), "300".to_string());
    codex_headers.insert("X-Codex-Primary-Reset-After-Seconds".to_string(), "7200".to_string());
    codex_headers.insert("X-Codex-Secondary-Used-Percent".to_string(), "12.0".to_string());
    codex_headers.insert("X-Codex-Secondary-Window-Minutes".to_string(), "10080".to_string());

    let (provider, wins) =
        parse_passive_response_headers(&codex_headers, 1_700_000_000).expect("parse codex headers");
    assert_eq!(provider, "codex");
    assert_eq!(wins.len(), 2);
    assert_eq!(wins[0].id, "five-hour");
    assert!((wins[0].remaining_percent - 62.5).abs() < 1e-6);
    assert_eq!(wins[0].reset_at, Some(1_700_007_200));
    assert_eq!(wins[1].id, "seven-day");
    assert!((wins[1].remaining_percent - 88.0).abs() < 1e-6);

    // 验证 Anthropic 上游响应头被动额度解析
    let mut claude_headers = BTreeMap::new();
    claude_headers.insert(
        "anthropic-ratelimit-unified-5h-utilization".to_string(),
        "0.25".to_string(),
    );
    claude_headers.insert(
        "anthropic-ratelimit-unified-5h-reset".to_string(),
        "1700003600".to_string(),
    );
    let (c_prov, c_wins) =
        parse_passive_response_headers(&claude_headers, 1_700_000_000).expect("parse claude");
    assert_eq!(c_prov, "claude");
    assert_eq!(c_wins.len(), 1);
    assert!((c_wins[0].remaining_percent - 75.0).abs() < 1e-6);

    // 验证长跨度历史分桶粒度
    assert_eq!(history_bucket_seconds(24 * 3600), 0);
    assert_eq!(history_bucket_seconds(3 * 86_400), 600);
    assert_eq!(history_bucket_seconds(30 * 86_400), 3600);
}

#[tokio::test]
async fn lamplighter_mcp_server_enforces_bearer_auth_origin_and_serves_redacted_tools() {
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);

    let secret = "swivox-mcp-test-bearer-secret-9981";
    let handle = tokio::runtime::Handle::current();
    sync_mcp_server(&handle, true, &addr, secret);
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    async fn raw_http_post(addr: &str, headers: &str, body: &Value) -> String {
        let payload = serde_json::to_string(body).unwrap();
        let req = format!(
            "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        let mut stream = TcpStream::connect(addr).await.expect("connect mcp");
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).await.unwrap();
        resp
    }

    // 1. 错误 Token 必须返回 401 Unauthorized
    let unauth = raw_http_post(
        &addr,
        "Authorization: Bearer wrong-token\r\n",
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .await;
    assert!(unauth.starts_with("HTTP/1.1 401 Unauthorized"));

    // 2. 跨源浏览器 Origin 必须返回 403 Forbidden
    let cross_origin = raw_http_post(
        &addr,
        &format!("Authorization: Bearer {secret}\r\nOrigin: https://evil.example.com\r\n"),
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    assert!(cross_origin.starts_with("HTTP/1.1 403 Forbidden"));

    // 3. 合法 Bearer Token 返回 JSON-RPC tools/list 且包含 quota_status / quota_events
    let ok_list = raw_http_post(
        &addr,
        &format!("Authorization: Bearer {secret}\r\n"),
        &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
    )
    .await;
    assert!(ok_list.starts_with("HTTP/1.1 200 OK"));
    let body_json: Value =
        serde_json::from_str(ok_list.split("\r\n\r\n").nth(1).unwrap()).expect("parse jsonrpc");
    let tool_names: Vec<&str> = body_json["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(tool_names.contains(&"list_quotas"));
    assert!(tool_names.contains(&"get_quota_events"));
    assert!(tool_names.contains(&"refresh_quotas"));

    // 4. 调用 list_quotas 工具验证脱敏 JSON 结构（绝不包含 @ 邮箱或 token 字段）
    let ok_call = raw_http_post(
        &addr,
        &format!("Authorization: Bearer {secret}\r\n"),
        &json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "list_quotas", "arguments": {} }
        }),
    )
    .await;
    assert!(ok_call.starts_with("HTTP/1.1 200 OK"));
    let call_body: Value =
        serde_json::from_str(ok_call.split("\r\n\r\n").nth(1).unwrap()).expect("parse call");
    let text_out = call_body["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text output");
    assert!(!text_out.contains("access_token"));
    assert!(!text_out.contains("refresh_token"));

    // 关闭测试监听
    sync_mcp_server(&handle, false, "", "");
}

#[test]
fn network_proxies_and_lamplighter_clients_and_accounts_are_strictly_isolated() {
    use swivox_core::antigravity::{AntigravityClientKind, resolve_launch_proxy_for_target};
    use swivox_core::lamplighter::{
        LamplighterEventRecord, observe_passive_headers_for_account,
        providers::{UnifiedCredential, resolve_effective_ignition_proxy_spec},
        store::{
            append_client_event_record, append_client_history_sample, client_history_dir,
            client_state_file_path, load_client_state, query_client_history_and_events,
            save_client_state, save_config, set_lamplighter_dir_for_tests,
        },
    };
    use swivox_core::network_proxies::{
        ClientProxyAssignments, resolve_app_self_proxy_spec, resolve_client_proxy_spec,
        set_network_proxies_path_for_tests, update_proxy_assignments, upsert_proxy_node,
    };
    use swivox_core::official_accounts::{
        OfficialAccount, OfficialAccountsState, OfficialTokens, save_official_accounts_state,
    };
    use swivox_core::paths::set_settings_path_for_tests;

    let dir = tempdir().expect("tempdir");
    let prev_px = set_network_proxies_path_for_tests(Some(dir.path().join("network-proxies.json")));
    let prev_ll = set_lamplighter_dir_for_tests(Some(dir.path().join("lamplighter")));
    let prev_oa = set_settings_path_for_tests(Some(dir.path().join("settings.json")));

    // 1. 建立代理节点池并为本程序、Codex、Antigravity 桌面版、Antigravity IDE 分配互不相同的代理
    let cfg_px = upsert_proxy_node(
        None,
        "Swivox 自身代理".to_string(),
        "http://127.0.0.1:7890".to_string(),
        None,
    )
    .unwrap();
    let self_node = cfg_px.nodes[0].id.clone();

    let cfg_px = upsert_proxy_node(
        None,
        "Codex 专属美区".to_string(),
        "socks5://127.0.0.1:1081".to_string(),
        None,
    )
    .unwrap();
    let codex_node = cfg_px.nodes[1].id.clone();

    let cfg_px = upsert_proxy_node(
        None,
        "Agy 桌面高速".to_string(),
        "http://127.0.0.1:7892".to_string(),
        None,
    )
    .unwrap();
    let ag_desk_node = cfg_px.nodes[2].id.clone();

    let cfg_px = upsert_proxy_node(
        None,
        "Agy IDE 独立代理".to_string(),
        "http://127.0.0.1:7893".to_string(),
        None,
    )
    .unwrap();
    let ag_ide_node = cfg_px.nodes[3].id.clone();

    update_proxy_assignments(
        format!("node:{self_node}"),
        ClientProxyAssignments {
            codex: format!("node:{codex_node}"),
            antigravity_desktop: format!("node:{ag_desk_node}"),
            antigravity_ide: format!("node:{ag_ide_node}"),
            external: "direct".to_string(),
        },
    )
    .unwrap();

    assert_eq!(
        resolve_app_self_proxy_spec().as_deref(),
        Some("http://127.0.0.1:7890")
    );
    assert_eq!(
        resolve_client_proxy_spec("codex").as_deref(),
        Some("socks5://127.0.0.1:1081")
    );
    assert_eq!(
        resolve_launch_proxy_for_target(AntigravityClientKind::Desktop, "inherit", "").as_deref(),
        Some("http://127.0.0.1:7892")
    );
    assert_eq!(
        resolve_launch_proxy_for_target(AntigravityClientKind::Ide, "inherit", "").as_deref(),
        Some("http://127.0.0.1:7893")
    );

    // 验证点火严格使用所属客户端隔离代理，绝不混用本程序代理或其他客户端代理
    let codex_cred = UnifiedCredential {
        client_id: "codex".to_string(),
        account_key: "swivox_codex:acc1".to_string(),
        provider: "codex".to_string(),
        email: "alpha@openai.com".to_string(),
        note_name: None,
        source_kind: "swivox_codex",
        raw_account_id: "acc1".to_string(),
        access_token: "tok-alpha".to_string(),
        refresh_token: None,
        chatgpt_account_id: Some("ws-alpha".to_string()),
        google_project_id: None,
        domain: None,
        proxy_spec: None,
        cached_plan: None,
    };
    let ag_cred = UnifiedCredential {
        client_id: "antigravity".to_string(),
        account_key: "swivox_ag:ag1".to_string(),
        provider: "antigravity".to_string(),
        email: "studio@google.com".to_string(),
        note_name: None,
        source_kind: "swivox_ag",
        raw_account_id: "ag1".to_string(),
        access_token: "ya29.studio".to_string(),
        refresh_token: Some("1//0g-studio".to_string()),
        chatgpt_account_id: None,
        google_project_id: None,
        domain: None,
        proxy_spec: None,
        cached_plan: None,
    };
    assert_eq!(
        resolve_effective_ignition_proxy_spec(&codex_cred).as_deref(),
        Some("socks5://127.0.0.1:1081")
    );
    assert_eq!(
        resolve_effective_ignition_proxy_spec(&ag_cred).as_deref(),
        Some("http://127.0.0.1:7892")
    );

    // 2. 验证各客户端点火状态与历史目录在物理磁盘上完全隔离（clients/codex vs clients/antigravity）
    let mut ll_cfg = LamplighterConfig::default();
    ll_cfg.ignition_for_client_mut("codex").start_hour = 7;
    ll_cfg.ignition_for_client_mut("antigravity").start_hour = 9;
    ll_cfg.ignition_for_client_mut("antigravity").enabled = false;
    save_config(&mut ll_cfg).unwrap();
    assert!(ll_cfg.ignition_for_client("codex").enabled);
    assert!(!ll_cfg.ignition_for_client("antigravity").enabled);
    assert_eq!(ll_cfg.ignition_for_client("antigravity").start_hour, 9);

    let now = chrono::Utc::now().timestamp();
    let mut codex_state = load_client_state("codex");
    codex_state
        .scheduler
        .entry("codex:swivox_codex:acc1:codex:main".to_string())
        .or_default()
        .last_success_at = now - 60;
    save_client_state("codex", &codex_state).unwrap();

    let ag_state = load_client_state("antigravity");
    assert!(ag_state.scheduler.is_empty());
    assert!(client_state_file_path("codex").is_file());
    assert!(!client_state_file_path("antigravity").is_file());

    append_client_history_sample(
        "codex",
        now - 10,
        "codex:swivox_codex:acc1:codex:main",
        "five-hour",
        82.0,
        Some(now + 3600),
        "active",
    );
    append_client_event_record(
        "antigravity",
        &LamplighterEventRecord {
            timestamp: now - 5,
            group_key: "antigravity:swivox_ag:ag1:antigravity:gemini".to_string(),
            kind: "ignite".to_string(),
            level: "info".to_string(),
            label: "Antigravity · Gemini".to_string(),
            message: "Agy 独立点火事件".to_string(),
            detail: None,
        },
    );
    assert!(client_history_dir("codex").is_dir());
    assert!(client_history_dir("antigravity").is_dir());

    let empty_labels = BTreeMap::new();
    let (codex_series, codex_events) =
        query_client_history_and_events("codex", 12, None, &empty_labels);
    let (ag_series, ag_events) =
        query_client_history_and_events("antigravity", 12, None, &empty_labels);
    assert_eq!(codex_series.len(), 1);
    assert!(codex_events.is_empty());
    assert!(ag_series.is_empty());
    assert_eq!(ag_events.len(), 1);

    // 3. 验证同一客户端内多账号被动响应头严格按 chatgpt_account_id 隔离，绝不串写到第一个账号
    let oa_state = OfficialAccountsState {
        accounts: vec![
            OfficialAccount {
                id: "acc-first".to_string(),
                email: "first@openai.com".to_string(),
                label: Some("一号".to_string()),
                user_id: None,
                plan_type: Some("plus".to_string()),
                account_id: Some("ws-first".to_string()),
                organization_id: None,
                account_name: None,
                tokens: OfficialTokens {
                    id_token: "id-1".to_string(),
                    access_token: "at-1".to_string(),
                    refresh_token: Some("rt-1".to_string()),
                },
                quota: None,
                bound_relay_profile_id: None,
                proxy_mode: "inherit".to_string(),
                egress_proxy_url: None,
                created_at: now,
                last_used: now,
                tags: Vec::new(),
            },
            OfficialAccount {
                id: "acc-second".to_string(),
                email: "second@openai.com".to_string(),
                label: Some("二号".to_string()),
                user_id: None,
                plan_type: Some("pro".to_string()),
                account_id: Some("ws-second".to_string()),
                organization_id: None,
                account_name: None,
                tokens: OfficialTokens {
                    id_token: "id-2".to_string(),
                    access_token: "at-2".to_string(),
                    refresh_token: Some("rt-2".to_string()),
                },
                quota: None,
                bound_relay_profile_id: None,
                proxy_mode: "inherit".to_string(),
                egress_proxy_url: None,
                created_at: now,
                last_used: now,
                tags: Vec::new(),
            },
        ],
        active_account_id: Some("acc-first".to_string()),
        global_proxy_mode: "inherit".to_string(),
        global_proxy_url: String::new(),
        proxy_resources: Vec::new(),
        relay_profile_proxies: std::collections::HashMap::new(),
        relay_profile_bound_accounts: std::collections::HashMap::new(),
    };
    save_official_accounts_state(&oa_state).unwrap();

    let mut passive_hdrs = BTreeMap::new();
    passive_hdrs.insert("x-codex-primary-used-percent".to_string(), "44.0".to_string());
    passive_hdrs.insert("x-codex-primary-window-minutes".to_string(), "300".to_string());
    observe_passive_headers_for_account(
        &passive_hdrs,
        Some("ws-second"),
        Some("at-2"),
        now,
    );

    let (codex_series_after, _) =
        query_client_history_and_events("codex", 12, None, &empty_labels);
    assert!(
        codex_series_after
            .iter()
            .any(|s| s.group_key == "codex:swivox_codex:acc-second:codex:main"),
        "expected passive sample recorded strictly under acc-second, got: {codex_series_after:?}"
    );
    assert!(
        !codex_series_after
            .iter()
            .any(|s| s.group_key == "codex:swivox_codex:acc-first:codex:main"),
        "acc-first must not be polluted by acc-second's passive headers"
    );

    set_settings_path_for_tests(prev_oa);
    set_lamplighter_dir_for_tests(prev_ll);
    set_network_proxies_path_for_tests(prev_px);
}
