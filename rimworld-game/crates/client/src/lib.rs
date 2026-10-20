//! Client renderer trait (stub). Client code is OUT of the sim determinism scope:
//! view types may use f64 — sim crates must not.

#[derive(Clone, Debug)]
pub struct View {
    pub tick: u64,
    pub pawns: Vec<(u32, f64, f64)>,
}

pub trait Renderer {
    fn render(&mut self, view: &View);
    fn collect_input(&mut self, tick: u64) -> Vec<(u32, shared::Command)>;
}

#[derive(Default)]
pub struct NullRenderer {
    pub seen: u64,
}

impl Renderer for NullRenderer {
    fn render(&mut self, view: &View) {
        self.seen = view.tick;
    }
    fn collect_input(&mut self, _tick: u64) -> Vec<(u32, shared::Command)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_renderer_tracks_tick() {
        let mut r = NullRenderer::default();
        r.render(&View {
            tick: 42,
            pawns: vec![],
        });
        assert_eq!(r.seen, 42);
    }
}
