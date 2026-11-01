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

/// WebKit views fill their window instead of reporting a useful natural size.
/// Cap dimensions to the actual logical monitor, with safe narrow-screen margins.
fn apply_webview_size(
    window: &gtk::ApplicationWindow,
    preferred_width: i32,
    preferred_height: i32,
) {
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

/// The offset below the 42px Top Bar.
pub const POPUP_TOP_MARGIN: i32 = 25;
