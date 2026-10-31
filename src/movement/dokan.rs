//! Linked stationary Dokan placement and puppet poses, SMO 1.0.0.
//! Nerve/camera/bind execution is not implied by loading these source poses.
use super::dimension::{DimensionAreas, DimensionKeeper};
use super::pose_math::{dot, make_mtx_front_up, make_mtx_up_front, quat_front_up, quat_to_axes};
use super::vector_lib::{is_near_zero, parallelize_vec};
use bevy::prelude::{Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Side,
    UpsideDown,
}

#[derive(Clone, Debug)]
pub struct Dokan {
    pub id: String,
    pub destination_id: String,
    pub is_2d: bool,
    pub direction: Direction,
    pub matrix: [f32; 12],
    pub bind_radius: f32,
    pub collision_part: Option<usize>,
    bind_offset: Vec3,
}

impl Dokan {
    pub fn load(source: &serde_json::Value, areas: &DimensionAreas) -> Result<Self, String> {
        let placement = &source["placement"];
        let is_2d = match placement["UnitConfigName"].as_str() {
            Some("Dokan2D") => true,
            Some("Dokan") => false,
            _ => return Err("unsupported pipe class".into()),
        };
        if placement["IsConnectToCollision"] != false
            || placement["IsNoSnap"] != false
            || placement["IsMoon"] != false
            || placement["IsExitOnly"] != false
            || placement["Scale"] != serde_json::json!({"X":1.,"Y":1.,"Z":1.})
        {
            return Err("unsupported moving/no-snap/Moon/exit-only pipe".into());
        }
        let raw = source["raw_matrix"]
            .as_array()
            .filter(|m| m.len() == 12)
            .ok_or("Dokan raw placement matrix")?;
        let mut matrix = [0.; 12];
        for (out, value) in matrix.iter_mut().zip(raw) {
            *out = value
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("Dokan matrix number")? as f32;
            if !out.is_finite() {
                return Err("Dokan matrix exceeds f32 range".into());
            }
        }
        if crate::prism::PartFrame::new(matrix).is_none() {
            return Err("singular Dokan matrix".into());
        }
        let mut position = Vec3::new(matrix[3], matrix[7], matrix[11]);
        let up = Vec3::new(matrix[1], matrix[5], matrix[9]);
        let front = Vec3::new(matrix[2], matrix[6], matrix[10]);
        let mut keeper = DimensionKeeper::default();
        keeper.update(areas, position);
        if is_2d && !keeper.in_area {
            return Err("Dokan2D outside movement area".into());
        }
        let gravity = if is_2d {
            areas
                .single_movement_at(position)
                .ok_or("Dokan2D overlapping movement areas unsupported")?
                .gravity(position)
                .map_or(Vec3::NEG_Y, |(direction, _)| direction)
        } else {
            Vec3::NEG_Y
        };
        // DokanInfo ctor: ordinary .001 at 0x256b18 / 2D .1 at
        // 0x256c04, both passed to al::isNear 0x91f31c (strict difference).
        let tolerance = if is_2d { 0.1 } else { 0.001 };
        let alignment = dot(up, gravity);
        let direction = if (alignment + 1.).abs() < tolerance {
            Direction::Up
        } else if alignment.abs() < tolerance {
            Direction::Side
        } else if (alignment - 1.).abs() < tolerance {
            Direction::UpsideDown
        } else {
            Direction::Up
        }; // original zero-initialized classification
        if is_2d {
            // Dokan::init 0x2570e4..0x2570f4; literal 500 at 0x187fc7c.
            let mut velocity = Vec3::ZERO;
            keeper.snap(areas, &mut position, &mut velocity, 500.);
            let q = keeper.snap_pose(areas, position, up, front, true);
            matrix = pose_matrix(q, position);
        }
        let sensor = source["sensors"]
            .as_array()
            .and_then(|a| a.iter().find(|s| s["Name"] == "Bind"))
            .ok_or("Dokan Bind sensor")?;
        let bind_radius = sensor["Radius"]
            .as_f64()
            .filter(|v| v.is_finite() && *v > 0.)
            .ok_or("Dokan Bind radius")? as f32;
        let component = |name| sensor[name].as_f64().unwrap_or(0.) as f32;
        Ok(Self {
            id: placement["Id"].as_str().ok_or("Dokan id")?.into(),
            destination_id: source["destination_id"]
                .as_str()
                .ok_or("Dokan destination")?
                .into(),
            is_2d,
            direction,
            matrix,
            bind_radius,
            collision_part: None,
            bind_offset: Vec3::new(component("X"), component("Y"), component("Z")),
        })
    }

