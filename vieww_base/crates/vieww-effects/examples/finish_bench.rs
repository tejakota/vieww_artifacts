//! Times the finishing stack on a 1920×1080 frame, stage by stage.
use std::time::Instant;
use vieww_effects::cpu::{grade, optics};
fn main() {
    let (w, h) = (1920usize, 1080usize);
    let mut px = vec![0u8; w * h * 4];
    for (i, p) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % w, i / w);
        p[0] = (x * 255 / w) as u8;
        p[1] = (y * 255 / h) as u8;
        p[2] = if (x / 40 + y / 40) % 7 == 0 { 255 } else { 30 };
        p[3] = 255;
    }
    let lut = grade::look_teal_orange(0.5);
    for _ in 0..2 {
        let t = Instant::now();
        let mut f = optics::Linear::from_rgba8(&px, w, h);
        let a = t.elapsed();
        optics::bloom(&mut f, 0.9, 0.15, 0.2, 0.75, [1.0; 3]);
        let b = t.elapsed();
        optics::natural_vignette(&mut f, 0.18, 0.6);
        let c = t.elapsed();
        optics::film_grain(&mut f, 0.035, 1);
        let d = t.elapsed();
        let mut out = px.clone();
        f.write_rgba8(&mut out);
        let e = t.elapsed();
        lut.apply_rgba(&mut out, 0.6);
        let g = t.elapsed();
        println!(
            "from {:?} bloom {:?} vig {:?} grain {:?} write {:?} lut {:?}",
            a,
            b - a,
            c - b,
            d - c,
            e - d,
            g - e
        );
    }
}
