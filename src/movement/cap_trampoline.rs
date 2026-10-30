//! Stationary CapTrampoline and its RequestPlayerSpinJump (1.0.0).
use super::*;

#[derive(Clone, Debug)]
pub struct Trampoline {
    pub id: String,
    pub position: Vec3,
    pub moon: bool,
    pub phase: u8,
    pub step: i32,
    pub cooldown: i32,
    pub close_counter: i32,
    pub initial: bool,
}

impl Trampoline {
    pub fn load(source: &serde_json::Value) -> Result<Self, String> {
        let p = &source["placement"];
        if source["initial_phase"] != "OpenPending"
            || p["UnitConfigName"] != "CapTrampoline"
            || p["IsContinueOpen"] != true
            || p["IsConnectToCollision"] != false
            || p["IsLinkDest"] != false
            || p["Scale"] != serde_json::json!({"X":1.,"Y":1.,"Z":1.})
            || p["Rotate"]["X"] != 0.
            || p["Rotate"]["Z"].as_f64() != Some(0.)
        {
            return Err("unsupported connected/closed trampoline".into());
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
            moon: p["IsMoon"].as_bool().ok_or("trampoline moon")?,
            phase: 1,
            step: -1,
            cooldown: 0,
            close_counter: 300,
            initial: true,
        })
    }

    fn accepts(&self, feet: Vec3, velocity: Vec3, min_speed: f32) -> bool {
        if self.phase != 1 || self.cooldown > 0 {
            return false;
        }
        let delta = self.position - Vec3::Y * 20. - (feet + Vec3::Y * 40.);
        pose_math::squared_length(delta) < 160. * 160.
            && vector_lib::length(velocity) >= min_speed
            && pose_math::dot(velocity, delta) >= 0.
    }

    fn power(&self) -> f32 {
        if self.moon { 50. } else { 60. }
    }

    fn launch(&mut self, normal_attack: bool) {
        self.phase = 2;
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
        // Jump's 19-frame action is tested after the animation update.
        if self.phase == 2 && self.step == 19 {
            self.phase = 1;
            self.step = -1;
        }
        self.step += 1;
    }
}

impl Player {
    pub(super) fn trampoline_sensor_phase(&mut self, scene: &Course) {
        let feet = scene.in_water_frame(self.position_cm());
        let velocity = self.v;
        let mut request = None;
        for source in &mut self.trampolines {
            if source.accepts(feet, velocity, 1.45) {
                request = Some(source.power());
                source.launch(true);
            }
            if source.initial {
                // The exported INITIAL pending nerve has not executed at the
                // first Mario sample; actor scheduling begins after that sample.
                source.initial = false;
            } else {
                source.tick();
            }
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
}

mod checks {
    use super::*;

    #[cfg_attr(test, test)]
    pub(super) fn attack_preempts_hip_drop_and_obeys_strict_contact_boundaries() {
        let mut s = Trampoline {
            id: "test".into(),
            position: Vec3::ZERO,
            moon: false,
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
}
