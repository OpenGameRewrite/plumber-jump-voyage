//! Stationary CapTrampoline and its RequestPlayerSpinJump (1.0.0).
//!
//! Phases: 0 Wait, 1 Open, 2 Jump, 3 OpenStart, 4 ReOpen. Three routes reach Mario:
//! the actor's own attack sensor (`attackSensor` 0x24332c, power 60/50, cooldown-gated),
//! the TrampleHit receive route for a hip drop (`receiveMsg` 0x243710, power 68/55, no
//! cooldown test, answers false to the hip-drop message) and, for a closed source, the
//! CapHit Reflect (a bounce, not a spin jump). A cap attack on CapHit (0x243910) restarts the
//! source in ReOpen with the cooldown that blocks the first route.
use super::*;

/// `ReOpen` action of the original `CapTrampoline.bfres` (clip length in frames).
const REOPEN_FRAMES: i32 = 27;
/// The nerve gate of `attackSensor` and `receiveMsg` (0x243348..0x243364, 0x2435e4..0x243618):
/// Wait takes the closed-state branch; Open, Jump, OpenStart and ReOpen the open one.
const WAIT: u8 = 0;
const OPEN: u8 = 1;
const JUMP: u8 = 2;
const OPEN_START: u8 = 3;
const RE_OPEN: u8 = 4;

#[derive(Clone, Debug)]
pub struct Trampoline {
    pub id: String,
    pub position: Vec3,
    /// The same position relative to the replay's start (set by the stage loader).
    pub relative: Vec3,
    pub moon: bool,
    pub continue_open: bool,
    pub phase: u8,
    pub step: i32,
    pub cooldown: i32,
    pub close_counter: i32,
    pub initial: bool,
}

impl Trampoline {
    pub fn load(source: &serde_json::Value) -> Result<Self, String> {
        let p = &source["placement"];
        if !matches!(
            source["initial_phase"].as_str(),
            Some("OpenPending" | "WaitPending")
        ) || p["UnitConfigName"] != "CapTrampoline"
            || p["IsConnectToCollision"] != false
            || p["IsLinkDest"] != false
            || p["Scale"] != serde_json::json!({"X":1.,"Y":1.,"Z":1.})
            || p["Rotate"]["X"] != 0.
            || p["Rotate"]["Z"].as_f64() != Some(0.)
        {
            return Err("unsupported connected trampoline".into());
        }
        let component = |axis: &str| {
            p["Translate"][axis]
                .as_f64()
                .map(|v| v as f32)
                .ok_or("trampoline position")
        };
        Ok(Self {
            id: p["Id"].as_str().ok_or("trampoline id")?.into(),
            position: Vec3::new(component("X")?, component("Y")?, component("Z")?),
            relative: Vec3::ZERO,
            moon: p["IsMoon"].as_bool().ok_or("trampoline moon")?,
            continue_open: p["IsContinueOpen"]
                .as_bool()
                .ok_or("trampoline open flag")?,
            phase: u8::from(source["initial_phase"] == "OpenPending"),
            step: if source["initial_phase"] == "OpenPending" {
                -1
            } else {
                0
            },
            cooldown: 0,
            close_counter: 300,
            initial: source["initial_phase"] == "OpenPending",
        })
    }

    /// Mario's foot (sphere 40, raised 40) strictly overlapping TrampleHit (120, lowered 20)
    /// with the attacker's speed and direction toward the sensor (`calcDirBetweenSensors`):
    /// the tests shared by both routes, which differ in the speed floor.
    fn foot_hits_trample_sensor(&self, feet: Vec3, velocity: Vec3, min_speed: f32) -> bool {
        let delta = self.position - Vec3::Y * 20. - (feet + Vec3::Y * 40.);
        pose_math::squared_length(delta) < 160. * 160.
            && vector_lib::length(velocity) >= min_speed
            && pose_math::dot(velocity, delta) >= 0.
    }

