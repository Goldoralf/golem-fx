// SPDX-License-Identifier: GPL-3.0-or-later
//! Every committed bundle (`bundles/*.gfxui`) is one the editor can show.

use gfx_ui::bundle::{Bundle, ControlKind, Header, ParamKind};
use std::path::PathBuf;

fn bundles() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bundles");
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "gfxui"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no bundle");
    out
}

#[test]
fn bundles_parse_and_name_their_crate() {
    for (name, bytes) in bundles() {
        let head = Header::read(&bytes).unwrap_or_else(|| panic!("{name}: header"));
        let b = Bundle::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(b.krate, name);
        assert_eq!(head.width, b.face.view_box[2].ceil() as u16, "{name}: header width");
        assert_eq!(head.height, b.face.view_box[3].ceil() as u16, "{name}: header height");
        let off_face = b.params.order.iter().any(|id| !b.on_face(id));
        assert_eq!(head.panel, off_face, "{name}: header panel flag");
    }
}

#[test]
fn controls_have_a_frame_for_every_position() {
    for (name, bytes) in bundles() {
        let b = Bundle::parse(&bytes).unwrap();
        for c in &b.face.controls {
            let p = b.params.get(&c.param).unwrap_or_else(|| panic!("{name}: control {} has no parameter", c.param));
            let want = match p.kind {
                ParamKind::Bool => 2,
                ParamKind::Enum => p.options.as_ref().map_or(0, |o| o.len()),
                ParamKind::Float => match c.kind {
                    ControlKind::Fader => 49,
                    _ => 65,
                },
            };
            assert_eq!(c.frames.len(), want, "{name}: frames of {}", c.param);
            assert!(c.bbox[2] > 0.0 && c.bbox[3] > 0.0, "{name}: box of {}", c.param);
            assert!(c.frames.iter().all(|f| f.starts_with("<svg") && !f.contains("<text")), "{name}: {} keeps text", c.param);
        }
        assert!(!b.face.body.contains("<text"), "{name}: the body keeps text");
    }
}

#[test]
fn groups_and_front_name_parameters() {
    for (name, bytes) in bundles() {
        let b = Bundle::parse(&bytes).unwrap();
        for id in b.front.iter().chain(b.groups.iter().flat_map(|g| g.params.iter())) {
            assert!(b.params.get(id).is_some(), "{name}: {id} is not a parameter");
        }
        for (id, p) in &b.params.by_id {
            if let Some(d) = &p.depends {
                assert!(b.params.get(&d.param).is_some(), "{name}: {id} depends on {}, which is not a parameter", d.param);
            }
            let def = p.plain(&p.default).unwrap_or_else(|| panic!("{name}: default of {id}"));
            assert!((p.min..=p.max).contains(&def), "{name}: default of {id} outside its range");
        }
    }
}

#[test]
fn presets_set_parameters_inside_their_range() {
    for (name, bytes) in bundles() {
        let b = Bundle::parse(&bytes).unwrap();
        for preset in &b.presets {
            for (id, value) in &preset.params {
                let p = b.params.get(id).unwrap_or_else(|| panic!("{name}: preset {} sets {id}, not a parameter", preset.id));
                let v = p.plain(value).unwrap_or_else(|| panic!("{name}: preset {} gives {id} = {value}", preset.id));
                assert!((p.min..=p.max).contains(&v), "{name}: preset {} sets {id} = {v}, outside {}..{}", preset.id, p.min, p.max);
            }
        }
    }
}
