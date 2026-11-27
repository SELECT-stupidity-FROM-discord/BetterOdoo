use std::sync::atomic::{AtomicU32, Ordering};

static SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);

const MIN: f32 = 0.7;
const MAX: f32 = 1.8;

pub fn get() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

pub fn set(v: f32) -> f32 {
    let v = ((v.clamp(MIN, MAX)) * 10.).round() / 10.;
    SCALE.store(v.to_bits(), Ordering::Relaxed);
    v
}

pub fn px(v: f32) -> gpui::Pixels {
    gpui::px(v * get())
}
