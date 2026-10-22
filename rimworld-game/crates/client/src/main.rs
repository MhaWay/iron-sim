//! iron-sim-colony native launcher (eframe 0.37).
//!
//! Backend flags: --glow (default), --wgpu[=BACKEND] (vulkan|dx12|dx11|gl|metal)

use client::app::UiApp;
use client::diag;

fn main() -> eframe::Result<()> {
    diag::init("colony");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut renderer = eframe::Renderer::Glow;
    for a in &args {
        if a == "--glow" {
            renderer = eframe::Renderer::Glow;
        } else if a == "--wgpu" || a.starts_with("--wgpu=") {
            renderer = eframe::Renderer::Wgpu;
            if let Some(b) = a.split_once('=') {
                std::env::set_var("WGPU_BACKEND", b.1);
            }
        }
    }
    diag::line(&format!("renderer={renderer:?} args={args:?} WGPU_BACKEND={:?}",
        std::env::var("WGPU_BACKEND").ok()));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1120.0, 800.0]),
        renderer,
        ..Default::default()
    };
    let r = eframe::run_native(
        "Iron-Sim Colony (deterministic)",
        options,
        Box::new(|cc| {
            diag::line("app closure invoked");
            diag::line(&format!("integration_info: {:?}", cc.integration_info));
            Ok(Box::new(UiApp::new()))
        }),
    );
    diag::line(&format!("run_native returned {:?}", r));
    r
}
