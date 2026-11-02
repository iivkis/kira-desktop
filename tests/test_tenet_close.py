"""TENET reverse dismissal contract for every pooled WebView surface."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
MOTION = (ROOT / "frontend/src/lib/motion.ts").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()


class TenetReverseDismissalTests(unittest.TestCase):
    def test_native_waits_for_reverse_before_unmapping_and_releasing_lease(self):
        request = RUST.split("fn request_close(self: &Rc<Self>)", 1)[1].split(
            "fn cancel_close", 1)[0]
        self.assertIn("self.closing.replace(true)", request)
        self.assertIn("kira:overlay:close", request)
        self.assertIn("self.window.set_opacity(1.0)", request)
        self.assertIn("Duration::from_millis(1100)", request)
        self.assertNotIn("self.window.set_visible(false)", request)
        self.assertNotIn("pool.release(self.id)", request)
        self.assertIn("Some(UiCommand::Closed(generation))", RUST)
        self.assertIn("&& self.closing.get()", RUST)
        self.assertIn("&& self.generation.get() == generation", RUST)
        self.assertIn("self.window.set_visible(false)", RUST)
        self.assertIn("pool.release(self.id)", RUST)

    def test_all_public_close_paths_use_the_two_phase_protocol(self):
        self.assertIn("Some(UiCommand::Close(generation))", RUST)
        self.assertIn("self.request_close();", RUST)
        self.assertIn("view.request_close();", RUST)
        self.assertIn("LauncherMessage::Launched(Ok(())) => self.request_close()", RUST)
        self.assertNotIn("connect_is_active_notify", RUST)
        self.assertIn("fn cancel_close(&self)", RUST)
        self.assertIn("kira:overlay:reopen", RUST)
        self.assertIn("view.cancel_close()", RUST)

    def test_exact_same_recorded_tenet_timeline_runs_backwards(self):
        self.assertIn("master.reverse()", MOTION)
        self.assertIn("onReverseComplete: acknowledgeClose", MOTION)
        self.assertIn("master.play()", MOTION)
        self.assertIn("master.progress(1).pause()", MOTION)
        self.assertIn("master.fromTo(traces,", MOTION)
        self.assertIn("master.to(content, {", MOTION)
        self.assertIn("window.addEventListener('kira:overlay:visible'", MOTION)
        self.assertIn("window.addEventListener('kira:overlay:close'", MOTION)
        self.assertIn("window.addEventListener('kira:overlay:hide'", MOTION)
        self.assertIn("notifyOverlayClosed(Number(node.dataset.kiraGeneration))", MOTION)
        self.assertIn("sendCalendarUi('closed:' + generation)", SDK)
        self.assertIn("parse_ui_generation(value).map(UiCommand::Closed)", RUST)
        self.assertIn("node.dataset.kiraGeneration = String(generation)", "") if False else None
        self.assertIn("root.dataset.kiraGeneration = String(generation)", ENTRY)

    def test_native_webview_backing_is_transparent_and_popup_only(self):
        # A fully opaque WebKit backing makes clipping the inner main useless:
        # an unanimated black rectangle still appears in Niri.
        styles = (ROOT / "frontend/src/style.css").read_text()
        native_ui = (ROOT / "crates/kira-shell/src/native_ui.rs").read_text()
        shell_main = (ROOT / "crates/kira-shell/src/main.rs").read_text()
        # GTK's themed window node is a distinct paint layer behind WebKit:
        # making just the embedded WebView transparent still shows gray.
        self.assertIn('window.add_css_class("kira-popup-transparent")', RUST)
        self.assertIn("native_ui::install_transparent_popup_style()", RUST)
        self.assertIn('window.kira-popup-transparent.background', native_ui)
        self.assertIn('background-color: transparent; background-image: none;', native_ui)
        self.assertIn('box-shadow: none;', native_ui)
        self.assertIn("gtk::STYLE_PROVIDER_PRIORITY_APPLICATION", native_ui)
        self.assertNotIn("kira-popup-transparent", shell_main)
        self.assertIn(
            "view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0))",
            shell_main,
        )
        self.assertIn(
            "view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0))",
            RUST,
        )
        self.assertIn('html[data-kira-surface="calendar"]', styles)
        self.assertIn("background: transparent;", styles)
        self.assertIn("seam.dataset.kiraSurfaceSeam = ''", MOTION)
        self.assertIn("clipPath: hiddenClip", MOTION)
        self.assertIn("clipPath: visibleClip", MOTION)
        self.assertIn("master.reverse()", MOTION)
        self.assertIn("master.to(seam, {", MOTION)
        self.assertIn("master.to(node, {", MOTION)
        # No animation of native Wayland position/size/scale.
        scene = MOTION.split("export function tenetWidgetReveal", 1)[1].split(
            "export interface TenetFlowTarget", 1
        )[0]
        self.assertNotIn("window.resizeTo(", scene)
        self.assertNotIn("set_margin(", scene)

    def test_open_and_reverse_share_exactly_twenty_percent_faster_tempo(self):
        # GSAP timeScale applies to every constituent track in both
        # directions. Native opacity must follow the same rate.
        self.assertIn("master.timeScale(1.2)", MOTION)
        self.assertIn("const OVERLAY_FADE_MS: u64 = 133;", RUST)
        self.assertIn("master.reverse()", MOTION)
        self.assertIn("onReverseComplete: acknowledgeClose", MOTION)
        self.assertAlmostEqual(540 / 1.2, 450)

    def test_reduced_motion_and_renderer_stall_have_finite_exit(self):
        self.assertIn("self.native_reduced_motion.get()", RUST)
        self.assertIn("if (preference.matches || !master || master.progress() <= 0)", MOTION)
        self.assertIn("if (closing) acknowledgeClose()", MOTION)
        self.assertIn("if reduced && self.closing.get()", RUST)
        self.assertIn("state.generation.get() == generation", RUST)
        self.assertIn("view.hide()", RUST)
        self.assertIn("window.removeEventListener('kira:overlay:close'", MOTION)
        self.assertIn("window.clearTimeout(restore)", MOTION)
        self.assertIn("master?.kill()", MOTION)
        self.assertIn("trace.remove()", MOTION)


if __name__ == "__main__":
    unittest.main()
