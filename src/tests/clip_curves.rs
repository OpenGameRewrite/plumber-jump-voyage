use super::*;

pub(super) fn run() {
    a_cubic_segment_runs_from_its_key_to_the_next_over_unit_time();
    the_value_is_held_before_the_first_key_and_after_the_last();
    euler_angles_turn_about_x_first_then_y_then_z();
    a_compensated_joint_is_stored_over_its_parents_scale();
    a_damaged_file_is_refused();
}

fn channel() -> Channel {
    // 0 -> 10 over frames 2..6 with a slope, then 4 -> 4 over 6..8 (a flat key at the end).
    Channel::Keys(vec![
        Key {
            frame: 2.,
            coefficients: [0., 3., 6., 1.],
        },
        Key {
            frame: 6.,
            coefficients: [10., 0., -6., 0.],
        },
        Key {
            frame: 8.,
            coefficients: [4., 0., 0., 0.],
        },
    ])
}

#[cfg_attr(test, test)]
fn a_cubic_segment_runs_from_its_key_to_the_next_over_unit_time() {
    let channel = channel();
    // Whole key frames give the key's own value; the first segment ends where the next key starts.
    assert_eq!(channel.at(2.), 0.);
    assert_eq!(channel.at(6.), 10.);
    // Frame 4 is half way: 0 + 3/2 + 6/4 + 1/8.
    assert!((channel.at(4.) - 3.125).abs() < 1e-6);
    // Between keys the curve is the polynomial, not the line between the two samples.
    assert!((channel.at(3.) - (0.75 + 6. / 16. + 1. / 64.)).abs() < 1e-6);
    // The second segment: 10 - 6 t^2 over two frames.
    assert!((channel.at(7.) - 8.5).abs() < 1e-6);
}

#[cfg_attr(test, test)]
fn the_value_is_held_before_the_first_key_and_after_the_last() {
    let channel = channel();
    assert_eq!(channel.at(-1.), 0.);
    assert_eq!(channel.at(8.), 4.);
    assert_eq!(channel.at(100.), 4.);
    assert_eq!(Channel::Constant(2.5).at(7.), 2.5);
}

#[cfg_attr(test, test)]
fn euler_angles_turn_about_x_first_then_y_then_z() {
    use std::f64::consts::FRAC_PI_2;
    let turn = |q: [f64; 4], v: [f64; 3]| {
        let m = crate::skeleton_pose::rotation_matrix(q);
        std::array::from_fn::<f64, 3, _>(|r| (0..3).map(|c| m[r][c] * v[c]).sum())
    };
    // X first: the y axis turns to z, which a following turn about z leaves where it is.
    let q = euler_xyz_quaternion([FRAC_PI_2, 0., FRAC_PI_2]);
    let moved = turn(q, [0., 1., 0.]);
    assert!(moved[0].abs() < 1e-9 && moved[1].abs() < 1e-9 && (moved[2] - 1.).abs() < 1e-9);
    // Z first would have taken the y axis to -x and then left it there; here x ends on y.
    let moved = turn(q, [1., 0., 0.]);
    assert!(moved[0].abs() < 1e-9 && (moved[1] - 1.).abs() < 1e-9 && moved[2].abs() < 1e-9);
    let identity = euler_xyz_quaternion([0.; 3]);
    assert_eq!(identity, [0., 0., 0., 1.]);
}

/// A one-clip file of two joints (the second under the first) written the way the converter does.
/// Both joints are animated: the first has scale x running 1 -> 3 over frames 0..10, the second
/// keeps a scale x of 3; every other channel is a constant.
fn file(compensated: bool, quaternion: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&2u32.to_le_bytes());
    for (name, parent) in [("Top", -1i32), ("Under", 0)] {
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&parent.to_le_bytes());
        out.push(u8::from(compensated && parent >= 0));
        for value in [0f32, 0., 0., 0., 0., 0., 1., 1., 1., 1.] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(b"Run");
    out.extend_from_slice(&10f32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    for joint in 0..2u32 {
        out.extend_from_slice(&joint.to_le_bytes());
        out.push(u8::from(quaternion));
        for channel in 0..CHANNELS {
            if channel == 0 && joint == 0 {
                out.extend_from_slice(&2u32.to_le_bytes());
                for (frame, c) in [(0f32, [1f32, 2., 0., 0.]), (10., [3., 0., 0., 0.])] {
                    out.extend_from_slice(&frame.to_le_bytes());
                    for value in c {
                        out.extend_from_slice(&value.to_le_bytes());
                    }
                }
            } else {
                out.extend_from_slice(&0u32.to_le_bytes());
                let value = match channel {
                    0 => 3f32,
                    6 => 1.,
                    _ => 0.5,
                };
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    out
}

#[cfg_attr(test, test)]
fn a_compensated_joint_is_stored_over_its_parents_scale() {
    let (top_scale, under_scale) = (2., 3.);
    for compensated in [false, true] {
        let curves = Curves::parse(&file(compensated, true)).expect("a well formed file");
        let clip = curves.clip("Run").unwrap();
        let (top, under) = (curves.joint("Top").unwrap(), curves.joint("Under").unwrap());
        // Halfway through frames 0..10 the first joint's scale x is 1 + 2 * 0.5 = 2.
        let sample = curves.sample(clip, top, 5.).unwrap();
        assert!((sample.scale[0] - top_scale).abs() < 1e-6 && sample.scale[1] == 0.5);
        // A quaternion is normalised: (0.5, 0.5, 0.5, 1) over its length.
        let length = 1.75f64.sqrt();
        let expected = [0.5 / length, 0.5 / length, 0.5 / length, 1. / length];
        assert!((0..4).all(|i| (sample.rotation[i] - expected[i]).abs() < 1e-9));
        // Its own scale is the same in the file or not; the stored one is over the parent's.
        let own = curves.sample(clip, under, 5.).unwrap();
        assert_eq!(own.scale[0], under_scale);
        let stored = curves.sample_for_file(clip, under, 5.).unwrap();
        let expected = if compensated {
            under_scale / top_scale
        } else {
            under_scale
        };
        assert!((stored.scale[0] - expected).abs() < 1e-6, "{compensated}");
        assert_eq!(stored.translation, own.translation);
    }
    let curves = Curves::parse(&file(true, false)).unwrap();
    // Euler angles of 0.5 rad each, not a quaternion: unit length.
    let q = curves
        .sample(curves.clip("Run").unwrap(), 0, 0.)
        .unwrap()
        .rotation;
    assert!((q.iter().map(|c| c * c).sum::<f64>() - 1.).abs() < 1e-9);
    assert!(curves.clip("Missing").is_none());
    assert!(curves.joint("Missing").is_none());
}

#[cfg_attr(test, test)]
fn a_damaged_file_is_refused() {
    let good = file(false, false);
    assert!(Curves::parse(&good).is_some());
    assert!(Curves::parse(&good[..good.len() - 1]).is_none());
    assert!(Curves::parse(b"not a curve file").is_none());
    assert!(Curves::parse(&[]).is_none());
}
