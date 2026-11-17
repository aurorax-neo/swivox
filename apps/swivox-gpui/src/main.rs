#![cfg_attr(windows, windows_subsystem = "windows")]

mod routes;
mod self_capture;
mod shell;
mod store;
mod tray;
mod views;
mod widgets;

use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use swivox_backend::CliInterceptOutcome;
use swivox_backend::i18n::{self, Language};
use swivox_backend::ui_prefs::{self, UiThemeMode};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::*;

use crate::shell::ManagerShell;
use crate::store::AppStore;
use crate::tray::{ManagedTray, TrayAction, poll_tray_actions};

#[derive(Clone, Copy)]
enum ExternalWakeEvent {
    FocusAndConsumeNavigation,
    RuntimeHealthChanged,
}

fn build_main_window_options(cx: &App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(1180.), px(820.)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(960.), px(720.))),
        titlebar: Some(TitlebarOptions {
            title: Some("Swivox".into()),
            ..Default::default()
        }),
        focus: true,
        show: true,
        ..Default::default()
    }
}

fn open_or_focus_main_window(
    slot: &mut Option<AnyWindowHandle>,
    store: &Entity<AppStore>,
    cx: &mut App,
) {
    cx.activate(true);
    if let Some(existing) = *slot
        && existing
            .update(cx, |_, window, _| {
                window.activate_window();
            })
            .is_ok()
    {
        return;
    }

    let store_for_window = store.clone();
    if let Ok((handle, _)) = gpui_kit::open_window(build_main_window_options(cx), cx, |window, cx| {
        cx.new(|cx| ManagerShell::new(store_for_window, window, cx))
    }) {
        *slot = Some(handle);
    }
}

