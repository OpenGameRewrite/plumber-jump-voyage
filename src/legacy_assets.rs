//! Compatibility adapter for packs exported before rendering metadata existed.
//! Interpret source names once while binding a scene, never in render systems.
use crate::asset_contract::{Bone, Part, Stain, Surface, VisibilityRole as V};

pub fn part(name: &str, metal: bool) -> Part {
    let (shape, material) = name.split_once("__").unwrap_or((name, ""));
    let (role, track) = if name.starts_with("EyeClose") || name.starts_with("EyeHalf") {
        (V::Hidden, None)
    } else if name.starts_with("EyeOpen") {
        (V::Visible, None)
    } else if name.starts_with("EyelidCloseLeft") {
        (V::ClosedLeft, None)
    } else if name.starts_with("EyelidCloseRight") {
        (V::ClosedRight, None)
    } else if name.starts_with("Eyeball__EyePupilLeft") {
        (V::PupilLeft, None)
    } else if name.starts_with("Eyeball__EyePupilRight") {
        (V::PupilRight, None)
    } else if name.starts_with("Eyelid") {
        (V::Track, Some(shape.to_string()))
    } else if name.starts_with("Eyeball__EyeMT") {
        (V::Hidden, None)
    } else if name.starts_with("Cap__") || name.starts_with("CapHair__") {
        (V::WornCap, Some(shape.to_string()))
    } else if name.starts_with("Hair__") {
        (V::UncappedHair, None)
    } else {
        (V::Unchanged, None)
    };
    let exported = name.contains("__");
    Part {
        version: 1,
        role,
        track,
        pupil: material
            .starts_with("EyePupil")
            .then(|| material.to_string()),
        surface: if name.contains("Glass") {
            Surface::Glass
        } else if name.ends_with("__HairMT") {
            Surface::Hair
        } else {
            Surface::Ordinary
        },
        stain: if !exported || name.contains("Eye") || name.contains("Glass") {
            Stain::None
        } else if role == V::WornCap {
            Stain::Cap
        } else {
            Stain::Body
        },
        exported,
        metal,
    }
}
pub fn bone(name: &str, parent: &str) -> Bone {
    let short = name.strip_prefix("Armature_").unwrap_or(name);
    Bone {
        version: 1,
        key: [
            name.to_string(),
            if matches!(parent, "Armature" | "Armature_nw4f_root") {
                String::new()
            } else {
                parent.to_string()
            },
        ],
        leg: !["Eye_", "Face_", "Head_"]
            .iter()
            .any(|p| short.starts_with(p))
            && [
                "Hip",
                "Leg",
                "Foot",
                "Toe",
                "JointRoot",
                "AllRoot",
                "nw4f_root",
            ]
            .iter()
            .any(|p| short.contains(p)),
        hip: name.ends_with("Hip"),
    }
}
