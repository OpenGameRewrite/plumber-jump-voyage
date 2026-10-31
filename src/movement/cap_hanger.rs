//! CapHanger, the hook a thrown cap locks onto (SMO 1.0.0 `CapHanger.cpp`).
//!
//! The hanger answers the cap's lock-on protocol: `CapStartLockOn` is accepted in Wait for a
//! non-collision sensor and enters Keep; every `CapKeepLockOn` is answered while it keeps. `exeKeep`
//! generates its configured coins and decides when to let go (`0x23597c`):
//! * the coin branch calls `rs::tryAppearMultiCoinFromObj` with the nerve step; the helper appears
//!   one only on a multiple of ten (`0x556444`), and the hanger counts each success against its
//!   `AppearItemNum`;
//! * once the quota is generated the hanger releases when the keep input is no longer held and the
//!   step is past 20 (`0x235b78`), or when it still is held past step 1200 or when the player is
//!   farther than 1500 horizontally (`0x187fd80`);
//! * Release lasts until its step is past 10 (`0x235d8c`) and returns to Wait.
//!
//! The release before the quota is generated (a keep input let go early) takes the shine-failure
//! reactions in the original and is not ported: the diagnostic reports it.
use super::*;

/// `CapPointHookStart` runs 11 frames (`MarioCapAnimation`); the hat then plays `CapPointHookWait`.
pub const START_ACTION_FRAMES: u32 = 11;
/// `exeRelease` waits for `isGreaterStep(10)`.
const RELEASE_STEPS: i32 = 10;
/// Past this step with the quota generated and the input let go the hanger releases.
const KEEP_MIN_STEPS: i32 = 20;
/// Past this step, or this far from the player, a held keep input gives up (`0x187fd80`).
const KEEP_MAX_STEPS: i32 = 1200;
const KEEP_MAX_DISTANCE: f32 = 1500.;
/// `rs::tryAppearMultiCoinFromObj` appears a coin on every tenth step.
const COIN_CADENCE: i32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HangerNerve {
    Wait,
    Keep,
    Release,
}

#[derive(Clone, Debug)]
pub struct CapHanger {
    pub id: String,
    /// The placement's translation (the actor pose the cap's target matrix follows).
    pub position: Vec3,
    /// The same position relative to the replay's start (set by the stage loader).
    pub relative: Vec3,
    /// `InitSensor.byml` Body: radius and offset above the actor.
    pub body_radius: f32,
    pub body_offset: f32,
    quota: u32,
    generated: u32,
    nerve: HangerNerve,
    step: i32,
    /// The exported INITIAL pending Wait has not executed at the first sample.
    initial: bool,
    /// The keep input was let go before the quota was generated (the original's shine-failure
    /// branch, not ported).
    early_release: bool,
}

impl CapHanger {
    pub fn load(source: &serde_json::Value) -> Result<Self, String> {
        let p = &source["placement"];
        let sensors = source["sensors"].as_array().ok_or("hanger sensors")?;
        if p["UnitConfigName"] != "CapHanger"
            || p["IsLinkDest"] != false
            || p["IsConnectToCollision"] != false
            || p["Links"] != serde_json::json!({})
            || p["ItemType"] != "Coin"
            || source["initial_phase"] != "WaitPending"
            || sensors.len() != 1
            || sensors[0]["Name"] != "Body"
        {
            return Err("unsupported cap hanger placement".into());
        }
        let component = |axis: &str| {
            p["Translate"][axis]
                .as_f64()
                .map(|v| v as f32)
                .ok_or("hanger position")
        };
        Ok(Self {
            id: p["Id"].as_str().ok_or("hanger id")?.into(),
            position: Vec3::new(component("X")?, component("Y")?, component("Z")?),
            relative: Vec3::ZERO,
            body_radius: sensors[0]["Radius"].as_f64().ok_or("hanger radius")? as f32,
            body_offset: sensors[0]["Y"].as_f64().ok_or("hanger offset")? as f32,
            quota: p["AppearItemNum"].as_u64().ok_or("hanger quota")? as u32,
            generated: 0,
            nerve: HangerNerve::Wait,
            step: 0,
            initial: true,
            early_release: false,
        })
    }

    /// `receiveMsg` CapStartLockOn: accepted in Wait (for the cap's non-collision Attack sensor).
    pub fn waiting(&self) -> bool {
        self.nerve == HangerNerve::Wait
    }

    /// `receiveMsg` CapKeepLockOn: accepted while the hanger keeps.
    pub fn keeps(&self) -> bool {
        self.nerve == HangerNerve::Keep
    }

    /// The accepted start sets the Keep nerve; it executes in the same frame.
    pub fn start(&mut self) {
        self.nerve = HangerNerve::Keep;
        self.step = 0;
    }

    /// One execution of the actor's nerve. `hold` is the keep input
    /// (`rs::isPlayerInputHoldCapKeepLockOn`), `distance` the horizontal distance to the player.
    pub fn tick(&mut self, hold: bool, distance: f32) {
        if std::mem::take(&mut self.initial) {
            return;
        }
        match self.nerve {
            HangerNerve::Wait => self.step += 1,
            HangerNerve::Keep => {
                let release = if self.generated != self.quota {
                    if self.step % COIN_CADENCE == 0 {
                        self.generated += 1;
                    }
                    self.early_release |= !hold;
                    false
                } else if hold {
                    self.step > KEEP_MAX_STEPS || distance > KEEP_MAX_DISTANCE
                } else {
                    self.step > KEEP_MIN_STEPS
                };
                if release {
                    self.set_nerve(HangerNerve::Release);
                } else {
                    self.step += 1;
                }
            }
            HangerNerve::Release => {
                if self.step > RELEASE_STEPS {
                    self.set_nerve(HangerNerve::Wait);
                } else {
                    self.step += 1;
                }
            }
        }
    }

    /// `al::setNerve` inside an execution: the new nerve shows with step 0 and first executes on
    /// the next frame.
    fn set_nerve(&mut self, nerve: HangerNerve) {
        self.nerve = nerve;
        self.step = 0;
    }

    pub fn diagnostic(&self) -> String {
        format!(
            "{}/{:?}/{}/{}{}",
            self.id,
            self.nerve,
            self.step,
            self.generated,
            if self.early_release {
                "/unported_early_release"
            } else {
                ""
            }
        )
    }
}
