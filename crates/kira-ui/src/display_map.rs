//! Source-backed, offline GTK display layout map. No Niri IPC or filesystem IO.
//! The Shell alone decides whether typed layout changes can be applied.
use std::cell::RefCell;
use std::f64::consts::TAU;
use std::rc::Rc;

use gtk::prelude::*;

use crate::displays::{DisplayChoice, DisplayOutput};

const MAP_HEIGHT: f64 = 230.0;
const PADDING_X: f64 = 14.0;
const PADDING_Y: f64 = 18.0;
const MAX_ZOOM: f64 = 0.22;
const SNAP_DISTANCE: i32 = 60;

#[derive(Clone, Debug, PartialEq)]
pub struct MapRect {
    pub index: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapIntent {
    Select(usize),
    Move { index: usize, x: i32, y: i32 },
}

/// Prefer the selected advertised mode over the stale observed dimensions.
/// An unknown mode is omitted from the map instead of inventing a monitor.
pub fn logical_size(output: &DisplayOutput, choice: &DisplayChoice) -> Option<(f64, f64)> {
    if !choice.scale.is_finite() || !(0.5..=4.0).contains(&choice.scale) {
        return None;
    }
    let mode = output.modes.iter().find(|mode| mode.id == choice.mode)?;
    let (dimensions, _) = mode.id.split_once('@')?;
    let (width, height) = dimensions.split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    if !(320..=16_384).contains(&width) || !(200..=16_384).contains(&height) {
        return None;
    }
    let rotated = matches!(
        choice.transform.as_str(),
        "90" | "270" | "flipped-90" | "flipped-270"
    );
    let (width, height) = if rotated {
        (height, width)
    } else {
        (width, height)
    };
    Some((
        f64::from(width) / choice.scale,
        f64::from(height) / choice.scale,
    ))
}

/// All screens share one linear transform; their screen-space positions match
/// the observed relative arrangement, including negative logical coordinates.
pub fn layout_rectangles(
    outputs: &[DisplayOutput],
    choices: &[DisplayChoice],
    viewport_width: f64,
    viewport_height: f64,
) -> Vec<MapRect> {
    if !viewport_width.is_finite()
        || !viewport_height.is_finite()
        || viewport_width <= 2.0 * PADDING_X
        || viewport_height <= 2.0 * PADDING_Y
    {
        return Vec::new();
    }
    let sized: Vec<_> = choices
        .iter()
        .enumerate()
        .filter(|(_, choice)| choice.enabled)
        .filter_map(|(index, choice)| {
            let output = outputs
                .iter()
                .find(|output| output.connector == choice.connector)?;
            let (width, height) = logical_size(output, choice)?;
            Some((
                index,
                f64::from(choice.x),
                f64::from(choice.y),
                width,
                height,
            ))
        })
        .collect();
    if sized.is_empty() {
        return Vec::new();
    }
    let min_x = sized
        .iter()
        .map(|(_, x, _, _, _)| *x)
        .fold(f64::INFINITY, f64::min);
    let min_y = sized
        .iter()
        .map(|(_, _, y, _, _)| *y)
        .fold(f64::INFINITY, f64::min);
    let max_x = sized
        .iter()
        .map(|(_, x, _, w, _)| x + w)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = sized
        .iter()
        .map(|(_, _, y, _, h)| y + h)
        .fold(f64::NEG_INFINITY, f64::max);
    let zoom = MAX_ZOOM
        .min((viewport_width - 2.0 * PADDING_X) / (max_x - min_x).max(1.0))
        .min((viewport_height - 2.0 * PADDING_Y) / (max_y - min_y).max(1.0));
    sized
        .into_iter()
        .map(|(index, x, y, width, height)| MapRect {
            index,
            x: PADDING_X + (x - min_x) * zoom,
            y: PADDING_Y + (y - min_y) * zoom,
            width: width * zoom,
            height: height * zoom,
            zoom,
        })
        .collect()
}

fn snapped_axis(raw: i32, edges: impl IntoIterator<Item = i32>) -> i32 {
    edges
        .into_iter()
        .filter(|edge| (i64::from(*edge) - i64::from(raw)).abs() < i64::from(SNAP_DISTANCE))
        .min_by_key(|edge| ((i64::from(*edge) - i64::from(raw)).abs(), *edge))
        .unwrap_or(raw)
        .clamp(-32_000, 32_000)
}

/// Source-aligned nearest-edge snapping. The nearest candidate wins, not the
/// last connector in enumeration order. Invalid/disabled outputs cannot snap.
pub fn snap_position(
    outputs: &[DisplayOutput],
    choices: &[DisplayChoice],
    index: usize,
    raw_x: i32,
    raw_y: i32,
) -> Option<(i32, i32)> {
    let moving = choices.get(index)?;
    if !moving.enabled {
        return None;
    }
    let output = outputs
        .iter()
        .find(|output| output.connector == moving.connector)?;
    let (self_width, self_height) = logical_size(output, moving)?;
    let width = self_width.round() as i32;
    let height = self_height.round() as i32;
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for other in choices
        .iter()
        .filter(|other| other.enabled && other.connector != moving.connector)
    {
        let Some(source) = outputs
            .iter()
            .find(|output| output.connector == other.connector)
        else {
            continue;
        };
        let Some((w, h)) = logical_size(source, other) else {
            continue;
        };
        xs.extend([
            other.x,
            other.x.saturating_add(w.round() as i32),
            other.x.saturating_sub(width),
        ]);
        ys.extend([
            other.y,
            other.y.saturating_add(h.round() as i32),
            other.y.saturating_sub(height),
        ]);
    }
    Some((snapped_axis(raw_x, xs), snapped_axis(raw_y, ys)))
}

struct Drag {
    index: usize,
    start_x: i32,
    start_y: i32,
    zoom: f64,
}

struct State {
    outputs: Vec<DisplayOutput>,
    choices: Vec<DisplayChoice>,
    selected: Option<usize>,
    editing: bool,
    drag: Option<Drag>,
    on_intent: Rc<dyn Fn(MapIntent)>,
}

pub struct DisplayMap {
    drawing: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
}

impl DisplayMap {
    pub fn new(on_intent: impl Fn(MapIntent) + 'static) -> Self {
        let drawing = gtk::DrawingArea::new();
        drawing.add_css_class("display-layout-map");
        drawing.set_content_height(MAP_HEIGHT as i32);
        drawing.set_content_width(350);
        drawing.set_hexpand(true);
        drawing.set_focusable(true);
        drawing.update_property(&[gtk::accessible::Property::Label(
            "Схема экранов. Выберите монитор мышью или в списке ниже. Стрелки сдвигают выбранный экран на десять логических пикселей, Shift плюс стрелки на один.",
        )]);
        let state = Rc::new(RefCell::new(State {
            outputs: Vec::new(),
            choices: Vec::new(),
            selected: None,
            editing: false,
            drag: None,
            on_intent: Rc::new(on_intent),
        }));

        let weak = Rc::downgrade(&state);
        drawing.set_draw_func(move |_, cr, width, height| {
            if let Some(state) = weak.upgrade() {
                paint(cr, f64::from(width), f64::from(height), &state.borrow());
            }
        });

        let drag = gtk::GestureDrag::new();
        let weak = Rc::downgrade(&state);
        let view = drawing.downgrade();
        drag.connect_drag_begin(move |_, x, y| {
            let Some(state) = weak.upgrade() else { return };
            let selected = {
                let mut current = state.borrow_mut();
                let rects = layout_rectangles(
                    &current.outputs,
                    &current.choices,
                    f64::from(view.upgrade().map_or(350, |area| area.width())),
                    MAP_HEIGHT,
                );
                let hit = rects.into_iter().rev().find(|rect| {
                    x >= rect.x
                        && y >= rect.y
                        && x <= rect.x + rect.width.max(30.0)
                        && y <= rect.y + rect.height.max(28.0)
                });
                hit.map(|rect| {
                    current.selected = Some(rect.index);
                    current.drag = if current.editing {
                        current.choices.get(rect.index).map(|choice| Drag {
                            index: rect.index,
                            start_x: choice.x,
                            start_y: choice.y,
                            zoom: rect.zoom,
                        })
                    } else {
                        None
                    };
                    rect.index
                })
            };
            if let Some(index) = selected {
                let notify = state.borrow().on_intent.clone();
                notify(MapIntent::Select(index));
            }
            if let Some(view) = view.upgrade() {
                view.queue_draw();
            }
        });

        let weak = Rc::downgrade(&state);
        drag.connect_drag_update(move |_, offset_x, offset_y| {
            let Some(state) = weak.upgrade() else { return };
            let action = {
                let current = state.borrow();
                let Some(drag) = current.drag.as_ref() else {
                    return;
                };
                if !current.editing || drag.zoom <= 0.0 {
                    return;
                }
                let clamp = |value: f64| value.round().clamp(-32_000.0, 32_000.0) as i32;
                let x = clamp(f64::from(drag.start_x) + offset_x / drag.zoom);
                let y = clamp(f64::from(drag.start_y) + offset_y / drag.zoom);
                snap_position(&current.outputs, &current.choices, drag.index, x, y).map(|(x, y)| {
                    MapIntent::Move {
                        index: drag.index,
                        x,
                        y,
                    }
                })
            };
            if let Some(action) = action {
                let notify = state.borrow().on_intent.clone();
                notify(action);
            }
        });
        let weak = Rc::downgrade(&state);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(state) = weak.upgrade() {
                state.borrow_mut().drag = None;
            }
        });
        drawing.add_controller(drag);

