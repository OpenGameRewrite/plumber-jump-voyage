//! The names the real game's probe logs for the player's state and for the
//! thrown cap, so a trace of the rewrite reads like a trace of the game.
//!
//! The probe writes `<actor nerve>/<sub-state nerve>` (`Wait/Wait`,
//! `Run/Run3D`, `Jump/Jump`, `Wait/Land`). The actor nerve is a nerve of the
//! original player actor; the sub-state is the nerve of the state object that
//! actor is running. The rewrite's states are finer or coarser than the
//! game's, so a state's name can depend on the clip it is playing.
//!
//! Where a name comes from:
//! * observed: the probe's runs log it (`Wait/Wait`, `Wait/Land`, `Run/Run3D`,
//!   `Jump/Jump` for the first, second and third jump, `Jump/JumpBack`,
//!   `Jump/JumpTurn`, `LongJump/Jump`, `HipDrop/*`, `HeadSliding/Dive`,
//!   `Squat/*`, `Rolling/*`, `SpinCap/SpinGround`). The braking and turning
//!   clips play inside `Run/Run3D`, so the rewrite's skid and turn use that
//!   name too.
//! * symbol: the nerve exists in the 1.0.0 symbol list of the public
//!   decompilation project and its role is plain from its name.
//! * guess (`?` in `--replay --states`): the rewrite's state is assigned to the
//!   nerve by this project's reading, or no distinct nerve is known for it.
use crate::movement::Move;

/// How a state is named in the game, and whether that name is a guess.
pub struct Nerve {
    pub name: &'static str,
    pub guess: bool,
}
const fn known(name: &'static str) -> Nerve {
    Nerve { name, guess: false }
}
const fn guessed(name: &'static str) -> Nerve {
    Nerve { name, guess: true }
}

/// The game's name for a state; `anim` is the clip the state is playing.
pub fn nerve(mode: Move, anim: &str) -> Nerve {
    match mode {
        Move::Idle => idle(anim),
        // Walking, running, braking and turning all run inside the same
        // sub-state; the clip tells them apart (`Brake`, `TurnPoint`).
        Move::Walk | Move::Run | Move::Skid | Move::Turn => known("Run/Run3D"),
        Move::SquatBrake => known("Squat/Brake"),
        Move::Crouch => known("Squat/Wait"),
        Move::Crawl => known("Squat/Walk"),
        Move::Fall => guessed("Fall/Fall"),
        Move::Jump => known("Jump/Jump"),
        // Consecutive and special jumps share the jump nerve; only the clip
        // and the jump counters differ in the game.
        Move::DoubleJump | Move::TripleJump => known("Jump/Jump"),
        Move::PoundJump => guessed("Jump/Jump"),
        Move::CapBounce | Move::CapVault | Move::CatchJump => guessed("Jump/Jump"),
        Move::LongJump => known("LongJump/Jump"),
        Move::Backflip => known("Jump/JumpBack"),
        Move::Sideflip => known("Jump/JumpTurn"),
        Move::SpinJump => guessed("Jump/JumpSpinGround"),
        Move::SpinDrop => guessed("Jump/JumpSpinGroundDownFall"),
        Move::GroundSpin => guessed("Run/GroundSpin3D"),
        Move::GroundPound if anim == "HipDropStart" => known("HipDrop/Start"),
        Move::GroundPound => known("HipDrop/Fall"),
        Move::PoundLand => known("HipDrop/Land"),
        Move::Roll if anim == "RollingStart" => known("Rolling/Start"),
        // The slow roll winds down in `UnRoll` (its clip is `RollingEnd`, kept
        // from the rolling clip's frame) and lands in `Land`.
        Move::Roll if anim == "RollingEnd" => guessed("Rolling/UnRoll"),
        Move::Roll if anim == "RollingLand" => guessed("Rolling/Land"),
        Move::Roll => known("Rolling/Rolling"),
        Move::RollEnd => known("Rolling/StandUp"),
        Move::RollJump => known("Rolling/Jump"),
        Move::Dive => known("HeadSliding/Dive"),
        Move::Slide => guessed("HeadSliding/Dive"),
        Move::SlopeSlide => known("Slope/Slide"),
        Move::BurnJump => known("DamageFire/Jump"),
        Move::BurnBounce => guessed("DamageFire/Jump2nd"),
        Move::FireRun => known("DamageFire/Run"),
        Move::WallSlide => known("WallAir/Slide"),
        Move::WallJump => known("WallAir/Jump"),
        Move::Ledge => ledge(anim),
        Move::Pole => pole(anim),
        Move::Swim if anim == "SwimFallIn" => known("Swim/SwimFallIn"),
        Move::Swim => guessed("Swim/SwimMove"),
        Move::SwimDash => guessed("Swim/SwimDive"),
        Move::Throw if anim.contains("Air") => known("SpinCap/SpinAir"),
        Move::Throw => known("SpinCap/SpinGround"),
    }
}
/// The actor nerve a state belongs to (`Wait`, `Run`, `Squat`): the part of
/// its name before the sub-state. The step counter restarts when it changes.
pub fn actor(mode: Move, anim: &str) -> &'static str {
    let name = nerve(mode, anim).name;
    name.split('/').next().unwrap_or(name)
}
/// Standing: the landing and stand-up clips are sub-states of their own.
fn idle(anim: &str) -> Nerve {
    match anim {
        // The landing clip and its variants after a triple jump and a flip.
        "Land" | "LandJump3" | "LandTurn" | "LandRolling" => known("Wait/Land"),
        "LandDownFall" => guessed("Wait/Land"),
        "LandStiffen" => known("Wait/LandStiffen"),
        "SquatEnd" => known("Squat/StandUp"),
        // The game's idle variants; the rewrite has no such clips yet.
        "WaitRelaxStart" => known("Wait/WaitRelaxStart"),
        "WaitRelax" => known("Wait/WaitRelax"),
        _ => known("Wait/Wait"),
    }
}
fn ledge(anim: &str) -> Nerve {
    match anim {
        "WallCatchStart" => known("WallCatch/Start"),
        "WallCatchEnd" | "WallCatchEndJump" => guessed("WallCatch/Climb"),
        "WallCatchMoveL" => known("WallCatch/MoveLeft"),
        "WallCatchMoveR" => known("WallCatch/MoveRight"),
        _ => known("WallCatch/Wait"),
    }
}
fn pole(anim: &str) -> Nerve {
    match anim {
        "PoleCatch" => known("PoleClimb/Start"),
        "PoleClimb" => known("PoleClimb/Up"),
        "PoleFall" => guessed("PoleClimb/Down"),
        _ => known("PoleClimb/Wait"),
    }
}

