//! iron-sim native launcher (eframe).
//!
//! Native window; deterministic sim internals — nothing here reads clock/timestamps into sim.

use game_ui::app::UiApp;
use game_ui::diag;

fn main() -> eframe::Result<()> {
    diag::init("iron-sim");
    diag::line("backend=glow requested, building NativeOptions");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1034.0, 1150.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let r = eframe::run_native(
        "Iron-Sim RTS (deterministic)",
        options,
        Box::new(|_cc| {
            diag::line("app closure invoked");
            Ok(Box::new(UiApp::new()))
        }),
    );
    diag::line(&format!("run_native returned {:?}", r));
    r
}
