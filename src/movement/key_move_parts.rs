//! The upright two-key Loop placements of KeyMoveMapParts. No switches or sign actions.
use bevy::prelude::Vec3;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct KeyMovePart {
    pub id: String,
    pub part: usize,
    pub keys: [Vec3; 2],
    pub wait: [u32; 2],
    pub duration: [u32; 2],
    pub moving: bool,
    pub key: usize,
    pub step: u32,
    pub position: Vec3,
}
impl KeyMovePart {
    /// KeyMoveMapParts::exeWait/exeMove: the duration step executes too; the
    /// new nerve is pending at step zero until the next source update.
    pub fn tick(&mut self) {
        if self.moving {
            let rate = (self.step as f32 / self.duration[self.key] as f32).clamp(0., 1.);
            let a = self.keys[self.key];
            let b = self.keys[1 - self.key];
            self.position = a + (b - a) * rate;
            if self.step >= self.duration[self.key] {
                self.key = 1 - self.key;
                self.moving = false;
                self.step = 0;
                return;
            }
        } else if self.step >= self.wait[self.key] {
            self.moving = true;
            self.step = 0;
            return;
        }
        self.step += 1;
    }
    pub fn reset(&mut self) {
        self.key = 0;
        self.moving = false;
        self.step = 0;
        self.position = self.keys[0];
    }
    /// Only the independently observed initial source state is an input to replay.
    pub fn seed(&mut self, row: &HashMap<String, String>) -> Result<(), String> {
        let get = |key: &str| {
            row.get(key)
                .map(String::as_str)
                .ok_or_else(|| format!("part seed {key}"))
        };
        if get("id")? != self.id
            || get("key_count")? != "2"
            || get("going_to_end")? != "1"
            || get("key_stop")? != "0"
            || get("key_restart")? != "0"
            || get("dead")? != "0"
            || get("clipped")? != "0"
        {
            return Err("unsupported key-move seed".into());
        }
        self.moving = match get("phase")? {
            "Move" => true,
            "Wait" => false,
            _ => return Err("part phase".into()),
        };
        self.key = get("key_idx")?.parse().map_err(|_| "part key")?;
        self.step = get("step")?.parse().map_err(|_| "part step")?;
        if self.key >= 2
            || self.step
                > if self.moving {
                    self.duration[self.key]
                } else {
                    self.wait[self.key]
                }
        {
            return Err("part step out of range".into());
        }
        let components = get("pos_bits")?
            .split(',')
            .map(|s| u32::from_str_radix(s, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "part position bits")?;
        self.position = Vec3::from_slice(&components);
        Ok(())
    }
    pub fn diagnostic(&self) -> String {
        format!(
            "{}/{}/{}/{}/{:08x}/{:08x}/{:08x}",
            self.id,
            if self.moving { "Move" } else { "Wait" },
            self.key,
            self.step,
            self.position.x.to_bits(),
            self.position.y.to_bits(),
            self.position.z.to_bits()
        )
    }
}

pub(super) fn self_check() {
    inclusive_nerve_steps_and_rebuild();
}

#[cfg_attr(test, test)]
fn inclusive_nerve_steps_and_rebuild() {
    let mut part = KeyMovePart {
        id: "test".into(),
        part: 0,
        keys: [Vec3::Y * 1050., Vec3::Y * 450.],
        wait: [60; 2],
        duration: [120; 2],
        moving: false,
        key: 0,
        step: 0,
        position: Vec3::Y * 1050.,
    };
    let mut frames = 0;
    loop {
        part.tick();
        frames += 1;
        if !part.moving && part.key == 0 && part.step == 0 {
            break;
        }
        assert!(frames < 400);
    }
    assert_eq!(frames, 2 * (60 + 1 + 120 + 1));
    part.tick();
    part.tick();
    part.reset();
    assert_eq!((part.key, part.step, part.moving), (0, 0, false));
    assert_eq!(part.position, part.keys[0]);
}
