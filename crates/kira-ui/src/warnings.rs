//! Reusable 42px, in-bar warning inspector. No extra layer-shell window,
//! untrusted markup, Core RPC or timer. Selection follows the frozen main
//! warning-model.ts contract and survives reordering by stable warning ID.
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;

pub const MAX_WARNINGS: usize = 8;
pub const MAX_WARNING_TEXT: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiWarning {
    pub id: String,
    pub text: String,
    pub short: String,
}

pub fn visible_warning_index(warnings: &[UiWarning], selected_id: Option<&str>) -> Option<usize> {
    if warnings.is_empty() {
        None
    } else {
        Some(
            warnings
                .iter()
                .position(|item| Some(item.id.as_str()) == selected_id)
                .unwrap_or(0),
        )
    }
}

pub fn next_warning_id(warnings: &[UiWarning], selected_id: Option<&str>) -> Option<String> {
    let index = visible_warning_index(warnings, selected_id)?;
    Some(warnings[(index + 1) % warnings.len()].id.clone())
}

/// Bound text and reject duplicate or empty IDs without manufacturing errors.
pub fn bounded_warnings(input: impl IntoIterator<Item = UiWarning>) -> Vec<UiWarning> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for warning in input {
        let id: String = warning.id.chars().take(64).collect();
        let id = id.trim();
        if id.is_empty() || !seen.insert(id.to_owned()) {
            continue;
        }
        let text: String = warning.text.chars().take(MAX_WARNING_TEXT).collect();
        let short: String = warning.short.chars().take(80).collect();
        if text.trim().is_empty() || short.trim().is_empty() {
            continue;
        }
        out.push(UiWarning {
            id: id.to_owned(),
            text,
            short,
        });
        if out.len() >= MAX_WARNINGS {
            break;
        }
    }
    out
}

struct WarningState {
    warnings: RefCell<Vec<UiWarning>>,
    selected_id: RefCell<Option<String>>,
    open: Cell<bool>,
    wide: Cell<bool>,
    button: gtk::Button,
    button_text: gtk::Label,
    button_index: gtk::Label,
    overlay: gtk::Box,
    counter: gtk::Label,
    details: gtk::TextView,
    next: gtk::Button,
    close: gtk::Button,
    background: gtk::Widget,
    fallback: gtk::Widget,
}

impl WarningState {
    fn selected(&self) -> Option<(usize, UiWarning, usize)> {
        let warnings = self.warnings.borrow();
        let index = visible_warning_index(&warnings, self.selected_id.borrow().as_deref())?;
        Some((index, warnings[index].clone(), warnings.len()))
    }

