pub mod about;
pub mod agent_cache;
pub mod enhance;
pub mod maintenance;
pub mod mcp;
pub mod multi_launch;
pub mod official_accounts;
pub mod overview;
pub mod plugin_market;
pub mod relay;
pub mod relay_env;
pub mod sessions;
pub mod settings;
pub mod skills;
pub mod user_scripts;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::*;

use crate::routes::{StatusTone, status_label, status_tone};
use crate::widgets::AppBadge;

pub fn status_badge(status: &str) -> AppBadge {
    let label = status_label(status);
    match status_tone(status) {
        StatusTone::Good => AppBadge::good(label),
        StatusTone::Bad => AppBadge::bad(label),
        StatusTone::Warn => AppBadge::warn(label),
    }
}

pub fn avatar_badge(cx: &App, initial: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .size(px(34.))
        .rounded(px(9.))
        .items_center()
        .justify_center()
        .bg(cx.theme().primary.opacity(0.14))
        .border_1()
        .border_color(cx.theme().primary.opacity(0.32))
        .text_color(cx.theme().primary)
        .font_semibold()
        .text_size(px(13.))
        .line_height(px(13.))
        .child(initial.into())
}

pub fn panel_card(cx: &App, children: impl IntoElement) -> Div {
    div()
        .v_flex()
        .w_full()
        .gap_4()
        .p_5()
        .rounded(px(12.))
        .bg(cx.theme().background)
        .border_1()
        .border_color(cx.theme().border)
        .child(children)
}

pub fn health_item_row(
    cx: &App,
    ok: bool,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    status: &str,
) -> Div {
    let icon_name = if ok {
        IconName::CircleCheck
    } else {
        IconName::Bell
    };
    let icon_color = if ok {
        cx.theme().success
    } else {
        cx.theme().warning
    };
    let border_tint = if ok {
        cx.theme().border
    } else {
        cx.theme().warning.opacity(0.45)
    };

    div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap_3()
        .p_3p5()
        .rounded(px(10.))
        .bg(cx.theme().secondary.opacity(0.35))
        .border_1()
        .border_color(border_tint)
        .child(
            div()
                .h_flex()
                .items_start()
                .gap_3()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .pt_0p5()
                        .child(Icon::new(icon_name).small().text_color(icon_color)),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(title.into()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(detail.into()),
                        ),
                ),
        )
        .child(status_badge(status))
}
