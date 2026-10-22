//! iron-sim-colony native launcher (eframe 0.37). Mirrors rts crates/ui/main.rs structure.

use client::app::UiApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1120.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Iron-Sim Colony (deterministic)",
        options,
        Box::new(|_cc| Ok(Box::new(UiApp::new()))),
    )
}
