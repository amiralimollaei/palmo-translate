mod api;
mod controller;
mod credentials;
mod languages;
mod model;
mod view;

use adw::prelude::*;
use gtk::glib;
use std::sync::Arc;

fn main() -> glib::ExitCode {
    let runtime =
        Arc::new(tokio::runtime::Runtime::new().expect("Cannot start networking runtime"));
    let app = adw::Application::builder()
        .application_id("io.github.amiralimollaei.PalmoTranslate")
        .build();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            controller::build_window(app, runtime.clone());
        }
    });
    app.run()
}

