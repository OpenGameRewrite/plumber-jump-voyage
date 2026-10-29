//! RocketFlower's cap pickup, equipment request and independently ticked watcher.
//! Only the upright, unlinked flower composition is supported here.
use super::*;

/// `RocketFlowerEquipWatcher::lineUp` (0x2fecd4), including the quaternion's
/// round trip rather than assuming the head matrix's axes remain orthogonal.
fn line_up(rotation: Quat, origin: Vec3, slot: usize) -> Vec3 {
    let side = pose_math::rotate_by_quat(Vec3::X, rotation);
    let up = pose_math::rotate_by_quat(Vec3::Y, rotation);
    let front = pose_math::rotate_by_quat(Vec3::Z, rotation);
    let offset = (side * 0. + up * 40.) - front * 10.;
    origin + offset * (slot + 1) as f32
}

#[derive(Clone, Debug)]
pub struct Flower {
    pub id: String,
    pub host: String,
    pub position: Vec3,
    pub phase: u8,
    pub step: i32,
    pub equip: Vec3,
    pub valid: bool,
    pub pending: Option<Vec3>,
    pub camera_clipped: bool,
    pub clipped: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RocketFlowers {
    pub flowers: Vec<Flower>,
    pub equipped: Vec<usize>,
    pub dash: bool,
    pub step: i32,
    pub head: Option<look_joint::Mat34>,
    pub stops: u32,
    pub pending_stop: u32,
    pub elapsed_stop: u32,
    pub pickup: bool,
    pub equip_request: bool,
    pub ended: bool,
}

impl RocketFlowers {
    fn equip(&mut self, index: usize) {
        if self.equipped.len() == 3 {
            let old = self.equipped.remove(0);
            self.flowers[old].phase = 0;
            self.flowers[old].step = -1;
            self.flowers[old].valid = false;
        }
        self.equipped.push(index);
        self.dash = true;
        self.step = -1;
        self.flowers[index].phase = 3;
        self.flowers[index].step = -1;
        self.equip_request = true;
    }

