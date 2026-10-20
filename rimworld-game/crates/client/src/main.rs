use client::Renderer;

fn main() {
    let mut r = client::NullRenderer::default();
    r.render(&client::View {
        tick: 42,
        pawns: vec![],
    });
    println!("client demo rendered tick {}", r.seen);
}
