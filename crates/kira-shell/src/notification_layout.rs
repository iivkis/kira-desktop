//! Output-local logical-pixel geometry of the one bottom-right toast WebView.
//! The renderer does not make GTK resize itself based on its own viewport.
use gtk::cairo::{RectangleInt, Region};

pub const PREFERRED_WIDTH: i32 = 360;
pub const MIN_HOST_WIDTH: i32 = 240;
pub const OUTER_MARGIN: i32 = 12;
pub const INNER_MARGIN: i32 = 8;
pub const GAP: i32 = 8;
pub const MIN_CARD_HEIGHT: i32 = 58;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: i32,
    pub height: i32,
    pub card_height: i32,
}

pub fn plan(output_width: i32, output_height: i32, bottom_clearance: i32) -> Option<Layout> {
    if output_width <= OUTER_MARGIN * 2 || output_height <= OUTER_MARGIN * 2 {
        return None;
    }
    let width = PREFERRED_WIDTH.min(output_width - OUTER_MARGIN * 2);
    // A 1..15px host makes a zero/negative Cairo input rectangle; keep
    // notifications queued instead of creating an unclickable Wayland pane.
    if width < MIN_HOST_WIDTH { return None; }
    let height = (output_height / 2)
        .min(bottom_clearance - OUTER_MARGIN)
        .max(0);
    let card_height = (height - INNER_MARGIN * 2 - GAP * 2) / 3;
    if card_height < MIN_CARD_HEIGHT {
        return None;
    }
    Some(Layout {
        width,
        height,
        card_height,
    })
}

/// Region contains exactly the actually visible card bounds, never the
/// transparent remainder of the half-screen WebView or spacing between cards.
pub fn hit_region(layout: Layout, visible_count: usize) -> Region {
    let region = Region::create();
    for index in 0..visible_count.min(3) {
        let y = layout.height
            - INNER_MARGIN
            - layout.card_height
            - index as i32 * (layout.card_height + GAP);
        let rect = RectangleInt::new(
            INNER_MARGIN,
            y,
            layout.width - INNER_MARGIN * 2,
            layout.card_height,
        );
        let _ = region.union_rectangle(&rect);
    }
    region
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_height_and_card_at_exactly_one_third() {
        for (width, height) in [(360, 640), (600, 480), (1920, 1080), (3840, 2160)] {
            let layout = plan(width, height, height).expect("full screen has free space");
            assert!(layout.height * 2 <= height);
            assert!(layout.card_height * 3 <= layout.height);
            assert!(layout.width <= width - 24);
            assert!(hit_region(layout, 3).num_rectangles() >= 1);
        }
    }

    #[test]
    fn tiny_outputs_do_not_create_negative_input_rectangles() {
        for width in 1..(MIN_HOST_WIDTH + OUTER_MARGIN * 2) {
            assert!(plan(width, 600, 600).is_none(), "width={width}");
        }
        assert!(plan(MIN_HOST_WIDTH + OUTER_MARGIN * 2, 600, 600).is_some());
    }

    #[test]
    fn all_three_click_regions_are_bounded_and_separated() {
        let layout = plan(900, 700, 700).unwrap();
        assert!(layout.height * 2 <= 700);
        assert!(layout.card_height * 3 <= layout.height);
        let region = hit_region(layout, 3);
        for i in 0..3 {
            let y = layout.height - INNER_MARGIN - layout.card_height
                - i * (layout.card_height + GAP);
            assert!(region.contains_point(layout.width / 2, y + layout.card_height / 2));
            if i < 2 { assert!(!region.contains_point(layout.width / 2, y - GAP / 2)); }
        }
        assert!(!region.contains_point(0, layout.height / 2));
    }

    #[test]
    fn refuses_to_cover_widget_in_bottom_right() {
        assert!(plan(600, 480, 95).is_none());
        assert!(plan(600, 480, 240).is_some());
    }

    #[test]
    fn empty_transparent_area_is_not_clickable() {
        let layout = plan(1920, 1080, 1080).unwrap();
        let region = hit_region(layout, 1);
        assert!(region.contains_point(100, layout.height - 20));
        assert!(!region.contains_point(100, 20));
        assert!(!region.contains_point(1, layout.height - 20));
        assert!(!hit_region(layout, 0).contains_point(100, layout.height - 20));
    }
}
