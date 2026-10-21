//! Scene: draw one Sim frame. Uses public game APIs; only the render layer may use floats.

use crate::font::draw_text;
use crate::noise::splitmix64;
use crate::palette::palette_for;
use crate::png::Image;
use game::economy::Building;
use game::economy::{building_spec, unit_spec, BuildingStatus};
use game::sim::{Pos, Sim, Unit};

pub const TILE: usize = 8;

/// Render one frame.
pub fn render_frame(sim: &Sim, img: &mut Image, player_civ: u8, tick: u64, locale: &str) {
    let pal = palette_for(player_civ);
    draw_terrain(sim, img, &pal);
    draw_buildings(sim, img, &pal);
    draw_units(sim, img, &pal, tick);
    draw_hud(sim, img, &pal, tick, locale);
}

fn draw_terrain(sim: &Sim, img: &mut Image, pal: &crate::palette::Palette) {
    for y in 0..128usize {
        for x in 0..128usize {
            let blocked = {
                use engine_path::TilePos;
                sim.grid.is_blocked(TilePos {
                    x: x as i16,
                    y: y as i16,
                })
            };
            let xi = (x as i32) / 3;
            let yi = (y as i32) / 3;
            let v = value3(xi, yi, x, y, 0x7117);
            let mut c = if v > 180 {
                pal.ground3
            } else if v > 90 {
                pal.ground2
            } else {
                pal.ground
            };
            if blocked {
                c = pal.rock;
            }
            for dy in 0..TILE {
                for dx in 0..TILE {
                    img.set(x * TILE + dx, y * TILE + dy, c);
                }
            }
            if blocked {
                for d in 0..TILE {
                    img.set(x * TILE + d, y * TILE + d, [30, 30, 36]);
                }
            }
        }
    }
    let mut ns: Vec<(engine_ecs::Entity, &game::economy::ResourceNode)> = sim
        .world
        .storages
        .iter::<game::economy::ResourceNode>()
        .collect();
    ns.sort_by_key(|(e, _)| e.0);
    for (e, n) in ns {
        let p = match sim.world.storages.get::<game::economy::NodePos>(e) {
            Some(p) => (p.x as usize, p.y as usize),
            None => continue,
        };
        if p.0 >= 128 || p.1 >= 128 {
            continue;
        }
        let (bx, by) = (p.0 * TILE + 1, p.1 * TILE + 1);
        match n.slot {
            0 => {
                img.rect(bx + 2, by, 3, 4, pal.node_food);
                img.rect(bx + 3, by + 4, 2, 3, pal.rock);
            }
            1 => {
                img.rect(bx + 1, by + 2, 3, 4, pal.node_wood);
                img.rect(bx + 3, by + 1, 2, 2, pal.node_wood);
                img.rect(bx, by + 5, 4, 2, pal.node_wood);
            }
            2 => {
                img.rect(bx, by + 3, 2, 3, pal.node_gold);
                img.rect(bx + 2, by + 1, 3, 3, pal.node_gold);
                img.rect(bx + 4, by + 5, 2, 2, pal.node_gold);
            }
            _ => {
                img.rect(bx + 1, by + 1, 5, 5, pal.node_mana);
            }
        }
    }
}

fn value3(xi: i32, yi: i32, x: usize, y: usize, seed: u64) -> u32 {
    // lattice spacing = 3 px; origin = xi*3 px; fractional offset within cell, integer fixed-point
    let fx = ((x as i32) * 256 - xi * (3 * 256)).clamp(0, 65535) as u32;
    let fy = ((y as i32) * 256 - yi * (3 * 256)).clamp(0, 65535) as u32;
    let sx = smooth(fx);
    let sy = smooth(fy);
    let h = |ix: i32, iy: i32| {
        (splitmix64(seed ^ ((ix as u64) << 32) ^ ((iy as u64) << 16)) >> 51) as u32
    };
    let v00 = h(xi, yi);
    let v10 = h(xi + 1, yi);
    let v01 = h(xi, yi + 1);
    let v11 = h(xi + 1, yi + 1);
    let v00 = v00 as i64;
    let v10 = v10 as i64;
    let v01 = v01 as i64;
    let v11 = v11 as i64;
    let sx = sx as i64;
    let sy = sy as i64;
    let x1 = v00 + ((v10 - v00) * sx) / 256;
    let x2 = v01 + ((v11 - v01) * sx) / 256;
    (x1 + ((x2 - x1) * sy) / 256) as u32
}
#[inline]
fn smooth(t: u32) -> u32 {
    let t = t.min(255);
    ((t * t / 255) * (3 * 255 - 2 * t)) / 255
}