    /// `attackSensor` 0x24332c: every supported nerve but Wait, speed 1.45, cooldown <= 0.
    fn accepts(&self, feet: Vec3, velocity: Vec3, min_speed: f32) -> bool {
        self.phase != WAIT
            && self.cooldown <= 0
            && self.foot_hits_trample_sensor(feet, velocity, min_speed)
    }

    /// Whether `receiveMsg` 0x243910 takes a cap attack on CapHit: open nerves, cooldown <= 0.
    pub fn accepts_cap_attack(&self) -> bool {
        self.phase != WAIT && self.cooldown <= 0
    }

    /// `receiveMsg` 0x243924..0x24398c: cooldown 10, close counter 300, ReOpen (0x1ce4ec0).
    pub fn receive_cap_attack(&mut self) {
        self.cooldown = 10;
        self.close_counter = 300;
        self.phase = RE_OPEN;
        self.step = -1;
    }

    /// The Foot sphere (centre raised 40) of the TrampleHit route, `receiveMsg` 0x243710..0x243838:
    /// every nerve but Wait and Jump (0x243754), speed 8.5; there is no cooldown test.
    fn receive_hip_drop(&mut self, feet: Vec3, velocity: Vec3) -> bool {
        if matches!(self.phase, WAIT | JUMP) || !self.foot_hits_trample_sensor(feet, velocity, 8.5)
        {
            return false;
        }
        self.close_counter = 300; // 0x2438f4
        self.phase = JUMP; // 0x243904
        self.step = -1;
        true
    }

    /// What the cap sees of this source: its CapHit sensor (110, raised 60) and whether it takes
    /// a cap attack now.
    pub fn cap_hit_target(&self) -> crate::cap::CapHitTarget {
        crate::cap::CapHitTarget {
            centre: self.relative + Vec3::Y * 60.,
            accepts: self.accepts_cap_attack(),
        }
    }

    // receiveMsg 0x24362c..0x2436ec: Wait/CapHit Reflect. InitSensor
    // supplies Foot 40/Y40 and CapHit 110/Y60; HitSensor uses strict overlap.
    fn receive_hip_drop_reflect(&mut self, feet: Vec3) -> bool {
        let foot = feet + Vec3::Y * 40.;
        let cap_hit = self.position + Vec3::Y * 60.;
        if self.phase != WAIT
            || self.cooldown > 0
            || pose_math::squared_length(foot - cap_hit) >= 150. * 150.
            || foot.y <= cap_hit.y - 100.
        {
            return false;
        }
        self.cooldown = 10; // 0x2436c0
        self.close_counter = 300; // 0x2436d8
        self.phase = OPEN_START; // OpenStart, 0x2436e0..0x2436e8
        self.step = -1;
        true
    }

    /// The float table at 0x18869b8 `[60, 50, 68, 55]`: attack route, then hip-drop route, by IsMoon.
    fn power(&self) -> f32 {
        if self.moon { 50. } else { 60. }
    }

    fn hip_drop_power(&self) -> f32 {
        if self.moon { 55. } else { 68. }
    }

    fn launch(&mut self, normal_attack: bool) {
        self.phase = JUMP;
        self.step = -1;
        self.close_counter = 300;
        if normal_attack {
            self.cooldown = 10;
        }
    }

    fn tick(&mut self) {
        self.cooldown = (self.cooldown - 1).max(0);
        if self.step < 0 {
            self.step = 0;
        }
        // OpenStart15 (original CapTrampoline action metadata), exeOpenStart
        // 0x243a54..0x243a78 tests its end and selects Open.
        if self.phase == OPEN_START && self.step == 15 {
            self.phase = OPEN;
            self.step = -1;
        }
        // ReOpen27 (original action metadata) ends the nerve like OpenStart's.
        if self.phase == RE_OPEN && self.step == REOPEN_FRAMES {
            self.phase = OPEN;
            self.step = -1;
        }
        if self.phase == OPEN && !self.continue_open && self.step >= 0 {
            // exeOpen 0x243a88: finite opening consumes its close counter.
            self.close_counter -= 1;
        }
        // Jump's 19-frame action is tested after the animation update.
        if self.phase == JUMP && self.step == 19 {
            self.phase = OPEN;
            self.step = -1;
        }
        self.step += 1;
    }
}

