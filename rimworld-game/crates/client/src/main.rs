//! iron-sim-colony native launcher (eframe 0.37). Mirrors rts crates/ui/main.rs structure.

use client::app::UiApp;
use client::diag;

fn main() -> eframe::Result<()> {
    diag::init("colony");
    diag::line("backend=glow requested, building NativeOptions");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1120.0, 800.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let r = eframe::run_native(
        "Iron-Sim Colony (deterministic)",
        options,
        Box::new(|_cc| {
            diag::line("app closure invoked");
            Ok(Box::new(UiApp::new()))
        }),
    );
    diag::line(&format!("run_native returned {:?}", r));
    r
}