fn main() {
    #[cfg(feature = "visual-audit")]
    if let Ok(out_dir) = std::env::var("SWIVOX_VISUAL_AUDIT_DIR")
        && !out_dir.trim().is_empty()
    {
        visual_audit::run_headless_visual_audit(std::path::Path::new(&out_dir));
        return;
    }

    if let CliInterceptOutcome::Handled(code) = swivox_backend::handle_cli_intercepts() {
        std::process::exit(code);
    }

    swivox_backend::install_panic_logger();

    let Some(_instance_guard) = swivox_backend::acquire_single_instance_guard() else {
        return;
    };

    swivox_backend::run_startup_housekeeping();

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start tokio runtime");
    let tokio_handle = tokio_runtime.handle().clone();

    let prefs = ui_prefs::load_ui_preferences();
    i18n::set_language(prefs.language);

    let (wake_tx, wake_rx) = mpsc::channel::<ExternalWakeEvent>();

    // 注册悬浮球/外部导航请求回调
    let nav_tx = wake_tx.clone();
    swivox_launcher::set_navigation_handler(move |_payload| {
        let _ = nav_tx.send(ExternalWakeEvent::FocusAndConsumeNavigation);
        Ok(())
    });

    // 监听单实例唤醒文件请求
    let file_wake_tx = wake_tx.clone();
    std::thread::spawn(move || {
        let path = swivox_backend::window_activation_request_path();
        let mut previous = std::fs::read(&path).unwrap_or_default();
        while !swivox_backend::shutdown_started() {
            std::thread::sleep(Duration::from_millis(300));
            let current = std::fs::read(&path).unwrap_or_default();
            if !current.is_empty() && current != previous {
                previous = current;
                let _ = file_wake_tx.send(ExternalWakeEvent::FocusAndConsumeNavigation);
            }
        }
    });

    // 恢复已有 Codex 会话并在完成后通知刷新健康度
    let resume_tx = wake_tx.clone();
    swivox_backend::commands::resume_existing_codex_background(move || {
        let _ = resume_tx.send(ExternalWakeEvent::RuntimeHealthChanged);
    });

    // 自动接管官方 App 守护（内部根据开关决定是否启动）
    swivox_backend::auto_takeover::start_supervisor();

    // 启动 Antigravity CDP 动态增强守护与 Lamplighter 后台监控/自动点火守护
    swivox_core::antigravity::ensure_background_enhance_supervisor(tokio_handle.clone());
    swivox_core::lamplighter::ensure_lamplighter_engine_started(tokio_handle.clone());

    let reopen_tx = wake_tx.clone();
    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.on_reopen(move |_cx| {
        let _ = reopen_tx.send(ExternalWakeEvent::FocusAndConsumeNavigation);
    });
    app.run(move |cx| {
            gpui_kit::init(cx);

            let initial_theme = match prefs.theme {
                UiThemeMode::Dark => ThemeMode::Dark,
                UiThemeMode::Light => ThemeMode::Light,
            };
            Theme::change(initial_theme, None, cx);
            widgets::apply_cjk_typography(cx);

            let store = cx.new(|cx| {
                let mut s = AppStore::new(tokio_handle.clone(), prefs.clone());
                s.initialize(cx);
                s
            });

            let mut main_window: Option<AnyWindowHandle> = None;
            open_or_focus_main_window(&mut main_window, &store, cx);

            let window_cell = Rc::new(std::cell::RefCell::new(main_window));

            // 应用退出时执行优雅停机
            cx.on_app_quit(|_cx| {
                std::thread::spawn(swivox_backend::run_graceful_shutdown);
                async move {}
            })
            .detach();

            // 若设置了 SWIVOX_CAPTURE_TOUR_DIR，则在窗口就绪后自动巡检各路由并导出实拍截图供 UI 走查
            if let Ok(tour_dir) = std::env::var("SWIVOX_CAPTURE_TOUR_DIR")
                && !tour_dir.trim().is_empty()
            {
                let tour_store = store.clone();
                let tour_window = window_cell.clone();
                let tour_path = std::path::PathBuf::from(tour_dir);
                cx.spawn(async move |cx| {
                    // 先等待首屏异步数据与字体/GPU 帧稳定
                    cx.background_executor()
                        .timer(Duration::from_millis(900))
                        .await;

                    let steps: &[(&str, fn(&mut AppStore, &mut Context<AppStore>))] = &[
                        ("01_overview.png", |s, cx| s.navigate(crate::routes::Route::Overview, cx)),
                        ("02_official_accounts.png", |s, cx| {
                            s.navigate(crate::routes::Route::OfficialAccounts, cx)
                        }),
                        ("03_relay_list.png", |s, cx| {
                            s.back_to_relay_list(cx);
                            s.navigate(crate::routes::Route::Relay, cx);
                        }),
                        ("04_multi_launch.png", |s, cx| {
                            s.navigate(crate::routes::Route::MultiLaunch, cx)
                        }),
                        ("05_mcp.png", |s, cx| s.navigate(crate::routes::Route::Context, cx)),
                        ("06_skills.png", |s, cx| s.navigate(crate::routes::Route::Skills, cx)),
                        ("07_sessions.png", |s, cx| {
                            s.navigate(crate::routes::Route::Sessions, cx)
                        }),
                        ("08_user_scripts.png", |s, cx| {
                            s.navigate(crate::routes::Route::UserScripts, cx)
                        }),
                        ("09_enhance.png", |s, cx| s.navigate(crate::routes::Route::Enhance, cx)),
                        ("10_plugin_market.png", |s, cx| {
                            s.navigate(crate::routes::Route::PluginMarket, cx)
                        }),
                        ("11_agent_cache.png", |s, cx| {
                            s.navigate(crate::routes::Route::AgentCache, cx)
                        }),
                        ("12_maintenance.png", |s, cx| {
                            s.navigate(crate::routes::Route::Maintenance, cx)
                        }),
                        ("13_about.png", |s, cx| s.navigate(crate::routes::Route::About, cx)),
                        ("14_settings.png", |s, cx| {
                            s.navigate(crate::routes::Route::Settings, cx)
                        }),
                        ("15_relay_env.png", |s, cx| {
                            s.navigate(crate::routes::Route::RelayEnvironment, cx)
                        }),
                        ("16_light_en_overview.png", |s, cx| {
                            if s.prefs.theme != UiThemeMode::Light {
                                s.toggle_theme(cx);
                            }
                            if s.prefs.language != Language::En {
                                s.toggle_language(cx);
                            }
                            s.navigate(crate::routes::Route::Overview, cx);
                        }),
                        ("17_restore_overview.png", |s, cx| {
                            if s.prefs.theme != UiThemeMode::Dark {
                                s.toggle_theme(cx);
                            }
                            if s.prefs.language != Language::Zh {
                                s.toggle_language(cx);
                            }
                            s.navigate(crate::routes::Route::Overview, cx);
                        }),
                    ];

                    for (filename, apply_step) in steps {
                        cx.update(|app| {
                            app.activate(true);
                            tour_store.update(app, |s, cx| apply_step(s, cx));
                            if let Some(win) = *tour_window.borrow() {
                                let _ = win.update(app, |_, window, _| {
                                    window.activate_window();
                                    window.refresh();
                                });
                            }
                            app.refresh_windows();
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(750))
                            .await;
                        let out = tour_path.join(filename);
                        let _ = crate::self_capture::capture_own_main_window_png(&out);
                    }
                })
                .detach();
            }

            // 主线程轻量轮询托盘事件、语言变更及跨线程唤醒信号（延后至首轮 RunLoop 完成后挂载托盘，确保 Regular 激活态已在 MenuBarAgent 就绪）
            {
                let window_cell = window_cell.clone();
                let store = store.clone();
                let mut tray: Option<Rc<ManagedTray>> = None;
                let mut tray_install_attempted = false;
                let mut last_lang: Language = prefs.language;
                let mut logged_settled_tray_rect = false;
                let mut poll_ticks: u32 = 0;

                cx.spawn(async move |cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(120))
                            .await;
                        poll_ticks = poll_ticks.saturating_add(1);

                        let should_quit = cx.update(|app| {
                            if !tray_install_attempted {
                                tray_install_attempted = true;
                                match ManagedTray::install() {
                                    Ok(installed) => {
                                        let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                                            "manager.tray_installed",
                                            serde_json::json!({ "rect": installed.rect_debug() }),
                                        );
                                        tray = Some(Rc::new(installed));
                                    }
                                    Err(err) => {
                                        let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                                            "manager.tray_install_failed",
                                            serde_json::json!({ "error": err.to_string() }),
                                        );
                                    }
                                }
                            }

                            if let Some(tray) = &tray {
                                if !logged_settled_tray_rect && poll_ticks >= 10 {
                                    logged_settled_tray_rect = true;
                                    let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                                        "manager.tray_rect_settled",
                                        serde_json::json!({ "rect": tray.rect_debug() }),
                                    );
                                }
                                let cur_lang = store.read(app).prefs.language;
                                if cur_lang != last_lang {
                                    last_lang = cur_lang;
                                    tray.refresh_labels();
                                }
                            }

                            for action in poll_tray_actions() {
                                match action {
                                    TrayAction::ShowMainWindow => {
                                        open_or_focus_main_window(
                                            &mut window_cell.borrow_mut(),
                                            &store,
                                            app,
                                        );
                                    }
                                    TrayAction::QuitApp => {
                                        swivox_backend::run_graceful_shutdown();
                                        app.quit();
                                        return true;
                                    }
                                }
                            }

                            while let Ok(wake) = wake_rx.try_recv() {
                                match wake {
                                    ExternalWakeEvent::FocusAndConsumeNavigation => {
                                        open_or_focus_main_window(
                                            &mut window_cell.borrow_mut(),
                                            &store,
                                            app,
                                        );
                                        store.update(app, |s, cx| s.consume_pending_navigation(cx));
                                    }
                                    ExternalWakeEvent::RuntimeHealthChanged => {
                                        store.update(app, |s, cx| s.refresh_overview(false, cx));
                                    }
                                }
                            }

                            false
                        });

                        if should_quit {
                            break;
                        }
                    }
                })
                .detach();
            }
        });

    swivox_backend::run_graceful_shutdown();
}