impl Player {
    fn request_hip_drop_reflect(&mut self) {
        // attackSensor 0x427d80..0x427dc8: attack trigger3, request type6.
        // Jump::appear 0x4713d8..0x471404, PlayerConst.cpp:216-217:
        // TrampleHipDropJumpPower35 / Gravity1.5 / extension0.
        // PlayerActorHakoniwa/ActorHitReactionCtrl.byml, StompHit:
        // IsStopSceneForHitEffect=true, StopSceneFrame=2.
        // SceneStopCtrl receives delay2 (HitReactionKeeper::start 0x90cb80..0x90cb8c);
        // its update 0x9cf150 spends the delay before the two stopped ticks.
        // Replay steps count player movements; the stopped scene has no player step.
        self.pick_jump_kind();
        self.events.push(Fx::HipDropReflectHit);
        self.jump_with(
            Move::Jump,
            None,
            self.constants.trample_hip_drop_jump_power,
            self.constants.trample_hip_drop_gravity,
            |p| p.jump_extend = false,
        );
        self.hip_drop_reflect = true;
        self.nerve.deferred = false;
        self.nerve.renewed = false;
        self.nerve.actor = "Jump";
        self.nerve.step = 0;
        self.sensor_restart = true;
    }
    pub(super) fn trampoline_sensor_phase(&mut self, scene: &Course) {
        let feet = scene.in_water_frame(self.position_cm());
        let velocity = self.v;
        let mut request = None;
        let mut reflect = false;
        let hip_drop = self.mode == Move::GroundPound && self.clip() == "HipDrop";
        for source in &mut self.trampolines {
            if source.accepts(feet, velocity, 1.45) {
                request = Some(source.power());
                source.launch(true);
            }
            // attackSensor precedes the player's send: the open source wins.
            let received = hip_drop && source.receive_hip_drop_reflect(feet);
            reflect |= received;
            if received {
                continue; // receive happens after this source's movement this frame
            }
            if source.initial {
                // The exported INITIAL pending nerve has not executed at the
                // first Mario sample; actor scheduling begins after that sample.
                source.initial = false;
            } else {
                source.tick();
            }
            // The hip-drop message (`PlayerObjHipDrop`, after Reflect and the own-cap variant
            // were refused) arrives after the actor's update of the frame, so the cooldown
            // has already counted down; the receiver answers false whatever Mario does.
            if hip_drop && request.is_none() && source.receive_hip_drop(feet, velocity) {
                request = Some(source.hip_drop_power());
            }
        }
        if reflect {
            self.request_hip_drop_reflect();
        }
        if let Some(power) = request {
            self.request_spin_flower(power);
        }
    }

