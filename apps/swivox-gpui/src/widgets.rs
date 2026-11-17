use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::theme::{ActiveTheme as _, Theme};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// 校准平台 CJK 字体栈并注入 Swivox 品牌主色令牌（对齐原版 `styles.css`）。
pub fn apply_cjk_typography(cx: &mut App) {
    let is_dark = cx.theme().is_dark();
    let theme = Theme::global_mut(cx);

    #[cfg(target_os = "macos")]
    {
        theme.font_family = SharedString::from("PingFang SC");
    }
    #[cfg(target_os = "windows")]
    {
        theme.font_family = SharedString::from("Microsoft YaHei UI");
    }

    if is_dark {
        theme.primary = hsla(232.0 / 360.0, 0.86, 0.62, 1.0);
        theme.primary_foreground = hsla(0.0, 0.0, 1.0, 1.0);
        theme.danger = hsla(0.0, 0.78, 0.56, 1.0);
        theme.danger_foreground = hsla(0.0, 0.0, 1.0, 1.0);
    } else {
        theme.primary = hsla(232.0 / 360.0, 0.82, 0.56, 1.0);
        theme.primary_foreground = hsla(0.0, 0.0, 1.0, 1.0);
        theme.danger = hsla(0.0, 0.76, 0.52, 1.0);
        theme.danger_foreground = hsla(0.0, 0.0, 1.0, 1.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppButtonVariant {
    #[default]
    Secondary,
    Primary,
    Outline,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum AppButtonSize {
    #[default]
    Medium,
    Small,
    Icon,
    SmallIcon,
    RailIcon,
}

/// 像素级水平/垂直双向正居中的通用按钮组件（对齐原版 `components/ui/button.tsx`）。
#[derive(IntoElement)]
pub struct AppButton {
    id: ElementId,
    variant: AppButtonVariant,
    size: AppButtonSize,
    icon: Option<Icon>,
    label: Option<SharedString>,
    tooltip: Option<SharedString>,
    disabled: bool,
    selected: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl AppButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: AppButtonVariant::Secondary,
            size: AppButtonSize::Medium,
            icon: None,
            label: None,
            tooltip: None,
            disabled: false,
            selected: false,
            on_click: None,
        }
    }

    pub fn primary(mut self) -> Self {
        self.variant = AppButtonVariant::Primary;
        self
    }

    pub fn secondary(mut self) -> Self {
        self.variant = AppButtonVariant::Secondary;
        self
    }

    pub fn outline(mut self) -> Self {
        self.variant = AppButtonVariant::Outline;
        self
    }

    pub fn ghost(mut self) -> Self {
        self.variant = AppButtonVariant::Ghost;
        self
    }

    pub fn danger(mut self) -> Self {
        self.variant = AppButtonVariant::Danger;
        self
    }

    pub fn small(mut self) -> Self {
        self.size = AppButtonSize::Small;
        self
    }

    #[allow(dead_code)]
    pub fn icon_only(mut self) -> Self {
        self.size = AppButtonSize::Icon;
        self
    }

    pub fn small_icon(mut self) -> Self {
        self.size = AppButtonSize::SmallIcon;
        self
    }

    pub fn rail_icon(mut self) -> Self {
        self.size = AppButtonSize::RailIcon;
        self
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for AppButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let (bg, fg, border, hover_bg) = if self.selected {
            match self.variant {
                AppButtonVariant::Primary => (
                    theme.primary,
                    theme.primary_foreground,
                    theme.primary,
                    theme.primary.opacity(0.9),
                ),
                _ => (
                    theme.primary.opacity(0.16),
                    theme.foreground,
                    theme.primary.opacity(0.55),
                    theme.primary.opacity(0.24),
                ),
            }
        } else {
            match self.variant {
                AppButtonVariant::Primary => (
                    theme.primary,
                    theme.primary_foreground,
                    theme.primary,
                    theme.primary.opacity(0.88),
                ),
                AppButtonVariant::Secondary => (
                    theme.secondary,
                    theme.secondary_foreground,
                    theme.border.opacity(0.8),
                    theme.secondary.opacity(0.75),
                ),
                AppButtonVariant::Outline => (
                    theme.transparent,
                    theme.foreground,
                    theme.border,
                    theme.accent.opacity(0.6),
                ),
                AppButtonVariant::Ghost => (
                    theme.transparent,
                    theme.muted_foreground,
                    theme.transparent,
                    theme.accent.opacity(0.7),
                ),
                AppButtonVariant::Danger => (
                    theme.danger,
                    theme.danger_foreground,
                    theme.danger,
                    theme.danger.opacity(0.88),
                ),
            }
        };

        let has_border = matches!(
            self.variant,
            AppButtonVariant::Secondary | AppButtonVariant::Outline
        ) || (self.selected && self.variant == AppButtonVariant::Ghost);

        let (height, pad_x, gap_px, font_px, icon_px, radius_px, fixed_square) = match self.size {
            AppButtonSize::Medium => (px(32.), px(12.), px(6.), px(13.), px(15.), px(8.), false),
            AppButtonSize::Small => (px(29.), px(11.), px(5.5), px(12.5), px(14.), px(7.), false),
            AppButtonSize::Icon => (px(32.), px(0.), px(0.), px(13.), px(15.), px(8.), true),
            AppButtonSize::SmallIcon => (px(29.), px(0.), px(0.), px(12.), px(14.), px(7.), true),
            AppButtonSize::RailIcon => (px(36.), px(0.), px(0.), px(13.), px(17.), px(9.), true),
        };

        let disabled = self.disabled;
        let hover_fg = if self.variant == AppButtonVariant::Ghost && !self.selected {
            theme.accent_foreground
        } else {
            fg
        };

        let mut el = div()
            .id(self.id)
            .flex()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .h(height)
            .when(fixed_square, |d| d.w(height))
            .when(!fixed_square, |d| d.px(pad_x).gap(gap_px))
            .rounded(radius_px)
            .bg(bg)
            .text_color(fg)
            .when(has_border, |d| d.border_1().border_color(border))
            .when(disabled, |d| d.opacity(0.45).cursor_default())
            .when(!disabled, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
                    .active(move |s| s.opacity(0.9))
            });

        if let Some(icon) = self.icon {
            el = el.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .size(icon_px)
                    .items_center()
                    .justify_center()
                    .child(
                        icon.with_size(gpui_kit::component::Size::Size(icon_px))
                            .text_color(fg),
                    ),
            );
        }

        if let Some(label) = self.label {
            el = el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .pt(px(0.5))
                    .whitespace_nowrap()
                    .font_medium()
                    .text_size(font_px)
                    .line_height(px(16.))
                    .child(label),
            );
        }

        if let Some(tip) = self.tooltip {
            el = el.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx));
        }

        if !disabled
            && let Some(on_click) = self.on_click
        {
            el = el.on_click(move |ev, window, cx| on_click(ev, window, cx));
        }

        el
    }
}

