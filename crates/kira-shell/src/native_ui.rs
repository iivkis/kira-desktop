//! Shared layer-shell/WebKit infrastructure for Svelte overlays.
use gtk::prelude::*;

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

/// WebKit does not expose its HTML's intrinsic height as a GTK size request.
/// The trusted Svelte host measures its natural root height and sends it to
/// GTK, which uses the current *logical output* for the 70% upper bound.
fn output_size(window: &gtk::ApplicationWindow) -> (i32, i32) {
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
    monitor
        .map(|m| (m.geometry().width(), m.geometry().height()))
        .unwrap_or((800, 800))
}

pub fn webview_popup_height_limit(window: &gtk::ApplicationWindow) -> i32 {
    let (_, output_height) = output_size(window);
    (i64::from(output_height.max(1)) * 7 / 10).max(1) as i32
}

/// Window sizing is driven by Svelte's measured root, never by a fixed
/// WebKit default. Do NOT reset size on GTK's map callback: a widget may
/// already have reported its correct height while the surface was hidden.
pub fn resize_webview_popup(
    window: &gtk::ApplicationWindow,
    preferred_width: i32,
    content_height: i32,
) -> i32 {
    let (output_width, _) = output_size(window);
    let width = preferred_width.min(output_width.saturating_sub(24)).max(1);
    let height = content_height.clamp(1, webview_popup_height_limit(window));
    window.set_default_size(width, height);
    height
}

pub fn configure_webview_popup(window: &gtk::ApplicationWindow, width: i32, height: i32) {
    resize_webview_popup(window, width, height);
}

/// The offset below the 42px Top Bar.
pub const POPUP_TOP_MARGIN: i32 = 25;
