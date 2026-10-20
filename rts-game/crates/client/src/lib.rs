//! Client stub: a minimal Renderer trait + mock renderer.
//!
//! No engine API is assumed for rendering (see docs/engine-api.md "NOT assumed"). wgpu/soft
//! renderer plugs into the same trait later. Renderers observe state snapshots; they must never
//! influence sim determinism.

use shared::{Command, SimEvent};

/// Read-only per-tick view handed to a renderer.
pub struct RenderView<'a> {
    pub tick: u64,
    pub units: Vec<UnitView>,
    pub events: &'a [SimEvent],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitView {
    pub id: u32,
    pub owner: u32,
    pub x: f64,
    pub y: f64,
    pub hp: u32,
}

/// Renderer contract. collect_input may emit commands for the NEXT tick.
pub trait Renderer {
    fn render(&mut self, view: &RenderView);
    fn collect_input(&mut self, tick: u64) -> Vec<(u32, Command)>;
}

/// Mock headless renderer: records seen ticks. Lets CI exercise the client without GPU.
#[derive(Default)]
pub struct MockRenderer {
    pub seen: Vec<u64>,
}
impl MockRenderer {
    pub fn new() -> Self {
        Self::default()
    }
}
impl Renderer for MockRenderer {
    fn render(&mut self, view: &RenderView) {
        self.seen.push(view.tick);
    }
    fn collect_input(&mut self, _tick: u64) -> Vec<(u32, Command)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_renders_ticks() {
        let mut r = MockRenderer::new();
        let ev: [SimEvent; 0] = [];
        r.render(&RenderView {
            tick: 1,
            units: vec![],
            events: &ev,
        });
        r.render(&RenderView {
            tick: 2,
            units: vec![],
            events: &ev,
        });
        assert_eq!(r.seen, vec![1, 2]);
    }
}
