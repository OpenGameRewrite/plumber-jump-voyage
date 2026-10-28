use super::*;

pub(super) fn run() {
    number_text();
    game_rows();
    sample_point();
    preroll_interpolation();
    geyser_collider_pose();
}

#[cfg_attr(test, test)]
fn geyser_collider_pose() {
    let trace = TraceFile::parse(
        "frame,px,py,pz,state,anim\n0,1,20,3,SandGeyser/SandGeyser,Fall\n1,1,40,3,SandGeyser/SandGeyser,Fall\n2,1,70,3,Fall/Fall,Fall\n",
    ).unwrap();
    assert_eq!(trace.snapshot(0).geyser_collider_from, None);
    assert_eq!(
        trace.snapshot(1).geyser_collider_from,
        Some(Vec3::new(1., 20., 3.))
    );
    assert_eq!(trace.snapshot(2).geyser_collider_from, None);
}

#[cfg_attr(test, test)]
fn preroll_interpolation() {
    let row = |carry| {
        let mut recorder = Recorder::new(None, None, None);
        let mut player = Player::default();
        let info = FrameInfo {
            frame: 0,
            game_frame: 0,
            stick: bevy::math::Vec2::ZERO,
            held: Vec::new(),
            pressed: Vec::new(),
            camera_yaw_deg: 0.,
            direction: bevy::math::Vec2::ZERO,
        };
        recorder.record(&info, &mut player);
        player.inject(&crate::movement::Snapshot {
            anim: Some(("SwimStandWait", 0.)),
            ..Default::default()
        });
        recorder.start_rows(carry);
        recorder.record(&info, &mut player)
    };
    let column = COLUMNS.iter().position(|name| *name == "interp").unwrap();
    assert!(row(false)[column].is_empty());
    let carried = row(true)[column].clone();
    assert!(carried.starts_with("Wait:"), "{carried:?}");
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
