// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! ARMATURA Studio as a native window.
//!
//! `armatura-studio [project.json]` starts the `armatura` engine next to it as `studio` on a free local port and shows
//! it in a window (WebView2 on Windows, WKWebView on macOS); closing the window stops the server. Without a project it
//! opens `Documents/ARMATURA/first-rig.json`, created from the built-in demo when missing.
//! `armatura-studio --selftest` starts the server, fetches the page, stops it and exits 0 or 1 (no window; used by CI).
//! Linux has no window here (it would need system web-view packages): use `armatura studio` and a browser.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod launch;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--selftest") {
        let ok = match launch::start(None) {
            Ok(s) => launch::page_ok(&s),
            Err(_) => false,
        };
        std::process::exit(if ok { 0 } else { 1 });
    }
    let project = args.first().filter(|a| !a.starts_with("--")).map(std::path::PathBuf::from);
    window::run(project);
}

#[cfg(any(windows, target_os = "macos"))]
mod window {
    use super::launch;
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    fn escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    pub fn run(project: Option<std::path::PathBuf>) {
        let started = launch::start(project);
        let event_loop = EventLoop::new();
        let title = match &started {
            Ok(s) => format!("ARMATURA Studio - {}", s.project.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            Err(_) => "ARMATURA Studio".to_string(),
        };
        let window = WindowBuilder::new().with_title(title).with_inner_size(LogicalSize::new(1400.0, 900.0)).build(&event_loop).expect("cannot create the window");
        let builder = WebViewBuilder::new();
        let builder = match &started {
            Ok(s) => builder.with_url(&s.url),
            Err(e) => builder.with_html(format!(
                "<html><body style=\"font-family:sans-serif;margin:2em\"><h2>ARMATURA Studio could not start</h2><p>{}</p></body></html>",
                escape(e)
            )),
        };
        let _webview = builder.build(&window).expect("cannot create the web view");
        let mut server = started.ok();
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
                server.take(); // Drop stops the server
                *control_flow = ControlFlow::Exit;
            }
        });
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod window {
    pub fn run(_project: Option<std::path::PathBuf>) {
        eprintln!("armatura-studio has no window on this platform; run `armatura studio <project.json>` and open the printed address in a browser.");
        std::process::exit(2);
    }
}
