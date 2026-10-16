use super::*;

pub(super) fn run() {
    number_text();
    game_rows();
    sample_point();
}

#[cfg_attr(test, test)]
fn number_text() {
    assert_eq!(num(-0.0000001), "0.000000");
    assert_eq!(num(14.), "14.000000");
    assert_eq!(num(-1.23456), "-1.234560");
}

#[cfg_attr(test, test)]
fn game_rows() {
    let file = TraceFile::parse(
        "frame,px,py,pz,vx,vy,vz,fx,fz,ground,state,anim,anim_frame,anim_len,state_step,counters,pre_input\n\
         3,1.5,0,20,0,0,6,0,1,1,Run/Run3D,Run,12,24,7,jump_timer=4|bogus=x|prejump=2|cjump=1/5,jump=3\n\
         4,1.5,0,20,0,0,6,0,1,1,Run/Run3D,Move,12,60,7,,\n",
    )
    .unwrap();
    let s = file.snapshot(0);
    assert_eq!(s.mode, Some(Move::Walk), "six units per frame is a walk");
    assert!((s.pos.z - 0.2).abs() < 1e-6 && s.grounded);
    assert_eq!(
        s.anim,
        Some(("Move", 30.0)),
        "frame 12 of 24 is half a cycle; the next step advances it"
    );
    assert_eq!(s.state_frames, Some(7));
    assert!(s.counters.contains(&("jump_timer".into(), 4)));
    assert!(s.counters.contains(&("pre_jump".into(), 3)));
    // The game's counters come back under the rewrite's names.
    assert!(s.counters.contains(&("pre_jump".into(), 2)));
    assert!(s.counters.contains(&("jump_count".into(), 1)));
    assert_eq!(file.cell(0, "interp"), None);
    // The game's own `Move` slot is the cycle phase itself.
    assert_eq!(file.snapshot(1).anim, Some(("Move", 12.0)));
}

/// The recorded animation frame follows the game's sample: a clip started in
/// a step reads frame 0 there, and the frame after reads 1. Triggers fire one
/// advance ahead of what the row shows.
#[cfg_attr(test, test)]
fn sample_point() {
    let clip = Anim {
        name: "Jump",
        serial: 0,
        frame: 0.,
        rate: 1.5,
    };
    assert_eq!(main_frame(&clip), 0.);
    assert_eq!(clip.held(), 1.5);
    let sub = Anim {
        name: "ThrowCapParts",
        serial: 0,
        frame: 1.,
        rate: 1.,
    };
    assert_eq!(sub_frame(&sub), 1.);
}