fn draw_buildings(sim: &Sim, img: &mut Image, pal: &crate::palette::Palette) {
    let mut bs: Vec<(engine_ecs::Entity, &Building)> =
        sim.world.storages.iter::<Building>().collect();
    bs.sort_by_key(|(e, _)| e.0);
    for (e, b) in bs {
        if b.status == BuildingStatus::Wreck {
            continue;
        }
        let spec = building_spec(b.kind);
        let p = match sim.world.storages.get::<Pos>(e) {
            Some(p) => (p.x.to_int_trunc(), p.y.to_int_trunc()),
            None => continue,
        };
        let x = (p.0 - spec.w as i32 / 2) as usize * TILE;
        let y = (p.1 - spec.h as i32 / 2) as usize * TILE;
        let w = spec.w as usize * TILE;
        let h = spec.h as usize * TILE;
        let done = b.status == BuildingStatus::Done;
        let body = if done { pal.accent } else { pal.rock };
        img.rect(x + 1, y + 1, w - 2, h - 2, body);
        if b.kind == 0 {
            if pal.accent[0] > 150 {
                img.rect(x + 1, y + 1, 1, h / 2, [255, 255, 255]);
                img.rect(x + w - 2, y + 1, 1, h / 2, [255, 255, 255]);
                img.rect(x + w / 2 - 1, y + h / 2 - 1, 3, 3, pal.ground);
            } else {
                img.rect(x + 1, y + 1, 1, 3, [255, 255, 255]);
                img.rect(x + w - 2, y + 1, 1, 3, [255, 255, 255]);
                img.rect(
                    x + w / 2 - 1,
                    (y.saturating_sub(1)) as usize,
                    3,
                    3,
                    [230, 210, 160],
                );
            }
        } else if b.kind == 8 {
            for i in (x..x + w).step_by(2) {
                img.rect(i, y, 1, 1, pal.rock);
            }
            img.rect(x, y + 2, w, h - 4, pal.rock);
        } else if b.kind == 3 || b.kind == 7 {
            img.rect(x + 1, y + h / 2, w - 2, 1, [255, 255, 255]);
        }
        if !done {
            for i in (x + 1..x + w).step_by(3) {
                img.vline(i, y + 1, h - 2, [140, 100, 60]);
            }
        }
        let bw = w.saturating_sub(4);
        let frac = if b.max_hp > 0 {
            (b.hp as usize * 64) / b.max_hp.max(1) as usize
        } else {
            64
        };
        img.rect(x + 2, y.saturating_sub(2), bw, 2, pal.ground);
        img.rect(
            x + 2,
            y.saturating_sub(2),
            ((bw * frac) / 64).max(1),
            2,
            if frac > 24 { pal.hp_good } else { pal.hp_bad },
        );
    }
}

fn draw_units(sim: &Sim, img: &mut Image, pal: &crate::palette::Palette, tick: u64) {
    let anim = ((tick / 6) & 1) as i32;
    let mut us: Vec<(engine_ecs::Entity, &Unit)> = sim.world.storages.iter::<Unit>().collect();
    us.sort_by_key(|(e, _)| e.0);
    for (e, u) in us {
        let p = match sim.world.storages.get::<Pos>(e) {
            Some(p) => (p.x.to_int_trunc(), p.y.to_int_trunc()),
            None => continue,
        };
        let bx = p.0 * TILE as i32;
        let by = p.1 * TILE as i32;
        img.rect((bx + 3) as usize, (by + 3) as usize, 3, 3, pal.unit);
        img.rect(
            (bx + 3 + anim) as usize,
            (by + 1) as usize,
            2,
            2,
            pal.unit_dark,
        );
        img.set((bx + 1) as usize, (by + 6 + anim) as usize, pal.unit_dark);
        img.set((bx + 5 - anim) as usize, (by + 6) as usize, pal.unit_dark);
        let spec = unit_spec(u.kind);
        let max = spec.hp.max(1) as i32;
        let frac = u.hp.max(0) * 8 / max;
        img.rect((bx + 1) as usize, by as usize, 8, 1, pal.ground);
        if frac > 0 {
            img.rect(
                (bx + 1) as usize,
                by as usize,
                frac as usize,
                1,
                if frac > 3 { pal.hp_good } else { pal.hp_bad },
            );
        }
    }
}

