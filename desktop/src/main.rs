//! Carbon as a macOS app.
//!
//! The app is the hub: it runs the same engine and server as `carbon serve`, inside its own process, and shows the
//! dashboard in a window. Closing the window leaves Carbon in the menu bar with every server still running;
//! quitting from the menu bar stops Carbon and the servers it started. If a hub is already answering on port 3000
//! (a `carbon serve` in a terminal), the app is just a window onto that one.

use std::sync::Arc;

use carbon::{
    client,
    engine::{HUB_PORT, Hub},
    server,
};
use tauri::{
    AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    let hub = Arc::new(Hub::new());
    // Someone else's hub is already there: don't start a second, and don't stop theirs on the way out
    let own = !client::running();

    let app = tauri::Builder::default()
        .setup({
            let hub = hub.clone();
            move |app| {
                if own {
                    tauri::async_runtime::block_on(server::listen(hub.clone()))?;
                }
                // The page tells the app apart from a browser by this, so "Open" goes to the real browser
                WebviewWindowBuilder::new(
                    app,
                    "main",
                    WebviewUrl::External(format!("http://localhost:{HUB_PORT}").parse()?),
                )
                .title("Carbon")
                .inner_size(1240.0, 860.0)
                .min_inner_size(720.0, 480.0)
                .user_agent("Carbon-Desktop")
                .build()?;

                let open = MenuItem::with_id(app, "open", "Open Carbon", true, None::<&str>)?;
                let browser =
                    MenuItem::with_id(app, "browser", "Open in Browser", true, None::<&str>)?;
                let quit =
                    MenuItem::with_id(app, "quit", "Quit Carbon", true, Some("CmdOrCtrl+Q"))?;
                let menu = Menu::with_items(
                    app,
                    &[&open, &browser, &PredefinedMenuItem::separator(app)?, &quit],
                )?;
                TrayIconBuilder::new()
                    .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                    // A template image: macOS draws it in the menu bar's own colour, light or dark
                    .icon_as_template(true)
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "open" => show(app),
                        "browser" => {
                            let _ = std::process::Command::new("open")
                                .arg(format!("http://localhost:{HUB_PORT}"))
                                .spawn();
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .build(app)?;
                Ok(())
            }
        })
        // Closing the window is not quitting: Carbon and its servers carry on in the menu bar
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("Carbon could not start");

    app.run(move |app, event| match event {
        // Clicking the Dock icon brings the window back
        RunEvent::Reopen { .. } => show(app),
        RunEvent::Exit if own => hub.shutdown(),
        _ => {}
    });
}
