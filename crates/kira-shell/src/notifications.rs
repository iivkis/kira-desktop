//! The freedesktop notification server and its single bottom-right Svelte host.
//! This deliberately uses the already-linked GIO session D-Bus API. GTK owns
//! the Wayland surface; no arbitrary D-Bus or OS commands are exposed to JS.
use crate::notification_layout::{self, Layout};
use crate::notification_queue::{DEFAULT_DWELL, NotificationQueue, Notice};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use gtk::glib::{self, variant::ToVariant};
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";
const NAME: &str = "org.freedesktop.Notifications";
const XML: &str = r#"<node>
  <interface name="org.freedesktop.Notifications">
    <method name="GetCapabilities"><arg type="as" direction="out"/></method>
    <method name="GetServerInformation">
      <arg type="s" direction="out"/><arg type="s" direction="out"/>
      <arg type="s" direction="out"/><arg type="s" direction="out"/>
    </method>
    <method name="Notify">
      <arg type="s" direction="in"/><arg type="u" direction="in"/>
      <arg type="s" direction="in"/><arg type="s" direction="in"/>
      <arg type="s" direction="in"/><arg type="as" direction="in"/>
      <arg type="a{sv}" direction="in"/><arg type="i" direction="in"/>
      <arg type="u" direction="out"/>
    </method>
    <method name="CloseNotification"><arg type="u" direction="in"/></method>
    <signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
    <signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
  </interface>
</node>"#;

thread_local! {
    // One notification service per running Shell, never per widget or toast.
    static ACTIVE: RefCell<Option<Rc<Service>>> = const { RefCell::new(None) };
}

struct Host {
    window: gtk::ApplicationWindow,
    view: webkit6::WebView,
    ready: Cell<bool>,
    layout: Cell<Option<Layout>>,
}

struct Service {
    app: gtk::Application,
    html: String,
    context: webkit6::WebContext,
    owned: Cell<bool>,
    bus: RefCell<Option<gtk::gio::DBusConnection>>,
    queue: RefCell<NotificationQueue>,
    host: RefCell<Option<Rc<Host>>>,
    timer_epoch: Cell<u64>,
}

pub fn start(app: &gtk::Application, html: &str, context: &webkit6::WebContext) {
    // A nested preview inherits the user's *real* session bus. Never seize
    // their live notification daemon while testing a child Niri compositor.
    if std::env::var_os("KIRA_PREVIEW").is_some()
        && std::env::var_os("KIRA_NOTIFICATIONS_PREVIEW").is_none()
    {
        eprintln!("kira-shell: notification D-Bus ownership disabled in isolated preview");
        return;
    }
    let service = Rc::new(Service {
        app: app.clone(),
        html: html.to_owned(),
        context: context.clone(),
        owned: Cell::new(false),
        bus: RefCell::new(None),
        queue: RefCell::new(NotificationQueue::default()),
        host: RefCell::new(None),
        timer_epoch: Cell::new(0),
    });
    ACTIVE.with(|active| *active.borrow_mut() = Some(service.clone()));

    let acquired = Rc::downgrade(&service);
    let name_acquired = Rc::downgrade(&service);
    let name_lost = Rc::downgrade(&service);
    let _id = gtk::gio::bus_own_name(
        gtk::gio::BusType::Session,
        NAME,
        gtk::gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        move |connection, _| {
            if let Some(service) = acquired.upgrade() {
                service.register_dbus(&connection);
            }
        },
        move |connection, _| {
            if let Some(service) = name_acquired.upgrade() {
                *service.bus.borrow_mut() = Some(connection);
                service.owned.set(true);
                service.sync();
                eprintln!("kira-shell: acquired org.freedesktop.Notifications");
            }
        },
        move |_, _| {
            if let Some(service) = name_lost.upgrade() {
                service.owned.set(false);
                service.bus.borrow_mut().take();
                service.sync();
                eprintln!("kira-shell: notification D-Bus name unavailable (another daemon?)");
            }
        },
    );
}

pub fn layout_changed() {
    ACTIVE.with(|active| {
        if let Some(service) = active.borrow().as_ref() {
            service.sync();
        }
    });
}

