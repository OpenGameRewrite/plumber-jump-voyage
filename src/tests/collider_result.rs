use super::*;
use serde_json::Value;

pub(super) fn run() {
    a_lone_floor_arrow_pushes_along_the_ground_normal();
    the_exes_result_computation_agrees_bit_for_bit();
    the_exes_arrow_ring_agrees_bit_for_bit();
}

fn arrow_result(depth: f32, margin: f32) -> ContactFacts {
    ContactFacts {
        motion: None,
        shape: ShapeFacts::Arrow {
            index: 0,
            margin,
            direction: Vec3::new(0., -60., 0.),
        },
        hit: Hit::Arrow(ArrowHit {
            depth,
            position: Vec3::ZERO,
            distance: 60. - depth,
        }),
        normal: Vec3::Y,
        triangle: 0,
    }
}

/// An arrow 5 cm past its margin lifts the body by 5 along the floor's normal.
#[cfg_attr(test, test)]
fn a_lone_floor_arrow_pushes_along_the_ground_normal() {
    let mut collider = PlayerCollider::new(Form::Normal);
    let solved = collider.solve_results(
        &[arrow_result(25., 20.)],
        &[],
        Vec3::ZERO,
        BorderCheck::NoFace,
        &mut |_, _| false,
    );
    assert_eq!(solved.fix, Vec3::new(0., 5., 0.));
    assert_eq!(solved.flags, 0x40);
    assert!(collider.floor.is_some() && collider.has_ground_normal);
}

fn v3(value: &Value) -> Vec3 {
    let a = value.as_array().expect("a vector");
    Vec3::new(
        a[0].as_f64().unwrap() as f32,
        a[1].as_f64().unwrap() as f32,
        a[2].as_f64().unwrap() as f32,
    )
}
fn num(value: &Value) -> f32 {
    value.as_f64().expect("a number") as f32
}

fn facts_of(case: &Value) -> ContactFacts {
    let normal = v3(&case["n"]);
    if case["k"] == "a" {
        ContactFacts {
            motion: None,
            shape: ShapeFacts::Arrow {
                index: case["index"].as_u64().unwrap() as usize,
                margin: num(&case["margin"]),
                direction: v3(&case["dir"]),
            },
            hit: Hit::Arrow(ArrowHit {
                depth: num(&case["depth"]),
                position: v3(&case["pos"]),
                distance: 0.,
            }),
            normal,
            triangle: 0,
        }
    } else {
        let loc = case["loc"].as_u64().unwrap() as u8;
        ContactFacts {
            motion: None,
            shape: ShapeFacts::Sphere {
                support: case["support"].as_f64().map(|r| r as f32),
                support_axis: case.get("axis").map_or(Vec3::Y, v3),
                ignore_ground: case["ign"].as_bool().unwrap(),
                ignore_ceiling: false,
            },
            hit: Hit::Sphere(SphereHit {
                motion: None,
                depth: num(&case["depth"]),
                location: match loc {
                    1 => Location::Face,
                    2..=4 => Location::Edge(loc - 1),
                    _ => Location::Corner(loc - 4),
                },
                position: v3(&case["pos"]),
                center: v3(&case["center"]),
            }),
            normal,
            triangle: 0,
        }
    }
}

fn same(a: f32, b: f32) -> bool {
    a == b
}
fn same3(a: Vec3, b: Vec3) -> bool {
    same(a.x, b.x) && same(a.y, b.y) && same(a.z, b.z)
}

