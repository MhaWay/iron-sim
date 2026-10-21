//! demo_v2: play an RTS match, render every frame, write PNG frames + final hash.
//! usage: demo_v2 [--seed N] [--ticks T] [--player-civ adeptus|dominion] [--locale en|it|de|fr|es]
//!         [--out DIR] [--frames K]

use game::run_sim;
use render::demo::demo_commands;
use render::png::Image;
use render::scene::render_frame;
use std::path::PathBuf;

fn parse_args() -> (u64, u64, u8, String, PathBuf, u64) {
    let argv: Vec<String> = std::env::args().collect();
    let mut seed = 12345u64;
    let mut ticks = 1800u64;
    let mut civ = 0u8;
    let mut locale = "en".to_string();
    let mut out = PathBuf::from("frames");
    let mut frames = 30u64;
    let mut i = 1;
    while i < argv.len() {
        let key = argv[i].as_str();
        let val = argv.get(i + 1).cloned().unwrap_or_default();
        match key {
            "--seed" => seed = val.parse().unwrap_or(seed),
            "--ticks" => ticks = val.parse().unwrap_or(ticks),
            "--player-civ" => civ = if val.starts_with("dom") { 1 } else { 0 },
            "--locale" => {
                if ["en", "it", "de", "fr", "es"].contains(&val.as_str()) {
                    locale = val;
                }
            }
            "--out" => out = PathBuf::from(val),
            "--frames" => frames = val.parse().unwrap_or(frames),
            _ => {}
        }
        i += 2;
    }
    (seed, ticks, civ, locale, out, frames)
}

fn main() {
    let (seed, ticks, civ, locale, out, frames) = parse_args();
    std::fs::create_dir_all(&out).expect("create out dir");
    let w = 1024usize;
    let h = 1100usize;
    let step = (ticks / frames.max(1)).max(1);
    let mut img = Image::new(w, h);
    let batches = demo_commands(seed, ticks);
    let h_final = run_sim(seed, ticks, &batches);
    // replay for frames (same stream, deterministic replay)
    let mut sim = game::sim::Sim::new(seed);
    let mut bi = 0usize;
    let mut frame_no = 0u64;
    while (sim.tick()) < ticks {
        if bi < batches.len() && batches[bi].tick == sim.tick() {
            let _ = sim.step(&batches[bi].cmds);
            bi += 1;
        } else {
            let _ = sim.step(&[]);
        }
        if sim.tick().is_multiple_of(step) && frame_no < frames {
            render_frame(&sim, &mut img, civ, sim.tick(), &locale);
            let path = out.join(format!("frame_{:04}.png", frame_no));
            std::fs::write(&path, img.encode()).expect("write frame");
            frame_no += 1;
        }
    }
    println!(
        "frames={} final_hash={:016x} seed={} ticks={} civ={} locale={} out={}",
        frame_no,
        h_final,
        seed,
        ticks,
        if civ == 0 { "adeptus" } else { "dominion" },
        locale,
        out.display()
    );
}
