// SPDX-License-Identifier: GPL-3.0-or-later
//! Draws a bundle's face as the editor does (the body, then each control's frame), with no window:
//!
//!   cargo run -p gfx-ui --example snapshot -- gfx-grit out.png [px per unit, 3 by default] [id=value …]
//!
//! Controls stand at their defaults unless given (`drive=80`, `quality=1`). To compare with node's drawing.

use gfx_ui::bundle::Bundle;
use gfx_ui::raster;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(krate), Some(out)) = (args.first(), args.get(1)) else {
        return Err("usage: snapshot <crate> <out.png> [px per unit] [id=value …]".into());
    };
    let scale: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bundles").join(format!("{krate}.gfxui"));
    let bundle = Bundle::parse(&std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?)?;
    let vb = bundle.face.view_box;
    let (w, h) = raster::face_pixels(vb, scale);
    let mut img = raster::render(&bundle.face.body, [0, 0, w, h], scale)?;

    for control in &bundle.face.controls {
        let info = bundle.params.get(&control.param).ok_or("a control without a parameter")?;
        let given = args.iter().skip(2).find_map(|a| a.strip_prefix(&format!("{}=", control.param)).map(str::to_string));
        let plain = match given {
            Some(v) => v.parse::<f32>().map_err(|e| format!("{}: {e}", control.param))?,
            None => info.plain(&info.default).ok_or("a default that is not a value")?,
        };
        let t = if info.max > info.min { (plain - info.min) / (info.max - info.min) } else { 0.0 };
        let frame = control.frame(info.kind, t, plain);
        let at = raster::pixel_box(vb, control.bbox, scale);
        let layer = raster::render(&control.frames[frame], at, scale)?;
        // Source over, both premultiplied; the layer is clipped to the face.
        for y in 0..layer.height {
            for x in 0..layer.width {
                let (fx, fy) = (at[0] + x as i32, at[1] + y as i32);
                if fx < 0 || fy < 0 || fx >= w || fy >= h {
                    continue;
                }
                let (s, d) = ((y * layer.width + x) * 4, (fy as usize * img.width + fx as usize) * 4);
                let a = layer.rgba[s + 3] as u32;
                for c in 0..4 {
                    img.rgba[d + c] = (layer.rgba[s + c] as u32 + img.rgba[d + c] as u32 * (255 - a) / 255).min(255) as u8;
                }
            }
        }
    }

    // PNG wants straight alpha.
    for px in img.rgba.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| e.to_string())?), img.width as u32, img.height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut w| w.write_image_data(&img.rgba)).map_err(|e| e.to_string())?;
    println!("{krate}: {}x{} -> {out}", img.width, img.height);
    Ok(())
}
