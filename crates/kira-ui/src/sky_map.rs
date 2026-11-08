//! Reusable GTK4 offline sky atlas. No compositor, network, GPS or Core API.
//! The scene shares the Calendar's selected date, and uses explicit editable
//! observer coordinates. The Gregorian date/time uses the host LOCAL timezone.
use std::cell::RefCell;
use std::f64::consts::TAU;
use std::rc::Rc;

use gtk::prelude::*;

use crate::calendar_model::CivilDate;
use crate::motion::MotionToken;
use crate::sky_catalog::{CONSTELLATION_LABELS, CONSTELLATION_SEGMENTS, NAMED_STARS, STARS};
use crate::sky_model::{
    horizon_point, local_observation_unix, observer_coordinates, shortest_sidereal_delta,
    sidereal_degrees,
};

const ANIMATION_MS: u64 = 420;
const GREENWICH_LAT: &str = "51.4779";
const GREENWICH_LON: &str = "0";
const DEFAULT_TIME: &str = "22:00";

struct Scene {
    date: CivilDate,
    latitude: String,
    longitude: String,
    time: String,
    current_latitude: f64,
    current_sidereal: f64,
    painted: bool,
}

/// Detached-only sky chart. The same view survives reattach and can be
/// remounted with different sizes without guessing the observer's location.
pub struct SkyMap {
    root: gtk::Box,
    drawing: gtk::DrawingArea,
    message: gtk::Label,
    example: gtk::Label,
    scene: Rc<RefCell<Scene>>,
    motion: MotionToken,
}

fn entry(title: &str, initial: &str, accessible: &str) -> (gtk::Box, gtk::Entry) {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.add_css_class("sky-field-title");
    row.append(&label);
    let field = gtk::Entry::new();
    field.set_text(initial);
    field.set_width_chars(10);
    field.set_max_length(24);
    field.set_hexpand(true);
    field.update_property(&[gtk::accessible::Property::Label(accessible)]);
    row.append(&field);
    (row, field)
}

impl SkyMap {
    pub fn new(date: CivilDate) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("sky-atlas");
        root.set_hexpand(true);
        root.set_size_request(490, -1);

        let heading = gtk::Label::new(Some("ЗВЁЗДНОЕ НЕБО"));
        heading.set_xalign(0.0);
        heading.add_css_class("sky-title");
        root.append(&heading);

        let workbench = gtk::Box::new(gtk::Orientation::Vertical, 12);
        workbench.add_css_class("sky-workbench");
        workbench.set_hexpand(true);
        root.append(&workbench);

        let example = gtk::Label::new(Some("ТОЧКА НАБЛЮДЕНИЯ · ДЕМО: ГРИНВИЧ"));
        example.add_css_class("sky-example");
        example.set_xalign(0.0);
        workbench.append(&example);

        let controls = gtk::Grid::new();
        controls.set_column_spacing(7);
        controls.set_row_spacing(7);
        let (lat_row, latitude) = entry("Широта, °", GREENWICH_LAT, "Широта наблюдателя");
        let (lon_row, longitude) = entry("Долгота, ° E", GREENWICH_LON, "Долгота наблюдателя");
        let (time_row, clock) = entry("Время, ЧЧ:ММ", DEFAULT_TIME, "Локальное время наблюдения");
        controls.attach(&lat_row, 0, 0, 1, 1);
        controls.attach(&lon_row, 1, 0, 1, 1);
        controls.attach(&time_row, 2, 0, 1, 1);
        let reset = gtk::Button::with_label("СБРОС");
        reset.set_valign(gtk::Align::End);
        reset.add_css_class("sky-reset");
        reset.set_tooltip_text(Some("Вернуть демонстрационные координаты Гринвича"));
        reset.update_property(&[gtk::accessible::Property::Label(
            "Вернуть демонстрационные координаты Гринвича",
        )]);
        controls.attach(&reset, 3, 0, 1, 1);
        workbench.append(&controls);

