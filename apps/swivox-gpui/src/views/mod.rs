pub mod about;
pub mod agent_cache;
pub mod antigravity_accounts;
pub mod antigravity_enhance;
pub mod enhance;
pub mod lamplighter;
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
use crate::widgets::{AppBadge, AppButton};

/// 渲染全局统一居中遮罩二次确认弹窗（`deferred` + 全窗半透明遮罩 + `.occlude()`，0 布局位移）。
pub fn modal_dialog_overlay<F>(
    cx: &App,
    dialog_id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    danger_border: bool,
    on_dismiss: F,
    actions: Vec<AnyElement>,
) -> AnyElement
where
    F: Fn(&mut Window, &mut App) + 'static,
{
    let dialog_id = dialog_id.into();
    let backdrop_id = SharedString::from(format!("{dialog_id}-backdrop"));
    let close_id = SharedString::from(format!("{dialog_id}-close"));
    let on_dismiss = std::rc::Rc::new(on_dismiss);
    let on_backdrop = on_dismiss.clone();
    let on_close = on_dismiss;

    let border_col = if danger_border {
        cx.theme().danger.opacity(0.65)
    } else {
        cx.theme().primary.opacity(0.55)
    };

    deferred(
        div()
            .id(backdrop_id)
            .occlude()
            .absolute()
            .inset_0()
            .bg(hsla(0.0, 0.0, 0.0, 0.54))
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .on_click(move |_, window, cx| {
                on_backdrop(window, cx);
            })
            .child(
                div()
                    .id(dialog_id)
                    .occlude()
                    .v_flex()
                    .w(px(620.))
                    .max_w_full()
                    .gap_4()
                    .p_5()
                    .rounded(px(12.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(border_col)
                    .shadow_xl()
                    .on_click(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_base()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(title.into()),
                            )
                            .child(
                                AppButton::new(close_id)
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::X)
                                    .on_click(move |_, window, cx| {
                                        on_close(window, cx);
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(body.into()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .pt_1()
                            .children(actions),
                    ),
            ),
    )
    .with_priority(20)
    .into_any_element()
}

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
        .w_full()
        .min_h(px(72.))
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
                .items_center()
                .gap_3()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
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
