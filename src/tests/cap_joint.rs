use super::*;

pub(super) fn run() {
    a_clip_is_read_from_a_binary_gltf();
    the_hovering_clips_of_the_installed_cap_do_not_turn_the_joint();
}

/// A binary glTF with one joint, `Armature_AllRoot`, and one clip, `Turn`, that turns it a
/// quarter of a turn about the vertical axis over six frames while its scale stays (1.44, 0.9,
/// 1.44), plus a clip for another joint that must be ignored.
fn sample_file() -> Vec<u8> {
    let mut binary = Vec::new();
    let mut push = |values: &[f32]| {
        let at = binary.len();
        for v in values {
            binary.extend_from_slice(&v.to_le_bytes());
        }
        (at, values.len() * 4)
    };
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let times = push(&[0., 0.1]);
    let rotations = push(&[0., 0., 0., 1., 0., half, 0., half]);
    let scales = push(&[1.44, 0.9, 1.44, 1.44, 0.9, 1.44]);
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "nodes": [{"name": "Armature_Other"}, {"name": "Armature_AllRoot"}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": times.0, "byteLength": times.1},
            {"buffer": 0, "byteOffset": rotations.0, "byteLength": rotations.1},
            {"buffer": 0, "byteOffset": scales.0, "byteLength": scales.1},
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC4"},
            {"bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3"},
        ],
        "animations": [
            {
                "name": "Turn",
                "samplers": [
                    {"input": 0, "output": 1, "interpolation": "LINEAR"},
                    {"input": 0, "output": 2},
                ],
                "channels": [
                    {"sampler": 0, "target": {"node": 1, "path": "rotation"}},
                    {"sampler": 1, "target": {"node": 1, "path": "scale"}},
                    {"sampler": 0, "target": {"node": 0, "path": "rotation"}},
                ],
            },
        ],
    })
    .to_string();
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + binary.len();
    let mut file = Vec::new();
    file.extend_from_slice(b"glTF");
    file.extend_from_slice(&2u32.to_le_bytes());
    file.extend_from_slice(&(total as u32).to_le_bytes());
    file.extend_from_slice(&(json.len() as u32).to_le_bytes());
    file.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    file.extend_from_slice(&json);
    file.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    file.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    file.extend_from_slice(&binary);
    file
}

/// The joint's front over a clip: the local +Z axis with its scale at the first frame, turned
/// halfway (45 degrees) at frame 3 and a quarter turn at frame 6 (and held there after).
#[cfg_attr(test, test)]
fn a_clip_is_read_from_a_binary_gltf() {
    let clips = read_clips(&sample_file()).expect("a readable file");
    let turn = clips.get("Turn").expect("the clip");
    let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-4;
    assert!(close(turn.front_at(0.), Vec3::new(0., 0., 1.44)));
    let half = std::f32::consts::FRAC_1_SQRT_2 * 1.44;
    assert!(close(turn.front_at(3.), Vec3::new(half, 0., half)));
    assert!(close(turn.front_at(6.), Vec3::new(1.44, 0., 0.)));
    assert!(close(turn.front_at(60.), Vec3::new(1.44, 0., 0.)));
    assert!(read_clips(b"not a glTF file").is_none());
}

/// With the converted cap installed: the clips the cap plays while it hovers keep the joint's
/// rotation at the bind pose on every frame, so its front is the actor's (what the trample's
/// `calcJointFrontDir` relies on). Skipped without the local assets.
#[cfg_attr(test, test)]
fn the_hovering_clips_of_the_installed_cap_do_not_turn_the_joint() {
    let path = crate::local_assets::asset_dir().join(CAP_MODEL);
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let clips = read_clips(&bytes).expect("the installed cap is readable");
    for name in ["StayR", "StayL"] {
        let clip = clips.get(name).unwrap_or_else(|| panic!("clip {name}"));
        for frame in 0..9 {
            let front = clip.front_at(frame as f32);
            assert!(
                front.normalize().abs_diff_eq(Vec3::Z, 1e-6),
                "{name} {frame}: {front:?}"
            );
        }
    }
}