fn draw_hud(sim: &Sim, img: &mut Image, pal: &crate::palette::Palette, tick: u64, locale: &str) {
    let w = img.w;
    let h = img.h;
    let panel = 72;
    img.rect(0, h - panel, w, panel, pal.hud);
    img.hline(0, h - panel, w, pal.hud_panel);
    let p = &sim.players[0];
    let rx = w / 2;
    for i in 0..3u8 {
        let name = game::economy::res_display(p.civ, i);
        let v = match i {
            0 => p.stock.food,
            1 => p.stock.wood,
            _ => p.stock.gold,
        };
        let t = format!("{}: {}", name, v);
        draw_text(
            img,
            rx + (i as usize) * 132,
            h - panel + 10,
            &t,
            pal.hud_text,
        );
    }
    let age = format!(
        "{}: {}",
        game::economy::ui(locale, "age"),
        game::economy::age_display(p.age)
    );
    draw_text(img, rx, h - panel + 34, &age, pal.hud_text);
    let score = format!("{}: {}", game::economy::ui(locale, "score"), p.score);
    draw_text(img, rx + 200, h - panel + 34, &score, pal.hud_text);
    let pop: u32 = sim
        .world
        .storages
        .iter::<Unit>()
        .filter(|(_, u)| u.owner == 0)
        .map(|(_, u)| unit_spec(u.kind).pop as u32)
        .sum();
    let tcs = sim
        .world
        .storages
        .iter::<Building>()
        .filter(|(e, b)| {
            b.owner == 0
                && b.kind == 0
                && b.status == BuildingStatus::Done
                && sim.world.storages.get::<Pos>(*e).is_some()
        })
        .count() as u32;
    let farms = sim
        .world
        .storages
        .iter::<Building>()
        .filter(|(e, b)| {
            b.owner == 0
                && b.kind == 1
                && b.status == BuildingStatus::Done
                && sim.world.storages.get::<Pos>(*e).is_some()
        })
        .count() as u32;
    let cap = game::economy::pop_cap_for(tcs, farms);
    draw_text(
        img,
        rx + 400,
        h - panel + 34,
        &format!("{}: {}/{}", game::economy::ui(locale, "pop"), pop, cap),
        pal.hud_text,
    );
    let tstr = format!("T {}", tick);
    draw_text(img, w - 130, 4, &tstr, pal.hud_text);
    let mx = 6;
    let my = h - panel - 6 - 112;
    draw_minimap(sim, img, pal, mx, my);
    img.rect_border(mx - 1, my - 1, 114, 114, pal.mm_frame);
}

fn draw_minimap(sim: &Sim, img: &mut Image, pal: &crate::palette::Palette, mx: usize, my: usize) {
    for y in 0..112 {
        for x in 0..112 {
            let gx = x / 8;
            let gy = y / 8;
            let v = (splitmix64(0x911 ^ ((gx as u64) << 32) ^ ((gy as u64) << 16)) >> 53) as u32;
            let blocked = {
                use engine_path::TilePos;
                sim.grid.is_blocked(TilePos {
                    x: gx as i16,
                    y: gy as i16,
                })
            };
            let c = if blocked {
                pal.rock
            } else if v > 15 {
                pal.ground2
            } else {
                pal.ground
            };
            img.set(mx + x, my + y, c);
        }
    }
    let mut us: Vec<(engine_ecs::Entity, &Unit)> = sim.world.storages.iter::<Unit>().collect();
    us.sort_by_key(|(e, _)| e.0);
    for (e, u) in us {
        if let Some(p) = sim.world.storages.get::<Pos>(e) {
            let x = (p.x.to_int_trunc() / 16) as usize;
            let y = (p.y.to_int_trunc() / 16) as usize;
            img.set(
                mx + x,
                my + y,
                if u.owner == 0 {
                    [255, 255, 100]
                } else {
                    [100, 200, 255]
                },
            );
        }
    }
    let mut bs: Vec<(engine_ecs::Entity, &Building)> =
        sim.world.storages.iter::<Building>().collect();
    bs.sort_by_key(|(e, _)| e.0);
    for (e, b) in bs {
        if b.status != BuildingStatus::Done {
            continue;
        }
        if let Some(p) = sim.world.storages.get::<Pos>(e) {
            let x = (p.x.to_int_trunc() / 16) as usize;
            let y = (p.y.to_int_trunc() / 16) as usize;
            img.rect(mx + x, my + y, 1, 1, pal.accent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_deterministic() {
        let mut s1 = Sim::new(77);
        let mut s2 = Sim::new(77);
        s1.spawn_unit(0, 20, 20);
        s2.spawn_unit(1, 55, 55);
        let mut f1 = Image::new(1000, 1100);
        let mut f2 = Image::new(1000, 1100);
        render_frame(&s1, &mut f1, 0, 0, "en");
        render_frame(&s2, &mut f2, 1, 0, "it");
        assert_eq!(f1.encode(), f1.encode());
    }
}
