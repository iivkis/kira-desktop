//! Minimal Niri layer-shell host using GTK4 + WebKitGTK 6.
use std::env;
use std::path::PathBuf;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use webkit6::prelude::*;

const BAR_HEIGHT: i32 = 42;

fn ui_index() -> PathBuf {
    let directory = env::var_os("KIRA_UI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist"));
    directory.join("index.html")
}

fn main() {
    let index = ui_index();
    if !index.is_file() {
        eprintln!(
            "Kira UI assets not found: {}. Run 'task frontend:build' first or set KIRA_UI_DIR.",
            index.display()
        );
        std::process::exit(1);
    }

    let app = gtk::Application::builder()
        .application_id("dev.kira.Desktop")
        .build();

    app.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(Some("Kira Desktop"));
        window.set_decorated(false);
        window.set_default_size(800, BAR_HEIGHT);

        // Layer configuration must happen before GTK realizes the window.
        window.init_layer_shell();
        window.set_namespace(Some("kira-desktop"));
        window.set_layer(Layer::Top);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_exclusive_zone(BAR_HEIGHT);

        let view = webkit6::WebView::new();
        view.set_background_color(&gtk::gdk::RGBA::new(0.09, 0.09, 0.12, 1.0));
        let file_uri = gtk::gio::File::for_path(&index).uri();
        view.load_uri(file_uri.as_str());
        window.set_child(Some(&view));
        window.present();
    });

    app.run();
}
