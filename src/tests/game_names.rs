use super::*;
use crate::movement::{CapPhase, Throw};

pub(super) fn run() {
    names_round_trip();
    observed_names();
    shared_names();
    cap_names();
}

#[cfg_attr(test, test)]
fn names_round_trip() {
    // A state's name maps back to a state of the same name whatever else shares it.
    for mode in Move::ALL {
        let anim = if mode == Move::GroundPound {
            "HipDropStart"
        } else {
            "Wait"
        };
        let name = nerve(mode, anim).name;
        let speed = if mode == Move::Walk { 4. } else { 14. };
        let back = state_from_nerve(name, anim, speed).unwrap();
        assert_eq!(nerve(back, anim).name, name, "{mode:?} {name}");
    }
    assert!(state_table().contains("Wait/Wait\n"));
    assert!(state_table().contains("DamageFire/Jump2nd ?"));
    // A name without a known sub-state still finds the family.
    assert_eq!(
        state_from_nerve("Squat", "Wait", 0.),
        Some(Move::SquatBrake)
    );
    assert_eq!(state_from_nerve("Unknown/Nerve", "Wait", 0.), None);
}

/// The names the real game's probe logged in its runs.
#[cfg_attr(test, test)]
fn observed_names() {
    assert_eq!(nerve(Move::Idle, "Wait").name, "Wait/Wait");
    assert_eq!(nerve(Move::Walk, "Move").name, "Run/Run3D");
    assert_eq!(nerve(Move::Run, "Move").name, "Run/Run3D");
    assert_eq!(nerve(Move::Skid, "Brake").name, "Run/Run3D");
    assert_eq!(nerve(Move::Turn, "TurnPoint").name, "Run/Run3D");
    assert_eq!(nerve(Move::Jump, "JumpReverse").name, "Jump/Jump");
    assert_eq!(
        nerve(Move::Throw, "SpinCapStart").name,
        "SpinCap/SpinGround"
    );
    // Landing is the Wait nerve's own sub-state while the landing clip plays.
    assert_eq!(nerve(Move::Idle, "Land").name, "Wait/Land");
    assert!(!nerve(Move::Idle, "Land").guess);
    assert!(!nerve(Move::Idle, "LandJump3").guess && nerve(Move::Idle, "LandDownFall").guess);
    // Second and third jumps, flips and rolls were logged by the real game too.
    assert!(!nerve(Move::DoubleJump, "Jump2").guess && !nerve(Move::TripleJump, "Jump3").guess);
    assert_eq!(nerve(Move::Backflip, "JumpBack").name, "Jump/JumpBack");
    assert_eq!(nerve(Move::Sideflip, "JumpTurn").name, "Jump/JumpTurn");
    assert_eq!(nerve(Move::RollEnd, "RollingEnd").name, "Rolling/StandUp");
    assert_eq!(nerve(Move::Idle, "SquatEnd").name, "Squat/StandUp");
    assert_eq!(nerve(Move::Idle, "WaitRelax").name, "Wait/WaitRelax");
    assert_eq!(nerve(Move::Idle, "LandStiffen").name, "Wait/LandStiffen");
    assert_eq!(nerve(Move::GroundPound, "HipDrop").name, "HipDrop/Fall");
    assert_eq!(
        nerve(Move::GroundPound, "HipDropStart").name,
        "HipDrop/Start"
    );
}

#[cfg_attr(test, test)]
fn shared_names() {
    // The clip picks among the states that share a sub-state.
    assert_eq!(
        state_from_nerve("Run/Run3D", "Brake", 10.),
        Some(Move::Skid)
    );
    assert_eq!(
        state_from_nerve("Run/Run3D", "TurnPoint", 2.),
        Some(Move::Turn)
    );
    assert_eq!(state_from_nerve("Run/Run3D", "Turn", 2.), Some(Move::Turn));
    assert_eq!(state_from_nerve("Run/Run3D", "Move", 4.), Some(Move::Walk));
    assert_eq!(state_from_nerve("Run/Run3D", "Move", 14.), Some(Move::Run));
    assert_eq!(
        state_from_nerve("Jump/Jump", "Jump2", 8.),
        Some(Move::DoubleJump)
    );
    assert_eq!(
        state_from_nerve("Jump/Jump", "JumpReverse", 0.),
        Some(Move::Jump)
    );
    // The game logs the landing state before its clip starts.
    assert_eq!(
        state_from_nerve("Wait/Land", "JumpReverse", 0.),
        Some(Move::Idle)
    );
}

#[cfg_attr(test, test)]
fn cap_names() {
    assert_eq!(cap_state(CapPhase::Out, Throw::Forward), "Throw");
    assert_eq!(cap_state(CapPhase::Out, Throw::Spin), "ThrowSpiral");
    assert_eq!(cap_state(CapPhase::Stay, Throw::Forward), "ThrowStay");
    assert_eq!(cap_state(CapPhase::Return, Throw::Down), "Return");
}
