use super::*;

pub(super) fn run() {
    nerve_names();
    number_text();
    game_rows();
}

#[cfg_attr(test, test)]
fn nerve_names() {
    // Every state's name maps back to the same state.
    for mode in Move::ALL {
        let anim = if mode == Move::GroundPound {
            "HipDropStart"
        } else {
            "Wait"
        };
        let name = nerve(mode, anim).name;
        let speed = if mode == Move::Walk { 4. } else { 14. };
        assert_eq!(state_from_nerve(name, anim, speed), Some(mode), "{name}");
    }
    assert!(state_table().contains("Jump/Jump2nd ?") && state_table().contains("Wait\n"));
    assert_eq!(nerve(Move::GroundPound, "HipDrop").name, "HipDrop/Fall");
    assert_eq!(state_from_nerve("Run", "Run", 4.), Some(Move::Walk));
    // A nerve without its sub-state still finds the family.
    assert_eq!(state_from_nerve("Squat", "Wait", 0.), Some(Move::Crouch));
    assert_eq!(state_from_nerve("Unknown", "Wait", 0.), None);
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
         3,1.5,0,20,0,0,6,0,1,1,Run,Run,12,24,7,jump_timer=4|bogus=x,jump=3\n",
    )
    .unwrap();
    let s = file.snapshot(0);
    assert_eq!(s.mode, Some(Move::Walk), "six units per frame is a walk");
    assert!((s.pos.z - 0.2).abs() < 1e-6 && s.grounded);
    assert_eq!(
        s.anim,
        Some(("Move", 30.)),
        "a 24-frame clip at frame 12 is half a cycle"
    );
    assert_eq!(s.state_frames, Some(7));
    assert!(s.counters.contains(&("jump_timer".into(), 4)));
    assert!(s.counters.contains(&("pre_jump".into(), 3)));
    assert_eq!(file.cell(0, "interp"), None);
}