        let local = gtk::Label::new(Some(
            "Часовой пояс: системный локальный · север и восток: положительные",
        ));
        local.add_css_class("sky-disclaimer");
        local.set_xalign(0.0);
        local.set_wrap(true);
        workbench.append(&local);

        let drawing = gtk::DrawingArea::new();
        drawing.set_content_width(500);
        drawing.set_content_height(410);
        drawing.set_hexpand(true);
        drawing.set_vexpand(true);
        drawing.add_css_class("sky-horizon");
        drawing.update_property(&[gtk::accessible::Property::Label(
            "Звёздное небо над горизонтом: север сверху, восток справа",
        )]);
        workbench.append(&drawing);

        let message = gtk::Label::new(None);
        message.set_xalign(0.0);
        message.set_wrap(true);
        message.add_css_class("sky-disclaimer");
        workbench.append(&message);

        let footer = gtk::Label::new(Some(
            "XHIP / Hipparcos · d3-celestial · J2000. Приближённая карта, не для навигации.",
        ));
        footer.set_xalign(0.0);
        footer.set_wrap(true);
        footer.add_css_class("sky-disclaimer");
        root.append(&footer);

        let atlas = Rc::new(Self {
            root,
            drawing: drawing.clone(),
            message,
            example,
            scene: Rc::new(RefCell::new(Scene {
                date,
                latitude: GREENWICH_LAT.into(),
                longitude: GREENWICH_LON.into(),
                time: DEFAULT_TIME.into(),
                current_latitude: 51.4779,
                current_sidereal: 0.0,
                painted: false,
            })),
            motion: MotionToken::default(),
        });

        let weak = Rc::downgrade(&atlas);
        drawing.set_draw_func(move |_, cr, width, height| {
            if let Some(atlas) = weak.upgrade() {
                atlas.paint(cr, width, height);
            }
        });
        let weak = Rc::downgrade(&atlas);
        drawing.connect_unmap(move |_| {
            if let Some(atlas) = weak.upgrade() {
                atlas.motion.cancel();
                atlas.scene.borrow_mut().painted = false;
            }
        });
        let weak = Rc::downgrade(&atlas);
        latitude.connect_changed(move |field| {
            if let Some(atlas) = weak.upgrade() {
                atlas.scene.borrow_mut().latitude = field.text().to_string();
                atlas.update();
            }
        });
        let weak = Rc::downgrade(&atlas);
        longitude.connect_changed(move |field| {
            if let Some(atlas) = weak.upgrade() {
                atlas.scene.borrow_mut().longitude = field.text().to_string();
                atlas.update();
            }
        });
        let weak = Rc::downgrade(&atlas);
        clock.connect_changed(move |field| {
            if let Some(atlas) = weak.upgrade() {
                atlas.scene.borrow_mut().time = field.text().to_string();
                atlas.update();
            }
        });
        reset.connect_clicked(move |_| {
            latitude.set_text(GREENWICH_LAT);
            longitude.set_text(GREENWICH_LON);
        });
        atlas.update();
        atlas
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_date(&self, date: CivilDate) {
        if self.scene.borrow().date != date {
            self.scene.borrow_mut().date = date;
            self.update();
        }
    }

    pub fn set_visible(&self, visible: bool) {
        self.root.set_visible(visible);
        if !visible {
            self.motion.cancel();
            self.scene.borrow_mut().painted = false;
        } else {
            self.update();
        }
    }

