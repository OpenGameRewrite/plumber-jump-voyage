//! SignBoardNormal's reflective collision (SMO 1.0.0 `SignBoard.cpp`).
//!
//! The sign is a static actor whose only behaviour is its answer to `CapReflectCollide`: any
//! non-wall board accepts it, plays its `Reaction` clip and asks the attacker's hit reaction
//! (`SignBoard::receiveMsg`). The cap's side of the exchange is `cap.rs`: its throw nerves ask the
//! actor of the ground, wall and ceiling collision they touch (`tryCollideReflectReaction`).
use bevy::prelude::Vec3;

/// `SignBoardNormal.bfres`: the Reaction clip runs 11 frames and the nerve ends with its action.
const REACTION_FRAMES: u32 = 11;

#[derive(Clone, Debug)]
pub struct SignBoard {
    pub id: String,
    /// Index of the sign's collision part in the stage export's `objects`.
    pub object: usize,
    pub position: Vec3,
    reacting: bool,
    step: u32,
    /// The exported INITIAL pending Wait has not executed at the first sample.
    initial: bool,
}

impl SignBoard {
    pub fn load(source: &serde_json::Value, objects: &[serde_json::Value]) -> Result<Self, String> {
        let p = &source["placement"];
        // `SignBoard::init` takes the plain `al::initActor` path for every board that is not
        // SignBoardNormalWall (whose receiver rejects the message and whose board is shifted).
        if p["UnitConfigName"] != "SignBoardNormal"
            || p["IsLinkDest"] != false
            || p["Links"] != serde_json::json!({})
            || source["initial_phase"] != "WaitPending"
            || source["collision"] != serde_json::json!({"Sensor": "Collision"})
        {
            return Err("unsupported sign board placement".into());
        }
        let component = |axis: &str| {
            p["Translate"][axis]
                .as_f64()
                .map(|v| v as f32)
                .ok_or("sign position")
        };
        let object = objects
            .iter()
            .position(|o| o["id"] == p["Id"] && o["model"] == "SignBoardNormal")
            .ok_or("sign collision part missing from the export")?;
        Ok(Self {
            id: p["Id"].as_str().ok_or("sign id")?.into(),
            object,
            position: Vec3::new(component("X")?, component("Y")?, component("Z")?),
            reacting: false,
            step: 0,
            initial: true,
        })
    }

    /// `receiveMsg`: CapReflectCollide sets the Reaction nerve and is accepted.
    pub fn reflect(&mut self) {
        self.reacting = true;
        self.step = 0;
    }

    pub fn tick(&mut self) {
        if std::mem::take(&mut self.initial) {
            return;
        }
        // exeReaction: the clip starts on the first step; its end returns to Wait.
        if self.reacting && self.step == REACTION_FRAMES {
            self.reacting = false;
            self.step = 0;
        } else {
            self.step += 1;
        }
    }

    pub fn diagnostic(&self) -> String {
        format!("{}/{}/{}", self.id, u8::from(self.reacting), self.step)
    }
}