    /// `DokanPuppetController::judgeMsgBindStart` (0x25b2f0) for an ordinary
    /// side entrance met by a 3D player: the straight-line branch at
    /// 0x25b3d8..0x25b6e8. The bind sensor overlap itself is the caller's.
    pub fn accepts_side_bind(&self, player_pos: Vec3, front: Vec3, velocity: Vec3) -> bool {
        if self.is_2d || self.direction != Direction::Side {
            return false;
        }
        // al::isNearZero 0x91df0c with the .001 literal 0x187fc80 (0x25b454).
        if is_near_zero(velocity, 0.001) {
            return false;
        }
        // The speed along the pipe's up axis (al::parallelizeVec 0x923fe0)
        // must exceed the 0.4 literal at 0x187fea8 (0x25b478).
        let up = self.up();
        if is_near_zero(parallelize_vec(up, velocity), 0.4) {
            return false;
        }
        // The player must face against the pipe's up axis (0x25b678..0x25b6a0)
        // while standing below its origin: the world down direction (negated
        // sead::Vector3f::ey, 0x25b608) against the normalized offset.
        if dot(front, up) >= 0. {
            return false;
        }
        let to_player = player_pos - self.position();
        let direction = if dot(to_player, to_player) > 0. {
            to_player.normalize()
        } else {
            Vec3::Z
        };
        dot(Vec3::NEG_Y, direction) > 0.
    }

    pub fn bind_radius(&self) -> f32 {
        self.bind_radius
    }
    /// Upright branch of judgeMsgBindStart, 0x25b3b8..0x25b3d4. The caller
    /// checks active player sensor, pipe ReadyBind nerve, dimension and puppet lock.
    pub fn accepts_upright_bind(&self, model_2d: bool, squat: bool) -> bool {
        self.direction == Direction::Up && self.is_2d == model_2d && squat
    }

    /// calcMoveMtxUp's 2D branch (0x25bd44..0x25bdec); helper 0x25cf40
    /// uses local Y=200. The target front retains Mario's side of the pipe.
    pub fn upright_2d_entry(&self, player_front: Vec3) -> Option<[f32; 12]> {
        if !self.is_2d || self.direction != Direction::Up {
            return None;
        }
        let mut front = self.side();
        if dot(player_front, front) < 0. {
            front = -front;
        }
        Some(matrix_from_axes(
            make_mtx_front_up(front, self.up()),
            self.local_offset(Vec3::new(0., 200., 0.)),
        ))
    }

    /// Ordinary side destination in warp, 0x25bc5c..0x25bca8: local Y=290
    /// (constructed at 0x25bc5c), world Y=-100 (0x187fd5c). The pose uses
    /// the pipe's up as front and world up (0x25ba7c..0x25bb94).
    pub fn side_exit(&self) -> Option<(Vec3, Quat)> {
        if self.is_2d || self.direction != Direction::Side {
            return None;
        }
        let mut position = self.local_offset(Vec3::new(0., 290., 0.));
        position.x += 0.;
        position.y += -100.;
        position.z += 0.;
        Some((position, quat_front_up(self.up(), Vec3::Y)))
    }

    pub fn position(&self) -> Vec3 {
        Vec3::new(self.matrix[3], self.matrix[7], self.matrix[11])
    }
    pub fn up(&self) -> Vec3 {
        Vec3::new(self.matrix[1], self.matrix[5], self.matrix[9])
    }
    pub fn side(&self) -> Vec3 {
        Vec3::new(self.matrix[0], self.matrix[4], self.matrix[8])
    }

    fn local_offset(&self, offset: Vec3) -> Vec3 {
        let m = &self.matrix;
        Vec3::new(
            m[3] + ((offset.x * m[0] + offset.y * m[1]) + offset.z * m[2]),
            m[7] + ((offset.x * m[4] + offset.y * m[5]) + offset.z * m[6]),
            m[11] + ((offset.x * m[8] + offset.y * m[9]) + offset.z * m[10]),
        )
    }

    pub fn bind_center(&self) -> Vec3 {
        self.local_offset(self.bind_offset)
    }

    /// Ordinary side entrance, calcMoveMtxSide 0x25bf48..0x25bfe8.
    /// 210 is constructed at 0x25bf8c; world offset -100 is 0x187fd5c.
    pub fn side_entry_target(&self) -> Option<[f32; 12]> {
        if self.is_2d || self.direction != Direction::Side {
            return None;
        }
        let mut position = self.local_offset(Vec3::new(0., 210., 0.));
        position.x += 0.;
        position.y += -100.;
        position.z += 0.;
        let [side, up, front] = make_mtx_up_front(Vec3::Y, -self.up());
        Some(matrix_from_axes([side, up, front], position))
    }

    /// Upright Dokan2D warp/out pose (0x25b9d0, 0x25ba50 and the
    /// upright branch of 0x25cd80): original local mouth offset 200.
    pub fn upright_2d_exit(&self) -> Option<(Vec3, Quat)> {
        (self.is_2d && self.direction == Direction::Up).then(|| {
            (
                self.local_offset(Vec3::new(0., 200., 0.)),
                quat_front_up(self.side(), self.up()),
            )
        })
    }
}

fn matrix_from_axes([side, up, front]: [Vec3; 3], p: Vec3) -> [f32; 12] {
    [
        side.x, up.x, front.x, p.x, side.y, up.y, front.y, p.y, side.z, up.z, front.z, p.z,
    ]
}

fn pose_matrix(q: Quat, p: Vec3) -> [f32; 12] {
    matrix_from_axes(quat_to_axes(q), p)
}

#[path = "../tests/dokan.rs"]
mod checks;
pub fn self_check() {
    checks::run();
}
