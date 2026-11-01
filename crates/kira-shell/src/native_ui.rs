//! Shared native overlay plumbing. Layout, interaction and state remain local
//! to each overlay: only identical infrastructure belongs here.
use gtk::prelude::*;

/// Keep palette declarations and component rules in one provider so GTK named
/// colors resolve reliably, including in Nix's source-filtered package.
pub fn install_styles(styles: &str) {
    let provider = gtk::CssProvider::new();
    let complete = format!("{}\n{}", include_str!("native-palette.css"), styles);
    provider.load_from_data(&complete);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Every WebKit surface renders a self-contained trusted HTML document.
/// Block arbitrary navigation and new windows, including from popup widgets.
pub fn restrict_webview_navigation(view: &webkit6::WebView) {
    use webkit6::prelude::*;
    view.connect_decide_policy(|_, decision, kind| {
        if kind == webkit6::PolicyDecisionType::NewWindowAction {
            decision.ignore();
            return true;
        }
        if kind == webkit6::PolicyDecisionType::NavigationAction {
            let permitted = decision
                .downcast_ref::<webkit6::NavigationPolicyDecision>()
                .and_then(|navigation| navigation.navigation_action())
                .and_then(|action| action.request())
                .and_then(|request| request.uri())
                .is_some_and(|uri| crate::permitted_ui_page(uri.as_str()));
            if !permitted {
                decision.ignore();
                return true;
            }
        }
        false
    });
}

/// WebKit views fill their window instead of reporting a useful natural size.
/// Cap dimensions to the actual logical monitor, with safe narrow-screen margins.
fn apply_webview_size(window: &gtk::ApplicationWindow, preferred_width: i32, preferred_height: i32) {
    let display = gtk::prelude::WidgetExt::display(window);
    let monitor = window
        .surface()
        .and_then(|surface| display.monitor_at_surface(&surface))
        .or_else(|| {
            display.monitors().item(0)
                .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok())
        });
    let (output_width, output_height) = monitor
        .map(|m| (m.geometry().width(), m.geometry().height()))
        .unwrap_or((800, 800));
    let width = preferred_width.min(output_width.saturating_sub(24)).max(1);
    let height_limit = (i64::from(output_height.max(1)) * 7 / 10) as i32;
    let height = preferred_height.min(height_limit).max(1);
    window.set_default_size(width, height);
}

pub fn configure_webview_popup(window: &gtk::ApplicationWindow, width: i32, height: i32) {
    apply_webview_size(window, width, height);
    window.connect_map(move |window| apply_webview_size(window, width, height));
}

pub fn find_overlay(application: &gtk::Application, title: &str) -> Option<gtk::Window> {
    application
        .windows()
        .into_iter()
        .find(|window| window.title().as_deref() == Some(title))
}

pub fn dismiss_overlay(application: &gtk::Application, title: &str) -> bool {
    if let Some(window) = find_overlay(application, title) {
        window.close();
        return true;
    }
    false
}

/// The old 50px popup offset left too much space below the Top Bar.
pub const POPUP_TOP_MARGIN: i32 = 25;

/// Let GTK use the natural content height, capped at 70% of the monitor.
/// Fixed preferred heights used to force needless scrolling on tall outputs.
/// Recheck the actual surface monitor after mapping.
fn apply_popup_height(window: &gtk::ApplicationWindow, scroll: &gtk::ScrolledWindow, width: i32) {
    let display = gtk::prelude::WidgetExt::display(window);
    let monitor = window
        .surface()
        .and_then(|surface| display.monitor_at_surface(&surface))
        .or_else(|| {
            display
                .monitors()
                .item(0)
                .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok())
        });
    let output_height = monitor.map_or(800, |m| m.geometry().height()).max(1);
    let height_limit = (i64::from(output_height) * 7 / 10) as i32;
    scroll.set_min_content_height(1);
    scroll.set_max_content_height(height_limit);
    scroll.set_propagate_natural_height(true);
    // -1 allows the scroll's natural (bounded) height, rather than
    // forcing the former 490px/600px default sizes.
    window.set_default_size(width, -1);
}

pub fn configure_popup(window: &gtk::ApplicationWindow, scroll: &gtk::ScrolledWindow, width: i32) {
    apply_popup_height(window, scroll, width);
    let scroll = scroll.clone();
    window.connect_map(move |window| {
        apply_popup_height(window, &scroll, width);
    });
}