    fn render(&self) {
        let Some((index, warning, count)) = self.selected() else {
            self.button.set_visible(false);
            self.overlay.set_visible(false);
            return;
        };
        self.button.set_visible(true);
        self.button_text.set_text(if self.wide.get() {
            &warning.text
        } else {
            &warning.short
        });
        self.button_index.set_visible(count > 1);
        self.button_index
            .set_text(&format!("{}/{}", index + 1, count));
        self.button.set_tooltip_text(Some(&format!(
            "{}\nНажмите для просмотра полного текста",
            warning.text
        )));
        self.button
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть предупреждение {} из {}: {}",
                index + 1,
                count,
                warning.short
            ))]);
        self.counter.set_text(&format!("!{}/{}", index + 1, count));
        self.details.buffer().set_text(&warning.text);
        self.next.set_visible(count > 1);
        self.overlay
            .update_property(&[gtk::accessible::Property::Label(
                "Подробности предупреждения",
            )]);
        self.details
            .update_property(&[gtk::accessible::Property::Label(
                "Полный текст предупреждения, можно выделить и скопировать",
            )]);
    }

    fn reveal(&self) {
        if self.selected().is_none() {
            return;
        }
        self.open.set(true);
        self.background.set_sensitive(false);
        self.overlay.set_visible(true);
        self.render();
        self.details.grab_focus();
    }

    fn dismiss(&self) {
        if !self.open.replace(false) {
            return;
        }
        self.background.set_sensitive(true);
        self.overlay.set_visible(false);
        if self.warnings.borrow().is_empty() {
            self.fallback.grab_focus();
        } else {
            self.button.grab_focus();
        }
    }

    fn advance(&self) {
        let id = next_warning_id(
            &self.warnings.borrow(),
            self.selected_id.borrow().as_deref(),
        );
        self.selected_id.replace(id);
        self.render();
        // The original readonly text area stays focused when paging. The
        // selection is cleared rather than copied from the previous warning.
        let buffer = self.details.buffer();
        buffer.place_cursor(&buffer.start_iter());
        self.details.grab_focus();
    }

    fn handle_key(&self, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
        if !self.open.get() {
            return false;
        }
        if key == gtk::gdk::Key::Escape {
            self.dismiss();
            return true;
        }
        if key != gtk::gdk::Key::Tab && key != gtk::gdk::Key::ISO_Left_Tab {
            return false;
        }
        let targets: Vec<gtk::Widget> = [
            Some(self.details.clone().upcast()),
            self.next.is_visible().then(|| self.next.clone().upcast()),
            Some(self.close.clone().upcast()),
        ]
        .into_iter()
        .flatten()
        .collect();
        let current = targets
            .iter()
            .position(|widget| widget.has_focus())
            .unwrap_or(0);
        let backwards = key == gtk::gdk::Key::ISO_Left_Tab
            || modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let target = if backwards {
            (current + targets.len() - 1) % targets.len()
        } else {
            (current + 1) % targets.len()
        };
        targets[target].grab_focus();
        true
    }
}

/// Pure view state with a 42px overlay, no other Wayland layer surface. The
/// parent supplies the normal bar layout and a keyboard focus fallback.
pub struct WarningInspector {
    state: Rc<WarningState>,
}

