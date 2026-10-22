//! iron-sim native launcher (eframe).
//!
//! Native window; deterministic sim internals — nothing here reads clock/timestamps into sim.

use game_ui::app::UiApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1034.0, 1150.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Iron-Sim RTS (deterministic)",
        options,
        Box::new(|_cc| Ok(Box::new(UiApp::new()))),
    )
}
