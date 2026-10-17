//! The resident mode: the core service, the IPC server for the UI, and the
//! tray icon with its own native menu (opens at once, without the UI:
//! profiles to connect, disconnect, open the app, exit). Left click opens
//! the UI's flyout.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use a2dpwb_core::{Service, ServiceOptions};
use bt_host::HciCapture;
use ipc::{Event, LinkSample};
use tao::event::{Event as TaoEvent, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::state::{CoreState, TrayView};

/// Instance guard of v2's core (per user session)
const CORE_MUTEX: &str = "Local\\A2DPWB_v2_Core";
/// Profiles listed in the tray menu (as v1)
const TRAY_PROFILES: usize = 10;

enum UserEvent {
    Tray(TrayView),
    Menu(String),
    Click,
    Quit,
}

/// Starts the UI (a2dpwb.exe next to us), optionally its flyout
fn launch_ui(flyout: bool) {
    let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) else { return };
    let ui = dir.join("a2dpwb.exe");
    if !ui.exists() {
        tracing::warn!("UI not found: {}", ui.display());
        return;
    }
    let mut cmd = std::process::Command::new(ui);
    if flyout {
        cmd.arg("--flyout");
    }
    if let Err(e) = cmd.spawn() {
        tracing::warn!("cannot start the UI: {e}");
    }
}

fn tr(lang: &str, key: &str, default: &str) -> String {
    a2dpwb_core::messages::translate(lang, key, default)
}

