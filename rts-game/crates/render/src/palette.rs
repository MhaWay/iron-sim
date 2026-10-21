//! Faction palettes (render layer only; never hashed).

#[derive(Clone, Copy)]
pub struct Palette {
    pub ground: [u8; 3],
    pub ground2: [u8; 3],
    pub ground3: [u8; 3],
    pub rock: [u8; 3],
    pub accent: [u8; 3],
    pub unit: [u8; 3],
    pub unit_dark: [u8; 3],
    pub hud: [u8; 3],
    pub hud_text: [u8; 3],
    pub hud_panel: [u8; 3],
    pub hp_good: [u8; 3],
    pub hp_bad: [u8; 3],
    pub mm_frame: [u8; 3],
    pub node_food: [u8; 3],
    pub node_wood: [u8; 3],
    pub node_gold: [u8; 3],
    pub node_mana: [u8; 3],
}

pub const ADEPTUS: Palette = Palette {
    ground: [56, 48, 52],
    ground2: [66, 56, 58],
    ground3: [84, 70, 60],
    rock: [96, 88, 84],
    accent: [210, 170, 96],
    unit: [214, 196, 150],
    unit_dark: [110, 96, 74],
    hud: [40, 34, 38],
    hud_text: [226, 210, 160],
    hud_panel: [58, 50, 54],
    hp_good: [110, 190, 120],
    hp_bad: [196, 80, 70],
    mm_frame: [170, 140, 86],
    node_food: [208, 186, 90],
    node_wood: [120, 90, 60],
    node_gold: [150, 150, 160],
    node_mana: [120, 200, 150],
};

pub const DOMINION: Palette = Palette {
    ground: [34, 42, 52],
    ground2: [42, 54, 68],
    ground3: [50, 68, 84],
    rock: [86, 96, 108],
    accent: [64, 210, 255],
    unit: [150, 220, 250],
    unit_dark: [30, 80, 110],
    hud: [14, 20, 30],
    hud_text: [150, 230, 255],
    hud_panel: [24, 36, 50],
    hp_good: [70, 230, 255],
    hp_bad: [255, 90, 90],
    mm_frame: [64, 210, 255],
    node_food: [90, 235, 120],
    node_wood: [220, 220, 80],
    node_gold: [170, 180, 196],
    node_mana: [90, 160, 255],
};

#[inline]
pub fn palette_for(civ: u8) -> Palette {
    if civ == 0 {
        ADEPTUS
    } else {
        DOMINION
    }
}