/// 左文右箭头的标准下拉选择触发器（用于供应商绑定、协议切换、策略选择等）。
#[derive(IntoElement)]
pub struct AppSelectTrigger {
    id: ElementId,
    label: SharedString,
    disabled: bool,
    open: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl AppSelectTrigger {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            disabled: false,
            open: false,
            on_click: None,
        }
    }

    #[allow(dead_code)]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for AppSelectTrigger {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let border = if self.open {
            theme.primary.opacity(0.75)
        } else {
            theme.border
        };
        let disabled = self.disabled;

        let mut el = div()
            .id(self.id)
            .h_flex()
            .w_full()
            .h(px(30.))
            .px_3()
            .items_center()
            .justify_between()
            .gap_2()
            .rounded(px(7.))
            .bg(theme.background)
            .border_1()
            .border_color(border)
            .when(disabled, |d| d.opacity(0.5).cursor_default())
            .when(!disabled, |d| {
                d.cursor_pointer()
                    .hover(|s| s.bg(theme.accent.opacity(0.45)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pt(px(0.5))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_size(px(12.5))
                    .line_height(px(16.))
                    .text_color(theme.foreground)
                    .child(self.label),
            )
            .child(
                Icon::new(IconName::ChevronDown)
                    .with_size(gpui_kit::component::Size::Size(px(14.)))
                    .text_color(theme.muted_foreground),
            );

        if !disabled
            && let Some(on_click) = self.on_click
        {
            el = el.on_click(move |ev, window, cx| on_click(ev, window, cx));
        }
        el
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppBadgeVariant {
    #[default]
    Secondary,
    Outline,
    Good,
    Warn,
    Bad,
    Info,
}

/// 像素级水平/垂直双向正居中的胶囊徽章组件。
#[derive(IntoElement)]
pub struct AppBadge {
    label: SharedString,
    variant: AppBadgeVariant,
}

impl AppBadge {
    #[allow(dead_code)]
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            variant: AppBadgeVariant::Secondary,
        }
    }

    pub fn good(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            variant: AppBadgeVariant::Good,
        }
    }

    pub fn warn(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            variant: AppBadgeVariant::Warn,
        }
    }

    pub fn bad(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            variant: AppBadgeVariant::Bad,
        }
    }

    pub fn info(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            variant: AppBadgeVariant::Info,
        }
    }

    pub fn secondary(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            variant: AppBadgeVariant::Secondary,
        }
    }

    pub fn outline(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            variant: AppBadgeVariant::Outline,
        }
    }
}

impl RenderOnce for AppBadge {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, border) = match self.variant {
            AppBadgeVariant::Good => (
                theme.success.opacity(0.14),
                theme.success,
                theme.success.opacity(0.45),
            ),
            AppBadgeVariant::Warn => (
                theme.warning.opacity(0.14),
                theme.warning,
                theme.warning.opacity(0.45),
            ),
            AppBadgeVariant::Bad => (
                theme.danger.opacity(0.14),
                theme.danger,
                theme.danger.opacity(0.45),
            ),
            AppBadgeVariant::Info => (
                theme.info.opacity(0.14),
                theme.info,
                theme.info.opacity(0.45),
            ),
            AppBadgeVariant::Secondary => (
                theme.secondary,
                theme.secondary_foreground,
                theme.border.opacity(0.7),
            ),
            AppBadgeVariant::Outline => (theme.transparent, theme.foreground, theme.border),
        };

        div()
            .flex()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .h(px(22.))
            .px(px(9.))
            .pt(px(0.5))
            .rounded_full()
            .border_1()
            .bg(bg)
            .border_color(border)
            .text_color(fg)
            .font_semibold()
            .text_size(px(11.))
            .line_height(px(14.))
            .whitespace_nowrap()
            .child(self.label)
    }
}