    pub fn diagnostic(&self) -> String {
        let flowers = self
            .flowers
            .iter()
            .map(|f| {
                format!(
                    "{}/{}/{}/{:08x}/{:08x}/{:08x}/{}/{}",
                    f.id,
                    f.phase,
                    f.step,
                    f.equip.x.to_bits(),
                    f.equip.y.to_bits(),
                    f.equip.z.to_bits(),
                    u8::from(f.valid),
                    u8::from(f.clipped)
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        format!(
            "{}/{}/{}/{}/{}/{}/{};flowers={}",
            u8::from(self.dash),
            self.step,
            self.equipped.len(),
            u8::from(self.pickup),
            u8::from(self.equip_request),
            u8::from(self.ended),
            self.stops,
            flowers
        )
    }
}

impl Player {
    pub fn seed_rocket_flowers(&mut self, seed: &serde_json::Value) -> Result<(), String> {
        let mut control = RocketFlowers {
            step: seed["watcher_step"].as_i64().ok_or("rocket watcher seed")? as i32 - 1,
            ..default()
        };
        for item in seed["flowers"].as_array().ok_or("rocket flowers seed")? {
            let p = item["position"].as_array().ok_or("rocket position")?;
            if p.len() != 3 {
                return Err("rocket position size".into());
            }
            control.flowers.push(Flower {
                id: item["id"].as_str().ok_or("rocket id")?.into(),
                host: item["host"].as_str().ok_or("rocket host")?.into(),
                position: Vec3::new(
                    p[0].as_f64().ok_or("rocket x")? as f32,
                    p[1].as_f64().ok_or("rocket y")? as f32,
                    p[2].as_f64().ok_or("rocket z")? as f32,
                ),
                phase: 0,
                step: item["step"].as_i64().ok_or("rocket step")? as i32 - 1,
                equip: Vec3::ZERO,
                valid: false,
                pending: None,
                camera_clipped: false,
                clipped: false,
            });
        }
        self.rocket = control;
        Ok(())
    }

    pub fn scene_stop_ticks(&self) -> u32 {
        self.rocket.elapsed_stop
    }

    pub fn rocket_diagnostic(&self) -> Option<String> {
        (!self.rocket.flowers.is_empty()).then(|| self.rocket.diagnostic())
    }

    pub fn rocket_look_sensors(
        &self,
        static_sensors: &[look_at::LookSensor],
        scene: Option<&crate::geometry::TriangleMesh>,
    ) -> Vec<look_at::LookSensor> {
        let mut sensors = static_sensors.to_vec();
        if let Some(mesh) = scene
            && let Some(source) = &mesh.pukupuku
        {
            sensors.push(look_at::LookSensor {
                host: "プクプク".into(),
                name: "Body".into(),
                kind: 9,
                radius: 80.,
                position: mesh.relative_vec(source.body),
                stage_position: source.body,
            });
        }
        for f in &self.rocket.flowers {
            for (name, kind, at) in [
                ("Body", 12, f.position + Vec3::Y * 50.),
                ("Equip", 6, f.equip),
            ] {
                sensors.push(look_at::LookSensor {
                    host: f.host.clone(),
                    name: name.into(),
                    kind,
                    radius: 70.,
                    position: scene.map_or(at, |m| m.relative_vec(at)),
                    stage_position: at,
                });
            }
        }
        sensors
    }

    pub(super) fn equipment_force_run(&self) -> bool {
        self.rocket.dash
    }

    /// Sources run before HackCap and Mario; scene stop advances the watcher's nerve
    /// clock, while the flower's LiveActor movement remains stopped.
    pub(super) fn tick_rocket_flowers(&mut self, scene: &Course) {
        let mut c = std::mem::take(&mut self.rocket);
        c.elapsed_stop = std::mem::take(&mut c.pending_stop);
        c.step += c.elapsed_stop as i32;
        c.pickup = false;
        c.equip_request = false;
        c.ended = false;
        let cap = self
            .cap
            .as_ref()
            .map(|cap| scene.in_water_frame(cap.pos / UNIT));
        let head = c
            .head
            .map(|m| scene.in_water_frame(look_joint::translation(&m)));
        let mut stop = false;
        for f in &mut c.flowers {
            if let Some(pending) = f.pending.take() {
                f.equip = pending;
            }
            f.clipped = f.phase == 0 && f.camera_clipped;
            if f.clipped {
                continue;
            }
            if f.step < 0 {
                f.step = 0;
            }
            match f.phase {
                1 => {
                    stop |= f.step == 0;
                    if let Some(cap) = cap {
                        f.equip = cap + Vec3::Y * 50.;
                    } else {
                        f.valid = true;
                        f.phase = 2;
                        f.step = -1;
                    }
                }
                2 => {
                    if let Some(head) = head {
                        f.equip = head;
                    }
                    if f.step >= 20 {
                        f.phase = 0;
                        f.step = -1;
                        f.valid = false;
                    }
                }
                _ => {}
            }
            f.step += 1;
        }
        if stop {
            c.stops += 2;
            c.pending_stop = 2;
        }
        if c.step < 0 {
            c.step = 0;
        }
        if c.dash
            && let Some(m) = c.head
        {
            let up = pose_math::normalize(Vec3::new(m[0][1], m[1][1], m[2][1]));
            let side = pose_math::normalize(Vec3::new(m[0][0], m[1][0], m[2][0]));
            if let Some(head) = head {
                let rotation = pose_math::quat_front_up(side, -up);
                let origin = (head + up * 16.) + side * 0.;
                for (slot, &i) in c.equipped.iter().enumerate() {
                    c.flowers[i].pending = Some(line_up(rotation, origin, slot));
                }
            }
        }
        if c.dash && c.step % 240 == 239 {
            let old = c.equipped.remove(0);
            c.flowers[old].phase = 0;
            c.flowers[old].step = -1;
            c.flowers[old].valid = false;
            c.flowers[old].clipped = c.flowers[old].camera_clipped;
            if c.equipped.is_empty() {
                c.dash = false;
                c.step = -1;
                c.ended = true;
            }
        }
        c.step += 1;
        self.rocket = c;
    }

    pub(super) fn rocket_sensor_phase(&mut self, scene: &Course) {
        let mut c = std::mem::take(&mut self.rocket);
        let feet = scene.in_water_frame(self.position_cm());
        let cap = self.prev_cap.map(|cap| scene.in_water_frame(cap / UNIT));
        for i in 0..c.flowers.len() {
            let f = &mut c.flowers[i];
            if f.phase == 0
                && !f.clipped
                && let Some(cap) = cap
            {
                // Cap item-get sphere (40), against the placed Body (70).
                let delta = cap - (f.position + Vec3::Y * 50.);
                if pose_math::dot(delta, delta) < 110. * 110. {
                    f.phase = 1;
                    f.step = -1;
                    c.pickup = true;
                }
            }
            if f.phase == 2 && f.valid {
                let hit = [(75., 50.), (110., 40.)].into_iter().any(|(y, r)| {
                    let delta = feet + Vec3::Y * y - f.equip;
                    pose_math::dot(delta, delta) < (r + 70.) * (r + 70.)
                });
                if hit {
                    c.equip(i);
                }
            }
        }
        self.rocket = c;
    }
}

pub(super) fn self_check() {
    checks::run();
}

mod checks {
    use super::*;

    pub(super) fn run() {
        seeds_are_validated_and_take_their_clocks_one_tick_early();
        the_watcher_keeps_three_flowers_and_displaces_the_oldest();
        pickup_stops_the_scene_and_expiry_ends_the_equipment();
        lineup_preserves_the_executable_quaternion_arithmetic();
        reconstructing_mario_does_not_tick_the_sources_twice();
        an_injected_run_keeps_the_catch_clip_chosen_before_it();
    }

    #[cfg(test)]
    #[test]
    fn port_checks() {
        run();
    }

    fn flower(id: &str) -> Flower {
        Flower {
            id: id.into(),
            host: "flower".into(),
            position: Vec3::ZERO,
            phase: 0,
            step: 0,
            equip: Vec3::ZERO,
            valid: false,
            pending: None,
            camera_clipped: false,
            clipped: false,
        }
    }

    fn reconstructing_mario_does_not_tick_the_sources_twice() {
        let mut player = Player::default();
        player.rocket.flowers = vec![flower("independent")];
        let course = Course::default();
        let orders = Orders::default();
        player.step(&orders, &course);
        let source = player.rocket.diagnostic();
        player.repeat_step_for_lockstep(&orders, &course);
        assert_eq!(player.rocket.diagnostic(), source);
        player.step(&orders, &course);
        assert_ne!(player.rocket.diagnostic(), source);
    }

    fn an_injected_run_keeps_the_catch_clip_chosen_before_it() {
        let mut player = Player {
            mode: Move::Run,
            catching: Some(2),
            catch_clip: "CatchCap",
            ..default()
        };
        player.resume_cap(&Snapshot {
            catching: Some(2),
            ..default()
        });
        assert_eq!(player.catch_clip, "CatchCap");
    }

    fn lineup_preserves_the_executable_quaternion_arithmetic() {
        let origin = Vec3::new(100., 120., -50.);
        assert_eq!(
            line_up(Quat::IDENTITY, origin, 2),
            Vec3::new(100., 240., -80.)
        );
        // Scalar-float executable oracle, including a non-unit input: normalizing
        // the quaternion or substituting matrix columns changes these results.
        let rotation = Quat::from_xyzw(0.2, -0.3, 0.4, 0.8);
        for (slot, expected) in [
            Vec3::new(72.8, 146.8, -53.5),
            Vec3::new(45.6, 173.6, -57.),
            Vec3::new(18.399994, 200.4, -60.5),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(line_up(rotation, origin, slot), expected);
        }
    }

    fn seed(position: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "watcher_step": 7,
            "flowers": [{"id": "a", "host": "h", "position": position, "step": 5}],
        })
    }

    fn seeds_are_validated_and_take_their_clocks_one_tick_early() {
        let mut player = Player::default();
        assert!(
            player
                .seed_rocket_flowers(&seed(serde_json::json!([1., 2.])))
                .is_err()
        );
        player
            .seed_rocket_flowers(&seed(serde_json::json!([1., 2., 3.])))
            .expect("a complete seed");
        // The recorded clocks are the ones of the frame after the seed's tick.
        assert_eq!(player.rocket.step, 6);
        assert_eq!(player.rocket.flowers[0].step, 4);
        assert_eq!(player.rocket.flowers[0].position, Vec3::new(1., 2., 3.));
        assert!(!player.rocket.dash);
    }

    fn the_watcher_keeps_three_flowers_and_displaces_the_oldest() {
        let mut control = RocketFlowers {
            flowers: (0..4).map(|i| flower(&format!("f{i}"))).collect(),
            ..default()
        };
        for i in 0..4 {
            control.equip(i);
        }
        assert_eq!(control.equipped, vec![1, 2, 3]);
        assert!(control.dash && control.equip_request);
        assert_eq!(control.flowers[0].phase, 0);
        assert!(!control.flowers[0].valid);
        assert!(control.flowers[1..].iter().all(|f| f.phase == 3));
    }

    fn pickup_stops_the_scene_and_expiry_ends_the_equipment() {
        let course = Course::default();
        let mut player = Player::default();
        let mut picked = flower("picked");
        picked.phase = 1;
        picked.step = 0;
        player.rocket.flowers = vec![picked];
        player.tick_rocket_flowers(&course);
        // The first Follow step asks the scene to stop for two ticks, which the watcher's
        // clock (not the flower's) spends on the next tick; with no cap the flower turns
        // into the equipment item.
        assert_eq!(player.rocket.pending_stop, 2);
        assert_eq!(
            (
                player.rocket.flowers[0].phase,
                player.rocket.flowers[0].valid
            ),
            (2, true)
        );
        player.tick_rocket_flowers(&course);
        assert_eq!(player.scene_stop_ticks(), 2);

        let mut worn = flower("worn");
        worn.phase = 3;
        player.rocket = RocketFlowers {
            flowers: vec![worn],
            equipped: vec![0],
            dash: true,
            step: 239,
            ..default()
        };
        player.tick_rocket_flowers(&course);
        assert!(player.rocket.ended && !player.rocket.dash);
        assert!(player.rocket.equipped.is_empty());
        assert_eq!(player.rocket.flowers[0].phase, 0);
    }
}
