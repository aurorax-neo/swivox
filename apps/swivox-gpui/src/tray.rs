use std::io::Cursor;

use swivox_backend::i18n::t;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const TRAY_MENU_SHOW: &str = "tray_show_main";
pub const TRAY_MENU_QUIT: &str = "tray_quit_app";

const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ShowMainWindow,
    QuitApp,
}

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
        let icon = decode_brand_tray_icon()?;

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Swivox")
            .with_icon(icon)
            .with_menu_on_left_click(true);

        #[cfg(target_os = "macos")]
        {
            // 保留品牌渐变云朵色彩，禁用黑白模板模式。
            builder = builder.with_icon_as_template(false);
        }

        let tray = builder.build()?;
        Ok(Self {
            _tray: tray,
            show_item,
            quit_item,
        })
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

pub fn decode_brand_tray_rgba() -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(Cursor::new(TRAY_ICON_PNG));
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

fn decode_brand_tray_icon() -> anyhow::Result<Icon> {
    let (rgba, width, height) = decode_brand_tray_rgba()?;
    Ok(Icon::from_rgba(rgba, width, height)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_preserves_brand_colour_and_transparency() {
        let (rgba, width, height) = decode_brand_tray_rgba().unwrap();
        assert_eq!(width, height);
        assert!(width > 0);
        assert!(
            rgba.chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[2] > pixel[0])
        );
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
    }
}