impl WarningInspector {
    pub fn new(background: &gtk::Widget, fallback: &gtk::Widget) -> Self {
        let button = gtk::Button::new();
        button.add_css_class("bar-warning-trigger");
        button.set_visible(false);
        let button_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let button_text = gtk::Label::new(None);
        button_text.set_ellipsize(gtk::pango::EllipsizeMode::End);
        button_text.set_max_width_chars(22);
        button_content.append(&button_text);
        let button_index = gtk::Label::new(None);
        button_index.add_css_class("bar-warning-index");
        button_content.append(&button_index);
        button.set_child(Some(&button_content));

        let overlay = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        overlay.add_css_class("bar-warning-inspector");
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        overlay.set_halign(gtk::Align::Fill);
        overlay.set_valign(gtk::Align::Fill);
        overlay.set_visible(false);
        let counter = gtk::Label::new(None);
        counter.add_css_class("bar-warning-count");
        overlay.append(&counter);
        let details = gtk::TextView::new();
        details.set_editable(false);
        details.set_cursor_visible(true);
        details.set_wrap_mode(gtk::WrapMode::WordChar);
        details.add_css_class("bar-warning-text");
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hexpand(true);
        scroll.set_vexpand(true);
        scroll.set_min_content_height(30);
        scroll.set_max_content_height(34);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_child(Some(&details));
        overlay.append(&scroll);
        let next = gtk::Button::with_label("›");
        next.add_css_class("bar-warning-control");
        next.update_property(&[gtk::accessible::Property::Label("Следующее предупреждение")]);
        overlay.append(&next);
        let close = gtk::Button::with_label("×");
        close.add_css_class("bar-warning-control");
        close.update_property(&[gtk::accessible::Property::Label("Закрыть подробности")]);
        overlay.append(&close);

        let state = Rc::new(WarningState {
            warnings: RefCell::new(Vec::new()),
            selected_id: RefCell::new(None),
            open: Cell::new(false),
            wide: Cell::new(false),
            button,
            button_text,
            button_index,
            overlay,
            counter,
            details,
            next,
            close,
            background: background.clone(),
            fallback: fallback.clone(),
        });
        let weak = Rc::downgrade(&state);
        state.button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.reveal();
            }
        });
        let weak = Rc::downgrade(&state);
        state.next.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.advance();
            }
        });
        let weak = Rc::downgrade(&state);
        state.close.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.dismiss();
            }
        });
        // Capture before a text editor consumes Tab. No focus may escape to
        // the disabled main bar while the in-place inspector is visible.
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&state);
        key.connect_key_pressed(move |_, key, _, modifiers| {
            if weak
                .upgrade()
                .is_some_and(|state| state.handle_key(key, modifiers))
            {
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        state.overlay.add_controller(key);
        Self { state }
    }

    pub fn button_widget(&self) -> gtk::Widget {
        self.state.button.clone().upcast()
    }

    pub fn inspector_widget(&self) -> gtk::Widget {
        self.state.overlay.clone().upcast()
    }

    pub fn set_warnings(&self, warnings: Vec<UiWarning>) {
        let warnings = bounded_warnings(warnings);
        let selected = self.state.selected_id.borrow().clone();
        let selected = visible_warning_index(&warnings, selected.as_deref())
            .map(|index| warnings[index].id.clone());
        let cleared = warnings.is_empty();
        self.state.warnings.replace(warnings);
        self.state.selected_id.replace(selected);
        if cleared {
            self.state.dismiss();
        }
        self.state.render();
    }

    pub fn set_bar_width(&self, width: i32) {
        let wide = width >= 900;
        if self.state.wide.replace(wide) != wide {
            self.state.render();
        }
    }

    pub fn is_inspecting(&self) -> bool {
        self.state.open.get()
    }

    pub fn dismiss(&self) {
        self.state.dismiss();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warn(id: &str) -> UiWarning {
        UiWarning {
            id: id.into(),
            text: format!("Ошибка {id}"),
            short: id.into(),
        }
    }

    #[test]
    fn next_preserves_frozen_main_order_wrap_and_missing_selection() {
        let warnings = vec![warn("config"), warn("launcher"), warn("calendar")];
        assert_eq!(visible_warning_index(&[], Some("config")), None);
        assert_eq!(next_warning_id(&[], Some("config")), None);
        assert_eq!(visible_warning_index(&warnings, None), Some(0));
        assert_eq!(
            next_warning_id(&warnings, None).as_deref(),
            Some("launcher")
        );
        assert_eq!(
            next_warning_id(&warnings, Some("launcher")).as_deref(),
            Some("calendar")
        );
        assert_eq!(
            next_warning_id(&warnings, Some("calendar")).as_deref(),
            Some("config")
        );
        assert_eq!(
            visible_warning_index(&warnings[1..], Some("calendar")),
            Some(1)
        );
        assert_eq!(
            visible_warning_index(&warnings[..2], Some("calendar")),
            Some(0)
        );
        assert_eq!(
            next_warning_id(&warnings[..2], Some("calendar")).as_deref(),
            Some("launcher")
        );
        assert_eq!(
            next_warning_id(&warnings[..1], None).as_deref(),
            Some("config")
        );
    }

    #[test]
    fn bounded_input_rejects_duplicates_empty_and_oversize() {
        let mut warnings = vec![warn("config"), warn("config"), warn(" ")];
        warnings.push(UiWarning {
            id: "long".into(),
            text: "⚠".repeat(MAX_WARNING_TEXT + 50),
            short: "a".repeat(100),
        });
        warnings.extend((0..20).map(|i| warn(&format!("n{i}"))));
        let bounded = bounded_warnings(warnings);
        assert_eq!(bounded.len(), MAX_WARNINGS);
        assert_eq!(bounded[0].id, "config");
        assert_eq!(bounded[1].id, "long");
        assert_eq!(bounded[1].text.chars().count(), MAX_WARNING_TEXT);
        assert_eq!(bounded[1].short.chars().count(), 80);
    }
}
