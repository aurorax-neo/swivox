use swivox_backend::i18n::t;

#[cfg(not(target_os = "macos"))]
pub const TRAY_MENU_SHOW: &str = "tray_show_main";
#[cfg(not(target_os = "macos"))]
pub const TRAY_MENU_QUIT: &str = "tray_quit_app";

const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray-icon.png");
#[cfg(target_os = "macos")]
const APP_ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ShowMainWindow,
    QuitApp,
}

#[cfg(target_os = "macos")]
mod macos_native {
    use std::collections::VecDeque;
    use std::sync::{Mutex, OnceLock};

    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{AllocAnyThread, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSImage, NSImageScaling, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
        NSVariableStatusItemLength,
    };
    use objc2_foundation::{MainThreadMarker, NSData, NSObject, NSSize, NSString};

    use super::{APP_ICON_PNG, TRAY_ICON_PNG, TrayAction, t};

    static TRAY_ACTION_QUEUE: OnceLock<Mutex<VecDeque<TrayAction>>> = OnceLock::new();

    fn action_queue() -> &'static Mutex<VecDeque<TrayAction>> {
        TRAY_ACTION_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
    }

    pub(super) fn push_action(action: TrayAction) {
        if let Ok(mut queue) = action_queue().lock() {
            queue.push_back(action);
        }
    }

    pub fn drain_actions() -> Vec<TrayAction> {
        let Ok(mut queue) = action_queue().lock() else {
            return Vec::new();
        };
        queue.drain(..).collect()
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "SwivoxTrayMenuTarget"]
        pub struct SwivoxTrayMenuTarget;

        impl SwivoxTrayMenuTarget {
            #[unsafe(method(onShowMainWindow:))]
            fn on_show_main_window(&self, _sender: Option<&AnyObject>) {
                push_action(TrayAction::ShowMainWindow);
            }

            #[unsafe(method(onQuitApp:))]
            fn on_quit_app(&self, _sender: Option<&AnyObject>) {
                push_action(TrayAction::QuitApp);
            }

            #[unsafe(method(validateMenuItem:))]
            fn validate_menu_item(&self, _item: &NSMenuItem) -> bool {
                true
            }
        }
    );

    impl SwivoxTrayMenuTarget {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            unsafe { msg_send![mtm.alloc::<Self>(), init] }
        }
    }

    pub struct ManagedTray {
        status_item: Retained<NSStatusItem>,
        _menu: Retained<NSMenu>,
        show_item: Retained<NSMenuItem>,
        quit_item: Retained<NSMenuItem>,
        _target: Retained<SwivoxTrayMenuTarget>,
        mtm: MainThreadMarker,
    }

    impl ManagedTray {
        pub fn install() -> anyhow::Result<Self> {
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| anyhow::anyhow!("macOS status item must be created on main thread"))?;

            // 运行时同步更新 Dock 应用图标（无边框 Apple 连续曲率圆角图标），消除系统图标缓存滞后。
            let app_icon_data = NSData::with_bytes(APP_ICON_PNG);
            if let Some(app_icon) = NSImage::initWithData(NSImage::alloc(), &app_icon_data) {
                unsafe {
                    NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&app_icon));
                }
            }

            let status_bar = NSStatusBar::systemStatusBar();
            let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
            status_item.setAutosaveName(Some(&NSString::from_str("Swivox.StatusItem")));
            status_item.setVisible(true);

            // 使用 32x32 (@2x 对应 16x16pt 画布，主体视觉高度 12pt) 纯透明底单色模板图元，
            // 开启 setTemplate(true) 并使用 ScaleProportionallyDown（禁止放大拉伸），实现无背景色、自适应深浅色菜单栏且尺寸与系统原生图标对齐。
            let nsdata = NSData::with_bytes(TRAY_ICON_PNG);
            let nsimage = NSImage::initWithData(NSImage::alloc(), &nsdata)
                .ok_or_else(|| anyhow::anyhow!("failed to decode Swivox tray PNG into NSImage"))?;
            nsimage.setSize(NSSize::new(16.0, 16.0));
            nsimage.setTemplate(true);

            let button = status_item
                .button(mtm)
                .ok_or_else(|| anyhow::anyhow!("NSStatusItem missing NSStatusBarButton"))?;
            button.setBordered(false);
            button.setImage(Some(&nsimage));
            button.setImageScaling(NSImageScaling::ScaleProportionallyDown);
            button.setToolTip(Some(&NSString::from_str("Swivox")));

            let target = SwivoxTrayMenuTarget::new(mtm);
            let menu = NSMenu::initWithTitle(mtm.alloc(), &NSString::from_str("Swivox"));
            menu.setAutoenablesItems(false);

            let show_item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    mtm.alloc(),
                    &NSString::from_str(&t("显示主窗口")),
                    Some(sel!(onShowMainWindow:)),
                    &NSString::from_str(""),
                )
            };
            unsafe {
                show_item.setTarget(Some(&target));
            }
            show_item.setEnabled(true);
            menu.addItem(&show_item);

            let separator = NSMenuItem::separatorItem(mtm);
            menu.addItem(&separator);

            let quit_item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    mtm.alloc(),
                    &NSString::from_str(&t("退出程序")),
                    Some(sel!(onQuitApp:)),
                    &NSString::from_str(""),
                )
            };
            unsafe {
                quit_item.setTarget(Some(&target));
            }
            quit_item.setEnabled(true);
            menu.addItem(&quit_item);

            status_item.setMenu(Some(&menu));

            Ok(Self {
                status_item,
                _menu: menu,
                show_item,
                quit_item,
                _target: target,
                mtm,
            })
        }

        pub fn rect_debug(&self) -> Option<String> {
            let visible = self.status_item.isVisible();
            let autosave = self.status_item.autosaveName();
            if let Some(button) = self.status_item.button(self.mtm) {
                let btn_frame = button.frame();
                let win_frame = button.window().map(|w| w.frame());
                Some(format!(
                    "visible={visible}, autosave={autosave}, button_frame={btn_frame:?}, window_frame={win_frame:?}"
                ))
            } else {
                Some(format!("visible={visible}, autosave={autosave}, button=None"))
            }
        }

        pub fn refresh_labels(&self) {
            self.show_item
                .setTitle(&NSString::from_str(&t("显示主窗口")));
            self.quit_item
                .setTitle(&NSString::from_str(&t("退出程序")));
        }
    }

    impl Drop for ManagedTray {
        fn drop(&mut self) {
            NSStatusBar::systemStatusBar().removeStatusItem(&self.status_item);
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos_native::ManagedTray;

#[cfg(target_os = "macos")]
pub fn poll_tray_actions() -> Vec<TrayAction> {
    macos_native::drain_actions()
}

#[cfg(not(target_os = "macos"))]
mod cross_platform {
    use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
    use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::{TRAY_MENU_QUIT, TRAY_MENU_SHOW, TrayAction, decode_brand_tray_rgba, t};

    pub struct ManagedTray {
        _tray: TrayIcon,
        show_item: MenuItem,
        quit_item: MenuItem,
    }

    impl ManagedTray {
        pub fn install() -> anyhow::Result<Self> {
            let show_item = MenuItem::with_id(TRAY_MENU_SHOW, t("显示主窗口"), true, None);
            let quit_item = MenuItem::with_id(TRAY_MENU_QUIT, t("退出程序"), true, None);
            let tray_menu = Menu::with_items(&[&show_item, &quit_item])?;
            let (mut rgba, width, height) = decode_brand_tray_rgba()?;
            // 非 macOS 平台无系统级模板染色，将透明底单色 Alpha 图元染为高对比度亮白以兼容深色任务栏。
            for px in rgba.chunks_exact_mut(4) {
                if px[3] > 0 {
                    px[0] = 248;
                    px[1] = 250;
                    px[2] = 255;
                }
            }
            let icon = Icon::from_rgba(rgba, width, height)?;

            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_tooltip("Swivox")
                .with_icon(icon)
                .with_menu_on_left_click(true)
                .build()?;

            Ok(Self {
                _tray: tray,
                show_item,
                quit_item,
            })
        }

        pub fn rect_debug(&self) -> Option<String> {
            self._tray.rect().map(|r| format!("{r:?}"))
        }

        pub fn refresh_labels(&self) {
            self.show_item.set_text(t("显示主窗口"));
            self.quit_item.set_text(t("退出程序"));
        }
    }

    pub fn poll_tray_actions() -> Vec<TrayAction> {
        let mut actions = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == MenuId::new(TRAY_MENU_SHOW) {
                actions.push(TrayAction::ShowMainWindow);
            } else if event.id == MenuId::new(TRAY_MENU_QUIT) {
                actions.push(TrayAction::QuitApp);
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                actions.push(TrayAction::ShowMainWindow);
            }
        }
        actions
    }
}

#[cfg(not(target_os = "macos"))]
pub use cross_platform::{ManagedTray, poll_tray_actions};

#[cfg(any(not(target_os = "macos"), test))]
pub fn decode_brand_tray_rgba() -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(TRAY_ICON_PNG));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let bytes = &buf[..info.buffer_size()];

    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in bytes.chunks_exact(3) {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            out
        }
        other => anyhow::bail!("unsupported tray icon color type: {other:?}"),
    };
    Ok((rgba, info.width, info.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_is_compact_transparent_monochrome_template() {
        let (rgba, width, height) = decode_brand_tray_rgba().unwrap();
        assert_eq!(width, 32);
        assert_eq!(height, 32);
        // 四角与中心镂空区域必须完全透明（不得包含任何背景底色或杂色光晕）
        for &(x, y) in &[(0usize, 0usize), (31, 0), (0, 31), (31, 31), (16, 16)] {
            let idx = (y * (width as usize) + x) * 4;
            assert_eq!(rgba[idx + 3], 0, "point ({x}, {y}) must be transparent");
        }
        // 所有可见像素必须为纯单色模板通道（RGB = 0,0,0），且留有充足菜单栏光学内边距（四周 >= 4px）
        let mut min_x = width as usize;
        let mut max_x = 0usize;
        let mut min_y = height as usize;
        let mut max_y = 0usize;
        for y in 0..(height as usize) {
            for x in 0..(width as usize) {
                let p = &rgba[(y * (width as usize) + x) * 4..][..4];
                if p[3] > 0 {
                    assert_eq!((p[0], p[1], p[2]), (0, 0, 0), "non-template color at ({x}, {y})");
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }
        assert!(min_x >= 4 && max_x <= 27, "unexpected x bounds: {min_x}..={max_x}");
        assert!(min_y >= 4 && max_y <= 28, "unexpected y bounds: {min_y}..={max_y}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_tray_action_queue_drains_in_fifo_order() {
        let _ = poll_tray_actions();
        macos_native::push_action(TrayAction::ShowMainWindow);
        macos_native::push_action(TrayAction::QuitApp);
        assert_eq!(
            poll_tray_actions(),
            vec![TrayAction::ShowMainWindow, TrayAction::QuitApp]
        );
        assert!(poll_tray_actions().is_empty());
    }
}
