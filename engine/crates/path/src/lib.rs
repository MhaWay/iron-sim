//! Deterministic grid pathfinding (STUB). Honors contract: rts-game/docs/engine-api.md.
//! 4-connected A*, neighbor expansion order fixed N,E,S,W. Open-list ties broken by total order
//! (min-f, earliest insertion). Same grid + same query -> identical path every run, every platform.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TilePos {
    pub x: i16,
    pub y: i16,
}

#[derive(Clone)]
pub struct Grid {
    w: u16,
    h: u16,
    blocked: Vec<bool>,
}

impl Grid {
    pub fn new(w: u16, h: u16) -> Self {
        Grid {
            w,
            h,
            blocked: vec![false; (w as usize) * (h as usize)],
        }
    }
    pub fn set_blocked(&mut self, p: TilePos, blocked: bool) {
        if self.contains(p) {
            self.blocked[p.y as usize * self.w as usize + p.x as usize] = blocked;
        }
    }
    pub fn is_blocked(&self, p: TilePos) -> bool {
        if !self.contains(p) {
            return true;
        }
        self.blocked[p.y as usize * self.w as usize + p.x as usize]
    }
    pub fn contains(&self, p: TilePos) -> bool {
        p.x >= 0 && (p.x as u32) < self.w as u32 && p.y >= 0 && (p.y as u32) < self.h as u32
    }
    pub fn w(&self) -> u16 {
        self.w
    }
    pub fn h(&self) -> u16 {
        self.h
    }
    pub fn blocked_count(&self) -> usize {
        self.blocked.iter().filter(|b| **b).count()
    }
}

/// Q16.16: straight step = S, heuristic = manhattan*S (4-connected, consistent).
const S: i64 = 1 << 16;

#[inline]
fn heur(ax: i16, ay: i16, bx: i16, by: i16) -> i64 {
    ((ax - bx).abs() as i64 + (ay - by).abs() as i64) * S
}

const NEIGH: [(i16, i16); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)]; // N,E,S,W — fixed order.

#[derive(Clone, Copy, Eq, PartialEq)]
struct Open {
    f: i64,
    order: i64,
    cell: usize,
}
impl Ord for Open {
    fn cmp(&self, o: &Self) -> core::cmp::Ordering {
        // std::collections::BinaryHeap is max-heap; we want min-f-first so pop returns min f.
        self.f.cmp(&o.f).reverse().then_with(|| o.order.cmp(&self.order))
    }
}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

pub fn find_path(grid: &Grid, start: TilePos, goal: TilePos) -> Option<Vec<TilePos>> {
    use std::collections::BinaryHeap;
    if start == goal {
        return Some(Vec::new());
    }
    if grid.is_blocked(goal) {
        return None;
    }
    let w = grid.w as usize;
    let n = w * grid.h as usize;
    let scell = start.y as usize * w + start.x as usize;
    let gcell = goal.y as usize * w + goal.x as usize;
    let mut g = vec![i64::MAX; n];
    let mut came = vec![usize::MAX; n];
    let mut closed = vec![false; n];

    let mut heap: BinaryHeap<Open> = BinaryHeap::new();
    let mut order: i64 = 0;
    heap.push(Open {
        f: heur(start.x, start.y, goal.x, goal.y),
        order,
        cell: scell,
    });
    order += 1;
    g[scell] = 0;

    while let Some(cur) = heap.pop() {
        if closed[cur.cell] {
            continue;
        }
        closed[cur.cell] = true;
        if cur.cell == gcell {
            let mut path = Vec::new();
            let mut c = gcell;
            while c != scell {
                path.push(TilePos {
                    x: (c % w) as i16,
                    y: (c / w) as i16,
                });
                c = came[c];
            }
            path.reverse();
            return Some(path);
        }
        let cx = (cur.cell % w) as i16;
        let cy = (cur.cell / w) as i16;
        for (dx, dy) in NEIGH {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 {
                continue;
            }
            let np = TilePos { x: nx, y: ny };
            if !grid.contains(np) || grid.is_blocked(np) {
                continue;
            }
            let ncell = np.y as usize * w + np.x as usize;
            if closed[ncell] {
                continue;
            }
            let ng = g[cur.cell] + S;
            if ng < g[ncell] {
                g[ncell] = ng;
                came[ncell] = cur.cell;
                heap.push(Open {
                    f: ng + heur(np.x, np.y, goal.x, goal.y),
                    order,
                    cell: ncell,
                });
                order += 1;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn determinism_same_input() {
        let mut g = Grid::new(24, 24);
        for i in 0..20 {
            g.set_blocked(
                TilePos {
                    x: (i % 23) as i16,
                    y: (i / 2 + 1) as i16,
                },
                true,
            );
        }
        let a = find_path(&g, TilePos { x: 1, y: 1 }, TilePos { x: 22, y: 22 }).unwrap();
        let b = find_path(&g, TilePos { x: 1, y: 1 }, TilePos { x: 22, y: 22 }).unwrap();
        assert_eq!(a, b);
    }
    #[test]
    fn blocked_wall_no_path() {
        let mut g = Grid::new(8, 8);
        for x in 0..8 {
            g.set_blocked(TilePos { x, y: 4 }, true);
        }
        assert!(find_path(&g, TilePos { x: 1, y: 1 }, TilePos { x: 6, y: 6 }).is_none());
    }
    #[test]
    fn straight_path_exact() {
        let g = Grid::new(8, 8);
        let p = find_path(&g, TilePos { x: 0, y: 3 }, TilePos { x: 7, y: 3 }).unwrap();
        assert_eq!(
            p,
            vec![
                TilePos { x: 1, y: 3 },
                TilePos { x: 2, y: 3 },
                TilePos { x: 3, y: 3 },
                TilePos { x: 4, y: 3 },
                TilePos { x: 5, y: 3 },
                TilePos { x: 6, y: 3 },
                TilePos { x: 7, y: 3 }
            ]
        );
    }
    #[test]
    fn no_diagonal_shortcut() {
        let mut g = Grid::new(6, 6);
        g.set_blocked(TilePos { x: 1, y: 0 }, true);
        g.set_blocked(TilePos { x: 0, y: 1 }, true);
        assert!(find_path(&g, TilePos { x: 0, y: 0 }, TilePos { x: 5, y: 5 }).is_none());
    }
    #[test]
    fn start_goal_same() {
        let g = Grid::new(5, 5);
        assert_eq!(
            find_path(&g, TilePos { x: 2, y: 2 }, TilePos { x: 2, y: 2 }),
            Some(Vec::new())
        );
    }
}
