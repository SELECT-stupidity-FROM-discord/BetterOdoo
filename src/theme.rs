pub const PAGE: u32 = 0xffffff;
pub const SIDEBAR: u32 = 0xf7f7f5;
pub const HOVER: u32 = 0xefefed;
pub const BORDER: u32 = 0xe9e9e7;
pub const ROW_RULE: u32 = 0xf1f1ef;
pub const TEXT: u32 = 0x37352f;
pub const TEXT_SOFT: u32 = 0x5f5e5b;
pub const MUTED: u32 = 0x6f6e69;
pub const ACCENT: u32 = 0x1f74c8;
pub const DANGER: u32 = 0xb8312f;
pub const SUCCESS: u32 = 0x3a7050;
pub const CHECK: u32 = 0x25a06c;
pub const BORDER_STRONG: u32 = 0xc7c6c2;
pub const SELECTION: u32 = 0x2383e247;

pub const FONT: &str = "Inter";

#[derive(Clone, Copy)]
pub struct Tone {
    pub bg: u32,
    pub fg: u32,
}

pub const GRAY: Tone = Tone {
    bg: 0xf1f1ef,
    fg: 0x5f5e5b,
};
pub const BLUE: Tone = Tone {
    bg: 0xe7f3f8,
    fg: 0x2b6e94,
};
pub const ORANGE: Tone = Tone {
    bg: 0xfaebdd,
    fg: 0xa8550b,
};
pub const GREEN: Tone = Tone {
    bg: 0xedf3ec,
    fg: 0x3a7050,
};
pub const PURPLE: Tone = Tone {
    bg: 0xf6f3f9,
    fg: 0x7a4f9c,
};
pub const STAGE_GRAY: Tone = Tone {
    bg: 0xf1f1ef,
    fg: 0x5f5e5b,
};
pub const STAGE_BLUE: Tone = Tone {
    bg: 0xe9f2f7,
    fg: 0x2d688a,
};
pub const STAGE_AMBER: Tone = Tone {
    bg: 0xf7efe2,
    fg: 0x8f5b1e,
};
pub const STAGE_PURPLE: Tone = Tone {
    bg: 0xf4eff8,
    fg: 0x6f4d8e,
};
pub const STAGE_CYAN: Tone = Tone {
    bg: 0xeaf4f2,
    fg: 0x2f6f6a,
};
pub const STAGE_GREEN: Tone = Tone {
    bg: 0xedf4ee,
    fg: 0x3a6f4f,
};
pub const STAGE_RED: Tone = Tone {
    bg: 0xfaeeee,
    fg: 0x9e3f3d,
};
pub const STAGE_PINK: Tone = Tone {
    bg: 0xf8eef3,
    fg: 0x8f456b,
};
pub const STAGE_ORANGE: Tone = Tone {
    bg: 0xf9ece0,
    fg: 0x9a521b,
};
pub const AI_BORDER: u32 = 0xe6dcef;

pub const PEOPLE: [Tone; 4] = [BLUE, ORANGE, GREEN, PURPLE];

pub const ACTIVE_STEP: u32 = 0xdcebf9;
pub const ACTIVE_STEP_TEXT: u32 = 0x1a5a9e;