/// Every state with the name the trace gives it; names that are guesses are
/// marked with a question mark.
pub fn state_table() -> String {
    Move::ALL
        .into_iter()
        .map(|mode| {
            let n = nerve(mode, "HipDropStart");
            format!(
                "{:<16} {}{}\n",
                format!("{mode:?}"),
                n.name,
                if n.guess { " ?" } else { "" }
            )
        })
        .collect()
}

/// The state that plays a clip the game names by its clip, for telling apart
/// states that share a nerve name.
fn state_of_clip(anim: &str) -> Option<Move> {
    Some(match anim {
        "Brake" => Move::Skid,
        // The game plays `Turn` after a reverse brake and `TurnPoint` for a pivot.
        "TurnPoint" | "Turn" => Move::Turn,
        "Jump2" => Move::DoubleJump,
        "Jump3" => Move::TripleJump,
        "JumpHipDrop" => Move::PoundJump,
        "JumpCapLeapFrog" => Move::CapVault,
        "JumpCapCatchCommon" => Move::CatchJump,
        "SpinJumpStart" | "SpinJumpLoop" => Move::SpinJump,
        "RollingStandUp" => Move::RollEnd,
        "LandRolling" => Move::Slide,
        "FireRunStart" => Move::BurnJump,
        _ => return None,
    })
}
/// The state a recorded nerve name stands for (the reverse of `nerve`).
/// Several states can share a name: the clip picks among them, then the speed
/// separates walking from running. A name without a known sub-state matches
/// the first state of that nerve.
pub fn state_from_nerve(name: &str, anim: &str, speed_h: f32) -> Option<Move> {
    let exact: Vec<Move> = Move::ALL
        .into_iter()
        .filter(|m| nerve(*m, anim).name == name)
        .collect();
    let candidates = if exact.is_empty() {
        let family = name.split('/').next().unwrap_or(name);
        Move::ALL
            .into_iter()
            .filter(|m| nerve(*m, anim).name.split('/').next() == Some(family))
            .collect()
    } else {
        exact
    };
    if let Some(by_clip) = state_of_clip(anim)
        && candidates.contains(&by_clip)
    {
        return Some(by_clip);
    }
    if candidates.contains(&Move::Walk) && candidates.contains(&Move::Run) {
        return Some(if speed_h < 8. { Move::Walk } else { Move::Run });
    }
    candidates.first().copied()
}

#[path = "tests/game_names.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