        let weak = Rc::downgrade(&state);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, keyval, _, modifier| {
            let Some(state) = weak.upgrade() else {
                return gtk::glib::Propagation::Proceed;
            };
            let step = if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                1
            } else {
                10
            };
            let offset = match keyval {
                gtk::gdk::Key::Left => Some((-step, 0)),
                gtk::gdk::Key::Right => Some((step, 0)),
                gtk::gdk::Key::Up => Some((0, -step)),
                gtk::gdk::Key::Down => Some((0, step)),
                _ => None,
            };
            let action = {
                let current = state.borrow();
                if !current.editing {
                    return gtk::glib::Propagation::Proceed;
                }
                offset.and_then(|(dx, dy)| {
                    let index = current.selected?;
                    let item = current.choices.get(index)?;
                    snap_position(
                        &current.outputs,
                        &current.choices,
                        index,
                        item.x.saturating_add(dx),
                        item.y.saturating_add(dy),
                    )
                    .map(|(x, y)| MapIntent::Move { index, x, y })
                })
            };
            if let Some(action) = action {
                let notify = state.borrow().on_intent.clone();
                notify(action);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        drawing.add_controller(key);
        Self { drawing, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.drawing.clone().upcast()
    }

    pub fn sync(
        &self,
        outputs: &[DisplayOutput],
        choices: &[DisplayChoice],
        selected: Option<usize>,
        editing: bool,
    ) {
        {
            let mut state = self.state.borrow_mut();
            state.outputs = outputs.to_vec();
            state.choices = choices.to_vec();
            state.selected = selected;
            state.editing = editing;
            if !editing {
                state.drag = None;
            }
        }
        self.drawing.queue_draw();
    }
}

fn paint(cr: &gtk::cairo::Context, width: f64, height: f64, state: &State) {
    cr.set_source_rgb(0.0, 0.0, 0.0);
    let _ = cr.paint();
    let rects = layout_rectangles(&state.outputs, &state.choices, width, height);
    if rects.is_empty() {
        cr.set_source_rgb(0.9, 0.9, 0.9);
        cr.move_to(18.0, height / 2.0);
        let _ = cr.show_text("Нет включённых экранов для схемы");
        return;
    }
    for rect in &rects {
        let is_selected = state.selected == Some(rect.index);
        let w = rect.width.max(30.0);
        let h = rect.height.max(28.0);
        cr.set_source_rgb(20.0 / 255.0, 33.0 / 255.0, 61.0 / 255.0);
        cr.rectangle(rect.x, rect.y, w, h);
        let _ = cr.fill_preserve();
        cr.set_source_rgb(
            if is_selected {
                252.0 / 255.0
            } else {
                229.0 / 255.0
            },
            if is_selected {
                163.0 / 255.0
            } else {
                229.0 / 255.0
            },
            if is_selected {
                17.0 / 255.0
            } else {
                229.0 / 255.0
            },
        );
        cr.set_line_width(if is_selected { 2.0 } else { 1.0 });
        let _ = cr.stroke();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.select_font_face(
            "Sans",
            gtk::cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Bold,
        );
        cr.set_font_size(12.0);
        cr.move_to(rect.x + 6.0, rect.y + 15.0);
        let _ = cr.show_text(&(rect.index + 1).to_string());
        if w > 85.0 && h > 45.0 {
            cr.set_font_size(10.0);
            cr.move_to(rect.x + 6.0, rect.y + 30.0);
            let label = &state.choices[rect.index].connector;
            let limited: String = label
                .chars()
                .filter(|ch| !ch.is_control())
                .take(20)
                .collect();
            let _ = cr.show_text(&limited);
        }
    }
    // Tiny compass markers prevent a blank map from looking like an error.
    cr.set_source_rgba(0.988, 0.639, 0.067, 0.6);
    cr.arc(width - 13.0, 13.0, 2.0, 0.0, TAU);
    let _ = cr.fill();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::displays::DisplayMode;

    fn output(connector: &str, width: u32, height: u32, x: i32, y: i32) -> DisplayOutput {
        let id = format!("{width}x{height}@60.000");
        DisplayOutput {
            connector: connector.into(),
            description: connector.into(),
            enabled: true,
            modes: vec![DisplayMode {
                id: id.clone(),
                label: id.clone(),
            }],
            mode: id,
            scale: 1.0,
            transform: "normal".into(),
            x,
            y,
            vrr: false,
            vrr_supported: false,
        }
    }

    #[test]
    fn j2000_style_source_layout_preserves_monitor_offsets_and_rotation() {
        let mut first = output("DP-1", 1920, 1080, -1920, 0);
        let second = output("HDMI-1", 2560, 1440, 0, -100);
        first.scale = 1.25;
        first.transform = "90".into();
        let outputs = [first, second];
        let choices = outputs.iter().map(DisplayChoice::from).collect::<Vec<_>>();
        assert_eq!(
            logical_size(&outputs[0], &choices[0]),
            Some((864.0, 1536.0))
        );
        assert_eq!(
            logical_size(&outputs[1], &choices[1]),
            Some((2560.0, 1440.0))
        );
        let rects = layout_rectangles(&outputs, &choices, 500.0, 230.0);
        assert_eq!(rects.len(), 2);
        assert!(rects[0].x < rects[1].x);
        assert!(rects[0].y > rects[1].y);
        assert!(rects.iter().all(|rect| rect.x.is_finite()
            && rect.y.is_finite()
            && rect.width > 0.0
            && rect.height > 0.0));
        assert!(layout_rectangles(&outputs, &choices, 0.0, 230.0).is_empty());
    }

    #[test]
    fn snapping_chooses_closest_alignment_independently_of_source_order() {
        let outputs = [
            output("DP-1", 1920, 1080, 0, 0),
            output("DP-2", 1280, 720, 1920, 0),
            output("DP-3", 640, 480, 3900, 200),
        ];
        let choices = outputs.iter().map(DisplayChoice::from).collect::<Vec<_>>();
        assert_eq!(
            snap_position(&outputs, &choices, 1, 1900, 12),
            Some((1920, 0))
        );
        assert_eq!(
            snap_position(&outputs, &choices, 1, -1275, -715),
            Some((-1280, -720))
        );
        assert_eq!(
            snap_position(&outputs, &choices, 1, 32_001, -33_000),
            Some((32_000, -32_000))
        );
        assert_eq!(snap_position(&outputs, &choices, 0, 1, 1), Some((0, 0)));
    }

    #[test]
    fn unknown_current_mode_never_gets_invented_monitor_geometry() {
        let output = output("DP-1", 1920, 1080, 0, 0);
        let mut choice = DisplayChoice::from(&output);
        choice.mode = "unadvertised".into();
        assert_eq!(logical_size(&output, &choice), None);
        assert!(layout_rectangles(std::slice::from_ref(&output), std::slice::from_ref(&choice), 500.0, 230.0).is_empty());
        assert_eq!(snap_position(&[output], &[choice], 0, 40, 0), None);
    }
}
