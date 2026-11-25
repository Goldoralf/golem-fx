// SPDX-License-Identifier: GPL-3.0-or-later
//! SVG to pixels (resvg), with no window: the editor and the snapshot example share it.

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;

/// Premultiplied RGBA.
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// A rectangle of physical pixels inside the face, from a box in viewBox units: floored and ceiled so a
/// layer drawn there lands on the body's own pixel grid.
pub fn pixel_box(view_box: [f32; 4], bbox: [f32; 4], px_per_unit: f32) -> [i32; 4] {
    let x0 = ((bbox[0] - view_box[0]) * px_per_unit).floor() as i32;
    let y0 = ((bbox[1] - view_box[1]) * px_per_unit).floor() as i32;
    let x1 = ((bbox[0] + bbox[2] - view_box[0]) * px_per_unit).ceil() as i32;
    let y1 = ((bbox[1] + bbox[3] - view_box[1]) * px_per_unit).ceil() as i32;
    [x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)]
}

/// The face's size in physical pixels.
pub fn face_pixels(view_box: [f32; 4], px_per_unit: f32) -> (i32, i32) {
    ((view_box[2] * px_per_unit).round() as i32, (view_box[3] * px_per_unit).round() as i32)
}

/// Draws the part `[x, y, w, h]` (physical pixels of the face) of an SVG document whose root covers the
/// face's viewBox, at `px_per_unit`.
pub fn render(svg: &str, region: [i32; 4], px_per_unit: f32) -> Result<Image, String> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let (w, h) = (region[2].max(1) as u32, region[3].max(1) as u32);
    let mut pixmap = Pixmap::new(w, h).ok_or("empty image")?;
    let t = Transform::from_scale(px_per_unit, px_per_unit).post_translate(-region[0] as f32, -region[1] as f32);
    resvg::render(&tree, t, &mut pixmap.as_mut());
    Ok(Image { width: w as usize, height: h as usize, rgba: pixmap.take() })
}