fn build_menu(view: &TrayView) -> Menu {
    let lang = view.language.as_str();
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::with_id("open", tr(lang, "tray.open", "Open A2DPWB"), true, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    for (i, name) in view.profiles.iter().take(TRAY_PROFILES).enumerate() {
        let active = view.streaming && view.active_profile.as_deref() == Some(name.as_str());
        let _ = menu.append(&CheckMenuItem::with_id(format!("profile:{i}"), name, true, active, None));
    }
    if !view.profiles.is_empty() {
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    let _ =
        menu.append(&MenuItem::with_id("disconnect", tr(lang, "tray.disconnect", "Disconnect"), view.streaming, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id("exit", tr(lang, "tray.exit", "Exit"), true, None));
    menu
}

/// Link samples 4 times a second while a UI subscribes
async fn link_sampler(state: Arc<CoreState>) {
    let stats = state.service.stats();
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let mut last: Option<(Instant, u64, u64)> = None;
    loop {
        tick.tick().await;
        if !state.link_subscribed() {
            last = None;
            continue;
        }
        let r = Ordering::Relaxed;
        let (bytes, packets) = (stats.bytes_sent.load(r), stats.packets_sent.load(r));
        let now = Instant::now();
        let (bps, pps) = match last {
            Some((t, b, p)) => {
                let dt = now.duration_since(t).as_secs_f64().max(0.001);
                ((bytes - b) as f64 / dt, (packets - p) as f64 / dt)
            }
            None => (0.0, 0.0),
        };
        last = Some((now, bytes, packets));
        let radio = stats.radio();
        // 3-DH5: up to 1021 bytes in 5 slots, plus the return slot (3.75 ms);
        // each packet also carries L2CAP (4) and ACL (4) headers
        let airtime = if pps > 0.0 {
            let per_packet = bps / pps + 8.0;
            pps * (per_packet / 1021.0).ceil() * 0.00375
        } else {
            0.0
        };
        let start_dropped = stats.start_dropped.load(r);
        state.publish(Event::Link(LinkSample {
            time_ms: state.started.elapsed().as_millis() as u64,
            rssi: radio.rssi,
            afh_in_use: radio.afh_in_use,
            afh_usable: radio.afh_usable,
            afh_avoided: radio.afh_avoided,
            bytes_per_second: bps,
            packets_per_second: pps,
            queue_depth: stats.queue_depth.load(r),
            dropped: (stats.queue_drops.load(r) + stats.send_errors.load(r)).saturating_sub(start_dropped),
            capture_dropped: stats.capture_dropped_frames.load(r).saturating_sub(stats.start_capture_dropped.load(r)),
            airtime: airtime.min(1.0),
        }));
    }
}

pub fn run(args: &[String]) -> i32 {
    let background = args.iter().any(|a| a == "--background" || a == "--minimized");
    let _instance = match win_sys::single_instance::SingleInstance::acquire(CORE_MUTEX) {
        Ok(Some(g)) => g,
        Ok(None) => {
            // Already running: a start from the shortcut opens the UI
            tracing::info!("the core runs already");
            if !background {
                launch_ui(false);
            }
            return 0;
        }
        Err(e) => {
            tracing::error!("single instance check failed: {e}");
            return 1;
        }
    };
    if win_sys::single_instance::is_held(win_sys::single_instance::V1_MUTEX_NAME) {
        tracing::warn!("A2DPWB v1 is running: it may hold the Bluetooth adapter");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let (tx, rx) = mpsc::channel();
    let capture = HciCapture::new();
    let service = Arc::new(Service::new(
        ServiceOptions { config_dir: a2dpwb_core::paths::config_dir(), hci_tcp: None, capture, afh: None },
        tx,
    ));
    let state = {
        let (p1, p2) = (proxy.clone(), proxy.clone());
        CoreState::new(
            service.clone(),
            Box::new(move |v| {
                let _ = p1.send_event(UserEvent::Tray(v));
            }),
            Box::new(move || {
                let _ = p2.send_event(UserEvent::Quit);
            }),
        )
    };
    {
        let state = state.clone();
        std::thread::Builder::new()
            .name("service-events".into())
            .spawn(move || {
                while let Ok(e) = rx.recv() {
                    state.on_service_event(e);
                }
            })
            .expect("spawning the service event thread");
    }
    {
        let state = state.clone();
        let quit = proxy.clone();
        std::thread::Builder::new()
            .name("ipc".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("IPC runtime: {e}");
                        return;
                    }
                };
                rt.block_on(async move {
                    tokio::spawn(link_sampler(state.clone()));
                    let name = match win_sys::pipe::current_user_sid() {
                        Ok(sid) => ipc::pipe_name(&sid),
                        Err(e) => {
                            tracing::error!("cannot read the user SID: {e}");
                            return;
                        }
                    };
                    if let Err(e) = crate::server::serve(state, name).await {
                        tracing::error!("IPC server: {e}");
                        let _ = quit.send_event(UserEvent::Quit);
                    }
                });
            })
            .expect("spawning the IPC thread");
    }
    {
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = p.send_event(UserEvent::Menu(e.id.0));
        }));
        let p = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                let _ = p.send_event(UserEvent::Click);
            }
        }));
    }
    if !background {
        launch_ui(false);
    }

    let mut tray: Option<TrayIcon> = None;
    let mut view: Option<TrayView> = None;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            TaoEvent::NewEvents(StartCause::Init) => {
                let v = view.clone().unwrap_or(TrayView {
                    streaming: false,
                    tooltip: "A2DPWB".into(),
                    profiles: Vec::new(),
                    active_profile: None,
                    language: "en".into(),
                });
                let icon = tray_icon::Icon::from_resource(101, None).ok();
                let mut b = TrayIconBuilder::new()
                    .with_menu(Box::new(build_menu(&v)))
                    .with_menu_on_left_click(false)
                    .with_tooltip(&v.tooltip);
                if let Some(i) = icon {
                    b = b.with_icon(i);
                }
                match b.build() {
                    Ok(t) => tray = Some(t),
                    Err(e) => tracing::error!("tray icon: {e}"),
                }
            }
            TaoEvent::UserEvent(UserEvent::Tray(v)) => {
                if let Some(t) = &tray {
                    t.set_menu(Some(Box::new(build_menu(&v))));
                    let _ = t.set_tooltip(Some(&v.tooltip));
                }
                view = Some(v);
            }
            TaoEvent::UserEvent(UserEvent::Click) => launch_ui(true),
            TaoEvent::UserEvent(UserEvent::Menu(id)) => match id.as_str() {
                "open" => launch_ui(false),
                "disconnect" => state.stop(),
                "exit" => state.quit(),
                other => {
                    if let Some(i) = other.strip_prefix("profile:").and_then(|i| i.parse::<usize>().ok()) {
                        let name = view.as_ref().and_then(|v| v.profiles.get(i).cloned());
                        if let Some(name) = name
                            && let Err(e) = state.start_profile(&name)
                        {
                            tracing::warn!("tray: cannot start {name}: {}", e.code);
                        }
                    }
                }
            },
            TaoEvent::UserEvent(UserEvent::Quit) => {
                tracing::info!("exiting");
                tray = None;
                // Streaming stops and the adapter is released before exit
                service.shutdown();
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}