    fn update(&self) {
        let target = {
            let scene = self.scene.borrow();
            let observer = observer_coordinates(&scene.latitude, &scene.longitude);
            let unix = local_observation_unix(scene.date, &scene.time);
            observer.zip(unix).map(|(observer, when)| {
                (
                    observer.latitude,
                    sidereal_degrees(when, observer.longitude),
                )
            })
        };
        self.motion.cancel();
        let Some((latitude, sidereal)) = target else {
            self.message.set_text(
                "Нет карты: проверьте широту (±90°), долготу (±180°), время ЧЧ:ММ и год 1900–2100.",
            );
            self.drawing.set_visible(false);
            self.scene.borrow_mut().painted = false;
            return;
        };
        let (date, time, observer_label) = {
            let scene = self.scene.borrow();
            let demo =
                scene.latitude.trim() == GREENWICH_LAT && scene.longitude.trim() == GREENWICH_LON;
            (
                scene.date.iso(),
                scene.time.clone(),
                if demo {
                    "ТОЧКА НАБЛЮДЕНИЯ · ДЕМО: ГРИНВИЧ"
                } else {
                    "ТОЧКА НАБЛЮДЕНИЯ · ВВЕДЕНО ВРУЧНУЮ"
                },
            )
        };
        self.example.set_label(observer_label);
        self.message.set_label(&format!(
            "{date} · {time} (локальное время) · звёзды до 5,35 mag"
        ));
        self.drawing.set_visible(true);
        self.drawing
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "Звёздное небо {date} в {time}, север сверху, восток справа"
            ))]);

        let (initial_latitude, initial_sidereal, animate) = {
            let mut scene = self.scene.borrow_mut();
            let initial = (
                scene.current_latitude,
                scene.current_sidereal,
                scene.painted && self.drawing.is_mapped(),
            );
            if !initial.2 {
                scene.current_latitude = latitude;
                scene.current_sidereal = sidereal;
            }
            scene.painted = true;
            initial
        };
        if !animate {
            self.drawing.queue_draw();
            return;
        }
        let delta = shortest_sidereal_delta(initial_sidereal, sidereal);
        // Every frame mutates the same scene that Cairo paints. A new change
        // cancels the prior tick and starts from its last *displayed* point.
        let scene = self.scene.clone();
        let area = self.drawing.downgrade();
        self.motion.run(
            &self.drawing.clone().upcast(),
            ANIMATION_MS,
            move |cursor| {
                let eased = 1.0 - (1.0 - cursor).powi(3);
                {
                    let mut current = scene.borrow_mut();
                    current.current_latitude =
                        initial_latitude + (latitude - initial_latitude) * eased;
                    current.current_sidereal = initial_sidereal + delta * eased;
                }
                if let Some(area) = area.upgrade() {
                    area.queue_draw();
                }
            },
            move || {},
        );
    }

    fn paint(&self, cr: &gtk::cairo::Context, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        let (latitude, sidereal) = {
            let scene = self.scene.borrow();
            (scene.current_latitude, scene.current_sidereal)
        };
        paint_sky(cr, width, height, latitude, sidereal);
    }
}