#[cfg(feature = "visual-audit")]
mod visual_audit {
    use std::fs::File;
    use std::io::BufWriter;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use swivox_backend::i18n::{self, Language};
    use swivox_backend::ui_prefs::{UiPreferences, UiThemeMode};
    use gpui_kit::component::theme::{Theme, ThemeMode};
    use gpui_kit::{AppContext as _, HeadlessAppContext, px, size};

    use crate::routes::Route;
    use crate::shell::ManagerShell;
    use crate::store::AppStore;
    use crate::widgets;

    fn save_rgba_png(
        path: &Path,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut png_writer = encoder.write_header()?;
        png_writer.write_image_data(rgba)?;
        Ok(())
    }

    pub fn run_headless_visual_audit(out_dir: &Path) {
        let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let tokio_handle = tokio_runtime.handle().clone();

        let text_system = gpui_kit::platform::current_platform(true).text_system();
        let mut hcx = HeadlessAppContext::with_platform(
            text_system,
            Arc::new(gpui_kit::assets::AllAssets),
            gpui_kit::platform::current_headless_renderer,
        );
        hcx.allow_parking();

        i18n::set_language(Language::Zh);
        let initial_prefs = UiPreferences {
            theme: UiThemeMode::Dark,
            language: Language::Zh,
        };

        let store = hcx.update(|app| {
            gpui_kit::init(app);
            Theme::change(ThemeMode::Dark, None, app);
            widgets::apply_cjk_typography(app);
            app.new(|cx| {
                let mut s = AppStore::new(tokio_handle.clone(), initial_prefs);
                s.initialize(cx);
                s
            })
        });

        let store_for_win = store.clone();
        let window_handle = hcx
            .open_window(size(px(1140.0), px(820.0)), |window, app| {
                app.new(|cx| ManagerShell::new(store_for_win, window, cx))
            })
            .expect("open headless window");
        let any_win = window_handle.into();

        // 等待初始化异步加载落地
        std::thread::sleep(Duration::from_millis(450));
        hcx.run_until_parked();

        type StepFn = Box<dyn FnOnce(&mut HeadlessAppContext, &gpui_kit::Entity<AppStore>)>;
        let steps: Vec<(&str, StepFn)> = vec![
            (
                "01_overview.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.clear_notice(cx);
                            s.navigate(Route::Overview, cx);
                        });
                    });
                }),
            ),
            (
                "01b_floating_toast.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Overview, cx);
                            s.set_notice(
                                i18n::t("切换官方账号"),
                                i18n::t("已切换官方账号（重启 Codex 后生效）。"),
                                "ok",
                                cx,
                            );
                        });
                    });
                }),
            ),
            (
                "02_official_accounts.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.clear_notice(cx);
                            s.navigate(Route::OfficialAccounts, cx);
                        })
                    });
                }),
            ),
            (
                "03a_relay_list.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.back_to_relay_list(cx);
                            s.navigate(Route::Relay, cx);
                        });
                    });
                }),
            ),
            (
                "03a2_relay_switch_modal.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.back_to_relay_list(cx);
                            s.navigate(Route::Relay, cx);
                            if let Some(first) = s.settings.relay_profiles.first() {
                                s.use_confirm_relay_id = Some(first.id.clone());
                                cx.notify();
                            }
                        });
                    });
                }),
            ),
            (
                "03b_relay_catalog.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.navigate(Route::Relay, cx);
                            s.open_relay_catalog(cx);
                        });
                    });
                }),
            ),
            (
                "03c_relay_editor.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Relay, cx);
                            s.open_new_preset_relay(0, cx);
                        });
                    });
                }),
            ),
            (
                "04_multi_launch.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::MultiLaunch, cx)));
                }),
            ),
            (
                "05a_mcp.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.mcp_editor_open = false;
                            s.navigate(Route::Context, cx);
                        });
                    });
                }),
            ),
            (
                "05b_mcp_editor.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Context, cx);
                            s.open_mcp_editor(
                                "fetch".to_string(),
                                "command = \"uvx\"\nargs = [\"mcp-server-fetch\"]\n".to_string(),
                                true,
                                cx,
                            );
                        });
                    });
                }),
            ),
            (
                "06a_skills.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.skills_repos_open = false;
                            s.navigate(Route::Skills, cx);
                        });
                    });
                }),
            ),
            (
                "06b_skills_repos.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Skills, cx);
                            s.skills_repos_open = true;
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "07_sessions.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::Sessions, cx)));
                }),
            ),
            (
                "08_user_scripts.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::UserScripts, cx)));
                }),
            ),
            (
                "09a_enhance_general.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.enhance_tab = "general";
                            s.navigate(Route::Enhance, cx);
                        });
                    });
                }),
            ),
            (
                "09b_enhance_stepwise.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Enhance, cx);
                            s.enhance_tab = "stepwise";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "09c_enhance_dictation.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Enhance, cx);
                            s.enhance_tab = "dictation";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "09d_enhance_overlay.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Enhance, cx);
                            s.enhance_tab = "overlay";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "09e_enhance_browser.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Enhance, cx);
                            s.enhance_tab = "browser";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "10_plugin_market.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| s.navigate(Route::PluginMarket, cx))
                    });
                }),
            ),
            (
                "11_agent_cache.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::AgentCache, cx)));
                }),
            ),
            (
                "12_maintenance.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::Maintenance, cx)));
                }),
            ),
            (
                "13_about.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::About, cx)));
                }),
            ),
            (
                "14_settings.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.navigate(Route::Settings, cx);
                        });
                    });
                }),
            ),
            (
                "14b_settings_reset_modal.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Settings, cx);
                            s.reset_confirm_open = true;
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "15_relay_env.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.navigate(Route::RelayEnvironment, cx);
                        })
                    });
                }),
            ),
            (
                "19a_ag_overview.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::Overview, cx);
                        });
                    });
                }),
            ),
            (
                "19b_ag_accounts.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::AntigravityAccounts, cx);
                        });
                    });
                }),
            ),
            (
                "19b2_ag_switch_modal.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::AntigravityAccounts, cx);
                            let sample = s
                                .ag_snapshot
                                .as_ref()
                                .and_then(|sp| sp.accounts.first().cloned())
                                .unwrap_or(swivox_core::antigravity::AntigravityAccountView {
                                    id: "demo-ag".to_string(),
                                    email: "studio-pro@google.com".to_string(),
                                    name: Some("Studio Pro".to_string()),
                                    label: Some("主力 AI Pro".to_string()),
                                    is_active: true,
                                    is_live_desktop: false,
                                    is_live_ide: false,
                                    disabled: false,
                                    disabled_reason: None,
                                    subscription_tier: "PRO".to_string(),
                                    oauth_client_key: "personal".to_string(),
                                    project_id: None,
                                    proxy_mode: "inherit".to_string(),
                                    egress_proxy_url: None,
                                    device_profile: None,
                                    quota: None,
                                    created_at: 0,
                                    last_used: 0,
                                });
                            s.ag_use_confirm_account = Some(sample);
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "19c_ag_enhance.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::AntigravityEnhance, cx);
                        });
                    });
                }),
            ),
            (
                "20a_lamplighter_overview.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.active_tool = "codex".to_string();
                            s.lamplighter_tab = "overview";
                            s.navigate(Route::Lamplighter, cx);
                        });
                    });
                }),
            ),
            (
                "20b_lamplighter_history.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Lamplighter, cx);
                            s.lamplighter_tab = "history";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "20c_lamplighter_settings.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Lamplighter, cx);
                            s.lamplighter_tab = "settings";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "20d_lamplighter_antigravity.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.lamplighter_tab = "overview";
                            s.navigate(Route::Lamplighter, cx);
                        });
                    });
                }),
            ),
            (
                "20e_lamplighter_ag_settings.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.lamplighter_tab = "settings";
                            s.navigate(Route::Lamplighter, cx);
                        });
                    });
                }),
            ),
            (
                "en_01_overview.png",
                Box::new(|hcx, store| {
                    i18n::set_language(Language::En);
                    hcx.update(|app| {
                        Theme::change(ThemeMode::Light, None, app);
                        widgets::apply_cjk_typography(app);
                        store.update(app, |s, cx| {
                            s.prefs.theme = UiThemeMode::Light;
                            s.prefs.language = Language::En;
                            s.active_tool = "codex".to_string();
                            s.navigate(Route::Overview, cx);
                        });
                    });
                }),
            ),
            (
                "en_02_official_accounts.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::OfficialAccounts, cx);
                        });
                    });
                }),
            ),
            (
                "en_03a_relay_list.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.back_to_relay_list(cx);
                            s.navigate(Route::Relay, cx);
                        });
                    });
                }),
            ),
            (
                "en_03b_relay_catalog.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Relay, cx);
                            s.open_relay_catalog(cx);
                        });
                    });
                }),
            ),
            (
                "en_03c_relay_editor.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Relay, cx);
                            s.open_new_preset_relay(0, cx);
                        });
                    });
                }),
            ),
            (
                "en_04_multi_launch.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::MultiLaunch, cx)));
                }),
            ),
            (
                "en_05b_mcp_editor.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Context, cx);
                            s.open_mcp_editor(
                                "fetch".to_string(),
                                "command = \"uvx\"\nargs = [\"mcp-server-fetch\"]\n".to_string(),
                                true,
                                cx,
                            );
                        });
                    });
                }),
            ),
            (
                "en_06b_skills_repos.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Skills, cx);
                            s.skills_repos_open = true;
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "en_07_sessions.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::Sessions, cx)));
                }),
            ),
            (
                "en_08_user_scripts.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::UserScripts, cx)));
                }),
            ),
            (
                "en_09a_enhance_general.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.enhance_tab = "general";
                            s.navigate(Route::Enhance, cx);
                        });
                    });
                }),
            ),
            (
                "en_09b_enhance_stepwise.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Enhance, cx);
                            s.enhance_tab = "stepwise";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "en_09c_enhance_dictation.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.navigate(Route::Enhance, cx);
                            s.enhance_tab = "dictation";
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "en_10_plugin_market.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| s.navigate(Route::PluginMarket, cx))
                    });
                }),
            ),
            (
                "en_11_agent_cache.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::AgentCache, cx)));
                }),
            ),
            (
                "en_12_maintenance.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::Maintenance, cx)));
                }),
            ),
            (
                "en_13_about.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::About, cx)));
                }),
            ),
            (
                "en_14_settings.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| store.update(app, |s, cx| s.navigate(Route::Settings, cx)));
                }),
            ),
            (
                "en_19a_ag_overview.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::Overview, cx);
                        });
                    });
                }),
            ),
            (
                "en_19b_ag_accounts.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::AntigravityAccounts, cx);
                        });
                    });
                }),
            ),
            (
                "en_19b2_ag_switch_modal.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::AntigravityAccounts, cx);
                            if let Some(first) =
                                s.ag_snapshot.as_ref().and_then(|sp| sp.accounts.first().cloned())
                            {
                                s.ag_use_confirm_account = Some(first);
                                cx.notify();
                            }
                        });
                    });
                }),
            ),
            (
                "en_19c_ag_enhance.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::AntigravityEnhance, cx);
                        });
                    });
                }),
            ),
            (
                "en_20a_ll_codex_overview.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "codex".to_string();
                            s.lamplighter_tab = "overview";
                            s.navigate(Route::Lamplighter, cx);
                        });
                    });
                }),
            ),
            (
                "en_20b_ll_codex_settings.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "codex".to_string();
                            s.lamplighter_tab = "settings";
                            s.navigate(Route::Lamplighter, cx);
                        });
                    });
                }),
            ),
            (
                "en_20c_ll_ag_settings.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.lamplighter_tab = "settings";
                            s.navigate(Route::Lamplighter, cx);
                        });
                    });
                }),
            ),
            (
                "en_20d_ll_ag_overview.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.lamplighter_tab = "overview";
                            s.navigate(Route::Lamplighter, cx);
                        });
                    });
                }),
            ),
            (
                "en_21a_confirm_ignite_modal.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.active_tool = "antigravity".to_string();
                            s.lamplighter_tab = "overview";
                            s.navigate(Route::Lamplighter, cx);
                            s.request_action_confirm(
                                crate::store::PendingActionConfirm::ManualIgniteLamplighter {
                                    group_key: "antigravity:demo:gemini".to_string(),
                                    display_label: "Antigravity#Tarik Marques · Gemini".to_string(),
                                    effective_proxy: "http://127.0.0.1:7890".to_string(),
                                },
                                cx,
                            );
                        });
                    });
                }),
            ),
            (
                "21b_zh_confirm_stop_ag_modal.png",
                Box::new(|hcx, store| {
                    i18n::set_language(Language::Zh);
                    hcx.update(|app| {
                        Theme::change(ThemeMode::Dark, None, app);
                        widgets::apply_cjk_typography(app);
                        store.update(app, |s, cx| {
                            s.prefs.theme = UiThemeMode::Dark;
                            s.prefs.language = Language::Zh;
                            s.dismiss_all_confirm_modals(cx);
                            s.active_tool = "antigravity".to_string();
                            s.navigate(Route::Overview, cx);
                            s.request_action_confirm(
                                crate::store::PendingActionConfirm::StopAntigravity {
                                    target: "desktop".to_string(),
                                },
                                cx,
                            );
                        });
                    });
                }),
            ),
            (
                "22a_zh_relay_editor_closed.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.dismiss_all_confirm_modals(cx);
                            s.active_tool = "codex".to_string();
                            s.relay_audit_dropdown = None;
                            s.navigate(Route::Relay, cx);
                            if let Some(first_id) =
                                s.settings.relay_profiles.first().map(|p| p.id.clone())
                            {
                                s.open_edit_relay(first_id, cx);
                            }
                        });
                    });
                }),
            ),
            (
                "22b_zh_relay_mode_dropdown.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.relay_audit_dropdown = Some("mode");
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "22c_zh_relay_proto_dropdown.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.relay_audit_dropdown = Some("proto");
                            cx.notify();
                        });
                    });
                }),
            ),
            (
                "22d_zh_official_bind_dropdown.png",
                Box::new(|hcx, store| {
                    hcx.update(|app| {
                        store.update(app, |s, cx| {
                            s.relay_audit_dropdown = None;
                            s.navigate(Route::OfficialAccounts, cx);
                            if let Some(acc_id) = s
                                .official_snapshot
                                .as_ref()
                                .and_then(|sp| sp.accounts.first().map(|a| a.id.clone()))
                            {
                                s.open_binding_dropdown_for = Some(acc_id);
                                cx.notify();
                            }
                        });
                    });
                }),
            ),
        ];

        for (filename, step_fn) in steps {
            step_fn(&mut hcx, &store);
            std::thread::sleep(Duration::from_millis(280));
            hcx.run_until_parked();

            hcx.update_window(any_win, |_, window, cx| {
                let _ = window.draw(cx);
            })
            .expect("first draw");
            hcx.run_until_parked();
            let img = hcx
                .update_window(any_win, |_, window, cx| {
                    let _ = window.draw(cx);
                    window.render_to_image()
                })
                .expect("update window")
                .expect("render_to_image");

            let out_path = out_dir.join(filename);
            save_rgba_png(&out_path, img.width(), img.height(), img.as_raw())
                .expect("save png");
            println!("captured {} ({}x{})", out_path.display(), img.width(), img.height());
        }

        i18n::set_language(Language::Zh);
    }
}
