//! Reusable native capacity cards. Linux telemetry is provided by the host;
//! no filesystem access, Core IPC or periodic sampling in kira-ui.
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

pub const CAPACITY_FRESHNESS: Duration = Duration::from_secs(6);

#[derive(Clone, Debug, PartialEq)]
pub struct CapacityReading {
    pub headline: String,
    pub fraction: String,
    pub used_fraction: Option<f64>,
}

fn bytes_label(kib: u64) -> String {
    if kib >= 1_048_576 {
        format!("{:.1} GiB", kib as f64 / 1_048_576.0)
    } else {
        format!("{:.0} MiB", kib as f64 / 1024.0)
    }
}

/// Faithful to main's usageLabel/fractionLabel:
/// zero configured swap is NOT SET UP, unknown/invalid metrics are unavailable,
/// and a small non-zero allocation never becomes a false 0%.
pub fn capacity_reading(total: Option<u64>, available: Option<u64>) -> CapacityReading {
    let unavailable = || CapacityReading {
        headline: "НЕДОСТУПНО".into(),
        fraction: "НЕДОСТУПНО".into(),
        used_fraction: None,
    };
    let (Some(total), Some(available)) = (total, available) else {
        return unavailable();
    };
    if available > total {
        return unavailable();
    }
    let used = total - available;
    let fraction = format!("{} / {}", bytes_label(used), bytes_label(total));
    if total == 0 {
        return CapacityReading {
            headline: "НЕ НАСТРОЕНО".into(),
            fraction,
            used_fraction: None,
        };
    }
    let ratio = used as f64 / total as f64;
    let percent = ratio * 100.0;
    let headline = if used == 0 {
        "0%".into()
    } else if percent < 0.1 {
        "<0.1%".into()
    } else if percent < 1.0 {
        format!("{percent:.1}%")
    } else {
        format!("{percent:.0}%")
    };
    CapacityReading {
        headline,
        fraction,
        used_fraction: Some(ratio),
    }
}

struct CapacityCard {
    headline: gtk::Label,
    fraction: gtk::Label,
    progress: gtk::ProgressBar,
}

impl CapacityCard {
    fn new(title: &str, parent: &gtk::Box) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 7);
        root.add_css_class("system-capacity-card");
        root.set_hexpand(true);

        let heading = gtk::Label::new(Some(title));
        heading.add_css_class("system-capacity-name");
        heading.set_xalign(0.0);
        root.append(&heading);

        let headline = gtk::Label::new(Some("НЕДОСТУПНО"));
        headline.add_css_class("system-capacity-value");
        headline.set_xalign(0.0);
        root.append(&headline);

        let progress = gtk::ProgressBar::new();
        progress.set_show_text(false);
        progress.set_visible(false);
        progress.set_tooltip_text(Some(title));
        root.append(&progress);

        let fraction = gtk::Label::new(Some("НЕДОСТУПНО"));
        fraction.add_css_class("system-capacity-fraction");
        fraction.set_xalign(0.0);
        fraction.set_ellipsize(gtk::pango::EllipsizeMode::End);
        root.append(&fraction);

        parent.append(&root);
        Self {
            headline,
            fraction,
            progress,
        }
    }

    fn update(&self, reading: CapacityReading) {
        self.headline.set_text(&reading.headline);
        self.fraction.set_text(&reading.fraction);
        self.fraction.set_tooltip_text(Some(&reading.fraction));
        if let Some(ratio) = reading.used_fraction {
            self.progress.set_fraction(ratio);
            self.progress.set_visible(true);
        } else {
            // Never present a zero-length progress bar as a zero measurement
            // when storage/swap is not configured or is currently unavailable.
            self.progress.set_visible(false);
        }
    }
}

struct CapacityState {
    root: CapacityCard,
    swap: CapacityCard,
    version: Cell<u64>,
}

impl CapacityState {
    fn clear(&self) {
        let absent = capacity_reading(None, None);
        self.root.update(absent.clone());
        self.swap.update(absent);
    }

    fn invalidate(&self) {
        self.version.set(self.version.get().wrapping_add(1));
        self.clear();
    }
}

/// The view is independently embeddable in any GTK4 host.
pub struct CapacityCards {
    root: gtk::Box,
    state: Rc<CapacityState>,
}

impl CapacityCards {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("system-capacity");

        let heading = gtk::Label::new(Some("ЁМКОСТЬ"));
        heading.add_css_class("system-capacity-heading");
        heading.set_xalign(0.0);
        root.append(&heading);

        let cards = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        cards.set_homogeneous(true);
        let disk = CapacityCard::new("ОСНОВНОЙ ДИСК", &cards);
        let swap = CapacityCard::new("ПАМЯТЬ SWAP", &cards);
        root.append(&cards);

        let state = Rc::new(CapacityState {
            root: disk,
            swap,
            version: Cell::new(0),
        });
        let weak = Rc::downgrade(&state);
        root.connect_unmap(move |_| {
            if let Some(state) = weak.upgrade() {
                state.invalidate();
            }
        });
        Self { root, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn update(
        &self,
        disk_total: Option<u64>,
        disk_available: Option<u64>,
        swap_total: Option<u64>,
        swap_free: Option<u64>,
    ) {
        let revision = self.state.version.get().wrapping_add(1);
        self.state.version.set(revision);
        self.state
            .root
            .update(capacity_reading(disk_total, disk_available));
        self.state
            .swap
            .update(capacity_reading(swap_total, swap_free));
        // Original main expires the slow native telemetry after six seconds.
        // A newer sample or unmap makes the old timeout inert.
        let weak = Rc::downgrade(&self.state);
        gtk::glib::timeout_add_local_once(CAPACITY_FRESHNESS, move || {
            if let Some(state) = weak.upgrade()
                && state.version.get() == revision
            {
                state.invalidate();
            }
        });
    }
}

impl Default for CapacityCards {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_distinguishes_absent_unconfigured_and_real_zero() {
        assert_eq!(capacity_reading(None, None).headline, "НЕДОСТУПНО");
        assert_eq!(capacity_reading(Some(100), None).used_fraction, None);
        assert_eq!(capacity_reading(Some(0), Some(1)).headline, "НЕДОСТУПНО");
        let empty_swap = capacity_reading(Some(0), Some(0));
        assert_eq!(empty_swap.headline, "НЕ НАСТРОЕНО");
        assert_eq!(empty_swap.used_fraction, None);
        let unused = capacity_reading(Some(2048), Some(2048));
        assert_eq!(unused.headline, "0%");
        assert_eq!(unused.used_fraction, Some(0.0));
        assert_eq!(unused.fraction, "0 MiB / 2 MiB");
    }

    #[test]
    fn small_nonzero_capacity_is_not_rounded_to_zero() {
        assert_eq!(
            capacity_reading(Some(1_000_000), Some(999_999)).headline,
            "<0.1%"
        );
        assert_eq!(
            capacity_reading(Some(1000), Some(995)).headline,
            "0.5%"
        );
        assert_eq!(capacity_reading(Some(1000), Some(0)).headline, "100%");
    }

    #[test]
    fn invalid_available_values_never_yield_a_progress_bar() {
        let invalid = capacity_reading(Some(100), Some(101));
        assert_eq!(invalid.headline, "НЕДОСТУПНО");
        assert_eq!(invalid.used_fraction, None);
        let known = capacity_reading(Some(10 * 1_048_576), Some(6 * 1_048_576));
        assert_eq!(known.headline, "40%");
        assert_eq!(known.fraction, "4.0 GiB / 10.0 GiB");
        assert_eq!(known.used_fraction, Some(0.4));
    }
}