fn paint_sky(cr: &gtk::cairo::Context, width: i32, height: i32, latitude: f64, sidereal: f64) {
    let w = f64::from(width);
    let h = f64::from(height);
    cr.set_source_rgb(20.0 / 255.0, 33.0 / 255.0, 61.0 / 255.0);
    let _ = cr.paint();
    let radius = ((w - 64.0).min(h - 58.0) / 2.0).max(1.0);
    let center_x = w / 2.0;
    let center_y = h / 2.0;

    let _ = cr.save();
    cr.translate(center_x, center_y);
    cr.arc(0.0, 0.0, radius, 0.0, TAU);
    cr.clip();
    cr.set_source_rgb(0.0, 0.0, 0.0);
    let _ = cr.paint();

    cr.set_line_width(1.0);
    cr.set_source_rgba(0.9, 0.9, 0.9, 0.15);
    for altitude in [30.0_f64, 60.0] {
        cr.arc(0.0, 0.0, radius * (90.0 - altitude) / 90.0, 0.0, TAU);
        let _ = cr.stroke();
    }
    cr.set_source_rgba(0.9, 0.9, 0.9, 0.09);
    for (x1, y1, x2, y2) in [(-radius, 0.0, radius, 0.0), (0.0, -radius, 0.0, radius)] {
        cr.move_to(x1, y1);
        cr.line_to(x2, y2);
        let _ = cr.stroke();
    }

    cr.set_line_width(0.85);
    cr.set_source_rgba(0.988, 0.639, 0.067, 0.48);
    for [ra1, dec1, ra2, dec2] in CONSTELLATION_SEGMENTS {
        let mut delta = ra2 - ra1;
        if delta > 180.0 {
            delta -= 360.0;
        }
        if delta < -180.0 {
            delta += 360.0;
        }
        let mut active = false;
        for step in 0..=8 {
            let t = f64::from(step) / 8.0;
            let point = horizon_point(
                ra1 + delta * t,
                dec1 + (dec2 - dec1) * t,
                latitude,
                sidereal,
            );
            if point.altitude < 0.0 {
                active = false;
            } else if !active {
                cr.move_to(point.x * radius, point.y * radius);
                active = true;
            } else {
                cr.line_to(point.x * radius, point.y * radius);
            }
        }
        let _ = cr.stroke();
    }

    for [ra, dec, magnitude] in STARS {
        let p = horizon_point(*ra, *dec, latitude, sidereal);
        if p.altitude < 0.0 {
            continue;
        }
        let size = if *magnitude < 1.4 {
            2.6
        } else if *magnitude < 2.6 {
            1.8
        } else if *magnitude < 4.0 {
            1.2
        } else {
            0.72
        };
        if *magnitude < 2.0 {
            cr.set_source_rgb(1.0, 1.0, 1.0);
        } else {
            cr.set_source_rgba(0.9, 0.9, 0.9, 0.78);
        }
        cr.arc(p.x * radius, p.y * radius, size, 0.0, TAU);
        let _ = cr.fill();
    }

    cr.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    cr.set_font_size(10.0);
    let mut occupied = Vec::<(f64, f64)>::new();
    for &(name, ra, dec) in CONSTELLATION_LABELS {
        let point = horizon_point(ra, dec, latitude, sidereal);
        if point.altitude < 12.0 {
            continue;
        }
        let x = point.x * radius;
        let y = point.y * radius;
        if occupied
            .iter()
            .any(|(px, py)| (px - x).hypot(py - y) < 65.0)
        {
            continue;
        }
        occupied.push((x, y));
        cr.set_source_rgba(0.988, 0.639, 0.067, 0.8);
        cr.move_to(x, y);
        let _ = cr.show_text(name);
    }
    for &(name, ra, dec) in NAMED_STARS {
        let point = horizon_point(ra, dec, latitude, sidereal);
        if point.altitude < 7.0 {
            continue;
        }
        let x = point.x * radius;
        let y = point.y * radius;
        if occupied
            .iter()
            .any(|(px, py)| (px - x).hypot(py - y) < 40.0)
        {
            continue;
        }
        occupied.push((x, y));
        cr.set_source_rgba(0.9, 0.9, 0.9, 0.9);
        cr.move_to(x + 5.0, y - 6.0);
        let _ = cr.show_text(name);
    }
    let _ = cr.restore();

    cr.set_line_width(1.5);
    cr.set_source_rgba(0.9, 0.9, 0.9, 0.35);
    cr.arc(center_x, center_y, radius, 0.0, TAU);
    let _ = cr.stroke();
    cr.set_source_rgb(0.988, 0.639, 0.067);
    cr.set_font_size(11.0);
    for (label, dx, dy) in [
        ("N", 0.0, -1.0),
        ("E", 1.0, 0.0),
        ("S", 0.0, 1.0),
        ("W", -1.0, 0.0),
    ] {
        cr.move_to(
            center_x + dx * (radius + 16.0),
            center_y + dy * (radius + 16.0),
        );
        let _ = cr.show_text(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_atlas_limits_match_reference() {
        assert_eq!(ANIMATION_MS, 420);
        assert_eq!(GREENWICH_LAT, "51.4779");
        assert_eq!(GREENWICH_LON, "0");
        assert!(CONSTELLATION_SEGMENTS.len() > 500);
        assert!(STARS.len() > 2000);
    }
}
