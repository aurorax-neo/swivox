use swivox_backend::i18n::{t, tf};
use gpui_kit::assets::IconName;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{health_item_row, panel_card};
use crate::widgets::AppButton;

pub fn render_relay_environment(store: &Entity<AppStore>, cx: &App) -> impl IntoElement {
    let data = store.read(cx);
    let report = data.relay_environment.as_ref();
    let refreshing = data.env_refreshing;

    let tun_passed = report.is_some_and(|r| !r.clash_verge_tun.enabled);
    let tun_detail = match report {
        Some(r) => {
            if r.clash_verge_tun.enabled {
                let cfg = r
                    .clash_verge_tun
                    .config_path
                    .clone()
                    .unwrap_or_else(|| t("未记录路径"));
                tf(
                    "检测到 TUN 模式已开启，请在 Clash Verge Rev 中关闭。配置：{0}",
                    &[&cfg],
                )
            } else if let Some(cfg) = &r.clash_verge_tun.config_path {
                tf("TUN 模式已关闭。配置：{0}", &[cfg])
            } else {
                t("未发现 Clash Verge Rev 配置，按未开启处理。")
            }
        }
        None => t("等待检测。"),
    };

    let proxy_passed = report.is_some_and(|r| r.proxy_environment.variables.is_empty());
    let proxy_detail = match report {
        Some(r) => {
            if r.proxy_environment.variables.is_empty() {
                t("未检测到 HTTP_PROXY、HTTPS_PROXY、ALL_PROXY、NO_PROXY 或 FTP_PROXY。")
            } else {
                let labels: Vec<String> = r
                    .proxy_environment
                    .variables
                    .iter()
                    .map(|item| {
                        let source = match item.source {
                            swivox_core::relay_environment::ProxyEnvironmentSource::User => {
                                t("用户环境")
                            }
                            swivox_core::relay_environment::ProxyEnvironmentSource::System => {
                                t("系统环境")
                            }
                            swivox_core::relay_environment::ProxyEnvironmentSource::Process => {
                                t("进程环境")
                            }
                        };
                        tf("{0}（{1}）", &[&item.name, &source])
                    })
                    .collect();
                let sep = t("、");
                tf(
                    "检测到代理环境变量：{0}。请清理后重新启动 Codex。",
                    &[&labels.join(&sep)],
                )
            }
        }
        None => t("等待检测。"),
    };

    let dotenv_passed = report.is_some_and(|r| !r.codex_env_file.exists);
    let dotenv_detail = match report {
        Some(r) => {
            if r.codex_env_file.exists {
                tf(
                    "检测到可能干扰供应商配置的 .env 文件：{0}",
                    &[&r.codex_env_file.path],
                )
            } else {
                tf("未发现 .env 文件：{0}", &[&r.codex_env_file.path])
            }
        }
        None => t("等待检测。"),
    };

    let all_passed = report.is_some() && tun_passed && proxy_passed && dotenv_passed;
    let header_detail = match report {
        Some(_) if all_passed => t("三项检测全部通过"),
        Some(_) => t("检测到需要处理的环境问题"),
        None => t("正在读取本机环境"),
    };

    let status_for = |passed: bool| -> &'static str {
        match report {
            Some(_) if passed => "ok",
            Some(_) => "failed",
            None => "not_checked",
        }
    };

    let store_refresh = store.clone();

    panel_card(
        cx,
        div()
            .v_flex()
            .w_full()
            .gap_4()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(t("中转站环境配置检测")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(header_detail),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .child(health_item_row(
                        cx,
                        tun_passed,
                        t("Clash Verge Rev TUN 模式"),
                        tun_detail,
                        status_for(tun_passed),
                    ))
                    .child(health_item_row(
                        cx,
                        proxy_passed,
                        t("系统代理环境变量"),
                        proxy_detail,
                        status_for(proxy_passed),
                    ))
                    .child(health_item_row(
                        cx,
                        dotenv_passed,
                        t("Codex .env 文件"),
                        dotenv_detail,
                        status_for(dotenv_passed),
                    )),
            )
            .child(
                div().h_flex().pt_1().child(
                    AppButton::new("relay-env-refresh")
                        .primary()
                        .small()
                        .disabled(refreshing)
                        .icon(IconName::RefreshCw)
                        .label(t("重新检测"))
                        .on_click(move |_, _, cx| {
                            store_refresh
                                .update(cx, |s, cx| s.refresh_relay_environment(true, cx));
                        }),
                ),
            ),
    )
}