/// Cases from the executable's own `calcResultVec`, run in a local interpreter on a faked
/// collider (the fidelity folder's `collider_oracle.py`, fed with the calls of the wall-slide,
/// swim and ledge runs and random variations of them): `PJV_COLLIDER_CASES` names the file;
/// without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_result_computation_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_COLLIDER_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: Value = serde_json::from_str(&text).expect("json");
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        let expect = &case["expect"];
        let border = match case["border"].as_u64().unwrap() {
            0 => BorderCheck::Off,
            1 => BorderCheck::NoFace,
            _ => BorderCheck::All,
        };
        let mut collider = PlayerCollider::new(Form::Normal);
        collider.keeper.has_arrow = border != BorderCheck::Off;
        collider.push_directions = case["old"].as_u64().unwrap() as u32;
        let best = case["best"].as_array().unwrap();
        for (slot, depth) in best.iter().enumerate() {
            collider.best[slot] = num(depth);
        }
        if let Some(floor) = case["floor"].as_array() {
            collider.floor = Some(Contact {
                normal: v3(&floor[1]),
                position: v3(&floor[0]),
                triangle: 0,
                location: Location::Face,
            });
        }
        for (slot, held) in case["slots"].as_array().unwrap().iter().enumerate() {
            collider.arrow_slots[slot] = held
                .as_array()
                .map(|h| (num(&h[0]), num(&h[1]), v3(&h[2]), v3(&h[3])));
        }
        let facts = |key: &str| -> Vec<ContactFacts> {
            case[key].as_array().unwrap().iter().map(facts_of).collect()
        };
        let (collide, support) = (facts("results"), facts("support"));
        let answers: Vec<bool> = case["border_seq"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_bool().unwrap())
            .collect();
        let mut asked = Vec::new();
        let solved = collider.solve_results(
            &collide,
            &support,
            v3(&case["prev"]),
            border,
            &mut |at, normal| {
                asked.push((at, normal));
                answers.get(asked.len() - 1).copied().unwrap_or(false)
            },
        );
        let what = format!("case {checked}: {case}");
        assert!(
            same3(solved.fix, v3(&expect["fix"])),
            "fix {:?} {what}",
            solved.fix
        );
        assert!(
            same3(solved.total, v3(&expect["total"])),
            "total {:?} {what}",
            solved.total
        );
        assert_eq!(
            solved.flags | collider.push_directions,
            expect["flags"].as_u64().unwrap() as u32,
            "flags {what}"
        );
        let expected_borders = expect["borders"].as_array().unwrap();
        assert_eq!(asked.len(), expected_borders.len(), "border calls {what}");
        for (got, want) in asked.iter().zip(expected_borders) {
            assert!(
                same3(got.0, v3(&want[0])) && same3(got.1, v3(&want[1])),
                "border {what}"
            );
        }
        for (slot, contact) in [&collider.floor, &collider.wall, &collider.ceiling]
            .into_iter()
            .enumerate()
        {
            let depth = num(&expect["depths"][slot]);
            assert!(same(collider.best[slot], depth), "depth {slot} {what}");
            if depth != num(&best[slot]) {
                let contact = contact.as_ref().expect("a recorded contact");
                let want = &expect["contacts"][slot];
                assert!(
                    same3(contact.position, v3(&want[0])),
                    "contact {slot} position {what}"
                );
                assert!(
                    same3(contact.normal, v3(&want[1])),
                    "contact {slot} normal {what}"
                );
            }
        }
        assert!(
            same3(collider.ground_position, v3(&expect["ground"][0]))
                && same3(collider.ground_normal, v3(&expect["ground"][1])),
            "ground {what}"
        );
        for (slot, held) in collider.arrow_slots.iter().enumerate() {
            let want = &expect["slots"][slot];
            let depth = num(&want[0]);
            match held {
                Some((d, margin, at, n)) => assert!(
                    same(*d, depth)
                        && same(*margin, num(&want[1]))
                        && same3(*at, v3(&want[2]))
                        && same3(*n, v3(&want[3])),
                    "slot {slot} {what}"
                ),
                None => assert!(depth < 0., "slot {slot} {what}"),
            }
        }
        checked += 1;
    }
    eprintln!("collider result cases checked against the executable: {checked}");
}

/// The arrows `PlayerColliderHakoniwa::init` creates, from the executable's own helper run in a
/// local interpreter (`arrow_keeper_probe.py`): `PJV_RING_CASES` names the file.
#[cfg_attr(test, test)]
fn the_exes_arrow_ring_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_RING_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: Value = serde_json::from_str(&text).expect("json");
    for case in cases.as_array().expect("cases") {
        let index = case["index"].as_u64().unwrap() as usize;
        let ours = ring_arrow_start(index, v3(&case["offset"]));
        assert!(
            same3(ours, v3(&case["start"])),
            "arrow {index}: {ours:?} for {case}"
        );
    }
    eprintln!(
        "arrow ring cases checked against the executable: {}",
        cases.as_array().unwrap().len()
    );
}