    pub fn trampoline_diagnostic(&self) -> String {
        let sources = self
            .trampolines
            .iter()
            .map(|s| {
                format!(
                    "{}/{}/{}/{}/{}",
                    s.id, s.phase, s.step, s.cooldown, s.close_counter
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        if sources.is_empty() {
            sources
        } else {
            format!(
                "{sources};jump={:08x}/{}",
                self.spin_flower.power.to_bits(),
                self.spin_flower.stay
            )
        }
    }
}

pub(super) fn self_check() {
    checks::attack_preempts_hip_drop_and_obeys_strict_contact_boundaries();
    checks::closed_reflect_opens_once_and_launches_the_shared_jump();
    checks::cap_hit_blocks_normal_send_but_not_the_hip_drop_receiver();
}

mod checks {
    use super::*;

    #[cfg_attr(test, test)]
    pub(super) fn closed_reflect_opens_once_and_launches_the_shared_jump() {
        let mut source = Trampoline {
            id: "closed".into(),
            position: Vec3::ZERO,
            relative: Vec3::ZERO,
            moon: false,
            continue_open: false,
            phase: 0,
            step: 1,
            cooldown: 0,
            close_counter: 300,
            initial: false,
        };
        assert!(!source.receive_hip_drop_reflect(Vec3::Y * -80.));
        assert!(!source.receive_hip_drop_reflect(Vec3::new(150., 20., 0.)));
        assert!(source.receive_hip_drop_reflect(Vec3::Y * 73.));
        assert_eq!((source.phase, source.step, source.cooldown), (3, -1, 10));
        assert!(!source.receive_hip_drop_reflect(Vec3::Y * 73.));
        for _ in 0..16 {
            source.tick();
        }
        assert_eq!(
            (
                source.phase,
                source.step,
                source.cooldown,
                source.close_counter
            ),
            (1, 0, 0, 300)
        );
        source.tick();
        assert_eq!(source.close_counter, 299);
        let mut player = Player {
            mode: Move::GroundPound,
            v: Vec3::NEG_Y * 45.,
            ..default()
        };
        player.request_hip_drop_reflect();
        player.begin_step(player.acting_nerve());
        assert_eq!(player.mode, Move::Jump);
        assert_eq!(player.v.y, 35.);
        assert_eq!(player.gravity, 1.5);
        assert!(!player.jump_extend);
    }

    #[cfg_attr(test, test)]
    pub(super) fn attack_preempts_hip_drop_and_obeys_strict_contact_boundaries() {
        let mut s = Trampoline {
            id: "test".into(),
            position: Vec3::ZERO,
            relative: Vec3::ZERO,
            moon: false,
            continue_open: true,
            phase: 1,
            step: 20,
            cooldown: 0,
            close_counter: 300,
            initial: false,
        };
        let feet = Vec3::Y * 80.;
        assert!(s.accepts(feet, Vec3::NEG_Y * 45., 1.45));
        assert_eq!(s.power(), 60.);
        s.moon = true;
        assert_eq!(s.power(), 50.);
        s.launch(true);
        assert!(!s.accepts(feet, Vec3::NEG_Y * 45., 8.5));
        s.tick();
        assert_eq!((s.phase, s.step, s.cooldown), (2, 1, 9));
        for _ in 0..19 {
            s.tick();
        }
        assert_eq!((s.phase, s.step, s.cooldown), (1, 0, 0));
        assert!(!s.accepts(Vec3::Y * 100., Vec3::NEG_Y * 45., 1.45));
        assert!(!s.accepts(feet, Vec3::Y * 45., 1.45));
    }

    #[cfg_attr(test, test)]
    pub(super) fn cap_hit_blocks_normal_send_but_not_the_hip_drop_receiver() {
        let mut source = Trampoline {
            id: "test".into(),
            position: Vec3::ZERO,
            relative: Vec3::ZERO,
            moon: false,
            continue_open: true,
            phase: OPEN,
            step: 20,
            cooldown: 0,
            close_counter: 300,
            initial: false,
        };
        assert!(source.accepts_cap_attack());
        source.receive_cap_attack();
        source.tick();
        assert_eq!(
            (source.phase, source.step, source.cooldown),
            (RE_OPEN, 1, 9)
        );
        assert!(!source.accepts_cap_attack());
        let feet = Vec3::Y * 80.;
        assert!(!source.accepts(feet, Vec3::NEG_Y * 45., 1.45));
        assert!(!source.receive_hip_drop(feet, Vec3::NEG_Y * 8.));
        assert!(source.receive_hip_drop(feet, Vec3::NEG_Y * 45.));
        assert_eq!(source.hip_drop_power(), 68.);
        assert_eq!((source.phase, source.step, source.cooldown), (JUMP, -1, 9));
        assert!(!source.receive_hip_drop(feet, Vec3::NEG_Y * 45.));
        source.moon = true;
        assert_eq!(source.hip_drop_power(), 55.);
    }
}