fn bounded(value: String, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn notice_from_params(queue: &mut NotificationQueue, params: &glib::Variant) -> Notice {
    let app = bounded(params.child_value(0).get::<String>().unwrap_or_default(), 128);
    let replace = params.child_value(1).get::<u32>().unwrap_or(0);
    let summary = bounded(params.child_value(3).get::<String>().unwrap_or_default(), 256);
    // Clients may send minimal HTML. No raw body markup ever reaches WebKit.
    let body = bounded(params.child_value(4).get::<String>().unwrap_or_default(), 2048);
    let raw_actions = params.child_value(5).get::<Vec<String>>().unwrap_or_default();
    let actions = raw_actions.chunks_exact(2).take(8)
        .map(|pair| (bounded(pair[0].clone(), 128), bounded(pair[1].clone(), 128)))
        .collect();
    let hints = glib::VariantDict::new(Some(&params.child_value(6)));
    let critical = hints.lookup_value("urgency", None)
        .and_then(|value| value.get::<u8>())
        .is_some_and(|urgency| urgency == 2);
    let timeout = params.child_value(7).get::<i32>().unwrap_or(-1);
    // 0 means logically persistent, but visual stack still yields after 5 s.
    let dwell = if timeout <= 0 { DEFAULT_DWELL } else {
        Duration::from_millis(u64::try_from(timeout.max(3000).min(30000)).unwrap_or(5000))
    };
    let id = if replace != 0 && queue.history().any(|item| item.id == replace) {
        replace
    } else {
        queue.allocate_id()
    };
    Notice { id, app, summary, body, actions, critical, dwell }
}

impl Service {
    fn register_dbus(self: &Rc<Self>, connection: &gtk::gio::DBusConnection) {
        let Ok(xml) = gtk::gio::DBusNodeInfo::for_xml(XML) else {
            eprintln!("kira-shell: failed to parse notification D-Bus introspection");
            return;
        };
        let Some(info) = xml.lookup_interface(INTERFACE) else { return };
        let weak = Rc::downgrade(self);
        let registered = connection.register_object(OBJECT_PATH, &info)
            .method_call(move |_connection, _sender, _path, _interface, method, params, invocation| {
                let Some(service) = weak.upgrade() else {
                    invocation.return_dbus_error("org.freedesktop.DBus.Error.Failed", "Notification host stopped");
                    return;
                };
                match method {
                    "GetCapabilities" => {
                        let caps = vec!["body".to_string(), "actions".to_string()];
                        invocation.return_value(Some(&(caps,).to_variant()));
                    }
                    "GetServerInformation" => {
                        invocation.return_value(Some(&(
                            "Kira Desktop", "Kira", env!("CARGO_PKG_VERSION"), "1.3",
                        ).to_variant()));
                    }
                    "Notify" => {
                        let id = {
                            let mut queue = service.queue.borrow_mut();
                            let notice = notice_from_params(&mut queue, &params);
                            let id = notice.id;
                            queue.push(notice, Instant::now());
                            id
                        };
                        invocation.return_value(Some(&(id,).to_variant()));
                        service.sync();
                    }
                    "CloseNotification" => {
                        let id = params.child_value(0).get::<u32>().unwrap_or(0);
                        let existed = service.queue.borrow_mut().close(id);
                        invocation.return_value(None);
                        if existed { service.signal_closed(id, 3); }
                        service.sync();
                    }
                    _ => invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod", "Unknown notification method"
                    ),
                }
            }).build();
        if let Err(error) = registered {
            eprintln!("kira-shell: cannot register D-Bus Notifications object: {error}");
        }
    }

    fn signal_closed(&self, id: u32, reason: u32) {
        if !self.owned.get() { return; }
        if let Some(connection) = self.bus.borrow().as_ref() {
            let _ = connection.emit_signal(
                None, OBJECT_PATH, INTERFACE, "NotificationClosed",
                Some(&(id, reason).to_variant()),
            );
        }
    }

    fn action(self: &Rc<Self>, id: u32, key: &str) {
        let allowed = self.queue.borrow().history().any(|item| item.id == id &&
            item.actions.iter().any(|(action, _)| action == key));
        if !allowed || !self.owned.get() { return; }
        if let Some(connection) = self.bus.borrow().as_ref() {
            let _ = connection.emit_signal(
                None, OBJECT_PATH, INTERFACE, "ActionInvoked",
                Some(&(id, key).to_variant()),
            );
        }
        if self.queue.borrow_mut().close(id) { self.signal_closed(id, 2); }
        self.sync();
    }

    fn handle_ui(self: &Rc<Self>, message: &str) {
        if message == "ready" {
            if let Some(host) = self.host.borrow().as_ref() {
                host.ready.set(true);
            }
            self.sync();
        } else if let Some(id) = message.strip_prefix("close:").and_then(|s| s.parse::<u32>().ok()) {
            if self.queue.borrow_mut().close(id) {
                self.signal_closed(id, 2);
                self.sync();
            }
        } else if let Some(value) = message.strip_prefix("action:") {
            if let Some((id, key)) = value.split_once(':') {
                if let Ok(id) = id.parse::<u32>() {
                    self.action(id, key);
                }
            }
        }
    }

    fn create_host(self: &Rc<Self>) -> Rc<Host> {
        crate::native_ui::install_transparent_popup_style();
        let window = gtk::ApplicationWindow::new(&self.app);
        window.set_title(Some("Kira Notifications"));
        window.set_decorated(false);
        window.add_css_class("kira-popup-transparent");
        window.init_layer_shell();
        window.set_namespace(Some("kira-notifications"));
        window.set_layer(Layer::Overlay);
        window.set_anchor(Edge::Top, false);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin(Edge::Right, notification_layout::OUTER_MARGIN);
        window.set_margin(Edge::Bottom, notification_layout::OUTER_MARGIN);
        window.set_exclusive_zone(0);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_default_size(360, 240);

        let manager = webkit6::UserContentManager::new();
        let weak = Rc::downgrade(self);
        manager.connect_script_message_received(Some("kiraNotificationUi"), move |_, value| {
            if let Some(service) = weak.upgrade() {
                let message = value.to_string().to_string();
                if message.len() <= 512 { service.handle_ui(&message); }
            }
        });
        assert!(manager.register_script_message_handler("kiraNotificationUi", None));
        // Unlike ordinary widget WebViews this bridge exposes no Core RPC.
        let view = webkit6::WebView::builder()
            .web_context(&self.context)
            .user_content_manager(&manager)
            .build();
        view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
        crate::native_ui::restrict_webview_navigation(&view);
        window.set_child(Some(&view));
        let weak = Rc::downgrade(self);
        view.connect_web_process_terminated(move |webview, _| {
            if let Some(service) = weak.upgrade() {
                if let Some(host) = service.host.borrow().as_ref() {
                    host.ready.set(false);
                    host.window.set_visible(false);
                }
                service.queue.borrow_mut().step(Instant::now(), false);
                // A fresh Svelte instance will request a full snapshot.
                let document = notification_html(&service.html);
                let webview = webview.clone();
                glib::timeout_add_local_once(Duration::from_millis(350), move || {
                    webview.load_html(&document, None);
                });
            }
        });
        let document = notification_html(&self.html);
        view.load_html(&document, None);
        let host = Rc::new(Host {
            window, view, ready: Cell::new(false), layout: Cell::new(None),
        });
        gtk::prelude::WidgetExt::realize(&host.window);
        host
    }

    fn render(&self, host: &Host, layout: Layout) {
        let showing: Vec<_> = self.queue.borrow().visible().map(|notice| {
            serde_json::json!({
                "id": notice.id,
                "app": notice.app,
                "summary": notice.summary,
                "body": notice.body,
                "critical": notice.critical,
                "actions": notice.actions.iter().map(|(key,label)| serde_json::json!({
                    "key": key, "label": label
                })).collect::<Vec<_>>(),
            })
        }).collect();
        let payload = serde_json::json!({
            "height": layout.height, "cardHeight": layout.card_height, "visible": showing
        });
        // Serialize twice: sender-controlled notification text is never JS.
        if let Ok(encoded) = serde_json::to_string(&payload.to_string()) {
            let script = format!(
                "window.dispatchEvent(new CustomEvent('kira:notifications:update', {{ detail: JSON.parse({encoded}) }}));"
            );
            host.view.evaluate_javascript(
                &script, None, None, None::<&gtk::gio::Cancellable>, |_| {},
            );
        }
    }

    fn sync(self: &Rc<Self>) {
        let now = Instant::now();
        let can_shape = gtk::gdk::Display::default().is_some_and(|display| display.supports_input_shapes());
        let (width, height) = if let Some(host) = self.host.borrow().as_ref() {
            let display = gtk::prelude::WidgetExt::display(&host.window);
            let monitor = host.window.surface()
                .and_then(|surface| display.monitor_at_surface(&surface))
                .or_else(|| display.monitors().item(0)
                    .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok()));
            monitor.map(|m| (m.geometry().width(), m.geometry().height())).unwrap_or((800, 800))
        } else {
            let display = gtk::gdk::Display::default();
            display.and_then(|d| d.monitors().item(0)
                .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok()))
                .map(|m| (m.geometry().width(), m.geometry().height()))
                .unwrap_or((800, 800))
        };
        let clearance = crate::calendar::notification_bottom_clearance(width, height);
        let layout = notification_layout::plan(width, height, clearance);
        let possible = self.owned.get() && can_shape && layout.is_some();
        if possible && (self.queue.borrow().has_pending() || self.queue.borrow().visible_count() > 0)
            && self.host.borrow().is_none()
        {
            let host = self.create_host();
            self.host.replace(Some(host));
            // Wait until the Svelte event listener is mounted.
        }
        let host = self.host.borrow().clone();
        let ready = host.as_ref().is_some_and(|host| host.ready.get());
        let usable = possible && ready;
        let changed = self.queue.borrow_mut().step(now, usable);
        if let Some(host) = host {
            if usable && self.queue.borrow().visible_count() > 0 {
                let layout = layout.expect("checked layout");
                host.layout.set(Some(layout));
                host.window.set_default_size(layout.width, layout.height);
                // Restrict the GTK input shape BEFORE the layer surface maps.
                let region = notification_layout::hit_region(layout, self.queue.borrow().visible_count());
                if let Some(surface) = host.window.surface() {
                    surface.set_input_region(Some(&region));
                }
                self.render(&host, layout);
                host.window.set_visible(true);
                let weak: Weak<Self> = Rc::downgrade(self);
                glib::idle_add_local_once(move || {
                    if let Some(service) = weak.upgrade() {
                        if let Some(host) = service.host.borrow().as_ref() {
                            if host.window.is_visible() {
                                if let (Some(layout), Some(surface)) = (host.layout.get(), host.window.surface()) {
                                    let region = notification_layout::hit_region(
                                        layout, service.queue.borrow().visible_count(),
                                    );
                                    surface.set_input_region(Some(&region));
                                }
                            }
                        }
                    }
                });
            } else {
                host.window.set_visible(false);
            }
        }
        if changed && self.queue.borrow().visible_count() == 0 {
            // No empty mapped WebView with a half-screen hit target.
            if let Some(host) = self.host.borrow().as_ref() {
                host.window.set_visible(false);
            }
        }
        self.schedule(now, usable);
    }

    fn schedule(self: &Rc<Self>, now: Instant, usable: bool) {
        let generation = self.timer_epoch.get().wrapping_add(1);
        self.timer_epoch.set(generation);
        if !usable { return; }
        let Some(deadline) = self.queue.borrow().next_deadline(now) else { return };
        let delay = deadline.saturating_duration_since(now).max(Duration::from_millis(1));
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(delay, move || {
            if let Some(service) = weak.upgrade() {
                if service.timer_epoch.get() == generation {
                    service.sync();
                }
            }
        });
    }
}

fn notification_html(html: &str) -> String {
    html.replacen(
        "<html lang=\"ru\">",
        "<html lang=\"ru\" data-kira-surface=\"notifications\">",
        1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_bounded_and_renderer_is_a_distinct_surface() {
        assert_eq!(bounded("привет".into(), 3), "при");
        assert!(notification_html("<html lang=\"ru\">").contains("data-kira-surface=\"notifications\""));
        assert!(XML.contains("org.freedesktop.Notifications"));
    }
}
