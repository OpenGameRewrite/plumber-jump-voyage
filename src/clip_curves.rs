//! The original animation curves of the player's clips, evaluated between frames.
//!
//! The converter bakes every clip at whole frames into `Mario.glb`, but the game keeps the
//! curves themselves and samples them at the clip's fractional frame (the frame advances by the
//! animation rate, 0.5, 1.2, ... a step). `Mario.curves.bin` holds those curves exactly as the
//! archive stores them (docs/asset-format.md, "Animation curves"): per joint and clip, ten
//! channels (scale xyz, rotation xyzw, translation xyz), each a constant or a list of keys whose
//! cubic segment is `((c3 t + c2) t + c1) t + c0` for `t` going from 0 to 1 over the segment.
//! A rotation is three Euler angles in radians, applied X first, then Y, then Z (the archive's
//! Maya order), or a quaternion.
//!
//! Nothing here is required: without the file (or without a clip in it) the callers keep
//! sampling the baked tracks, linear between whole frames.
use std::collections::HashMap;
use std::sync::OnceLock;

/// The curve file next to the model.
const FILE: &str = "local/Mario.curves.bin";
const MAGIC: &[u8; 8] = b"PJVCURV1";
/// Scale xyz, rotation xyzw, translation xyz.
const CHANNELS: usize = 10;
/// A clip frame lasts 1/60 s: the converter's time base.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub const FRAMES_PER_SECOND: f32 = 60.;

/// One key: the frame it starts at and the coefficients of the cubic from there to the next key.
#[derive(Clone, Copy)]
struct Key {
    frame: f32,
    coefficients: [f32; 4],
}

enum Channel {
    Constant(f32),
    /// Keys in frame order; never empty.
    Keys(Vec<Key>),
}

impl Channel {
    /// The value at a clip frame: the cubic of the segment the frame is in, the first key's
    /// value before the first key and the last key's after the last one.
    fn at(&self, frame: f32) -> f32 {
        let keys = match self {
            Channel::Constant(value) => return *value,
            Channel::Keys(keys) => keys,
        };
        let last = keys.len() - 1;
        if frame <= keys[0].frame {
            return keys[0].coefficients[0];
        }
        if frame >= keys[last].frame {
            return keys[last].coefficients[0];
        }
        // The first key after the frame: the frame is in the segment that starts before it.
        let next = keys.partition_point(|key| key.frame <= frame);
        let (from, to) = (&keys[next - 1], &keys[next]);
        let t = (frame - from.frame) / (to.frame - from.frame);
        let [c0, c1, c2, c3] = from.coefficients;
        ((c3 * t + c2) * t + c1) * t + c0
    }
}

/// What a clip animates on one joint.
struct Record {
    quaternion: bool,
    channels: [Channel; CHANNELS],
}

/// A joint as the converter wrote it: its parent and the scale it has without a clip.
struct JointInfo {
    name: String,
    parent: Option<usize>,
    /// Segment scale compensation: the file stores the joint's scale over its parent's.
    compensated: bool,
    bind_scale: [f32; 3],
}

/// The curves of one clip, a record per joint that the clip animates.
pub struct Clip {
    records: Vec<Option<Record>>,
}

/// A joint's local pose in a clip at a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub translation: [f64; 3],
    /// Quaternion x, y, z, w.
    pub rotation: [f64; 4],
    /// The joint's own scale, whatever its parent's.
    pub scale: [f64; 3],
}

pub struct Curves {
    joints: Vec<JointInfo>,
    by_name: HashMap<String, usize>,
    clips: HashMap<String, Clip>,
}

/// Little-endian reads over the file's bytes; `None` when it ends early.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes(&mut self, count: usize) -> Option<&[u8]> {
        let (head, rest) = self.0.split_at_checked(count)?;
        self.0 = rest;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn text(&mut self) -> Option<String> {
        let length = usize::from(self.u16()?);
        String::from_utf8(self.bytes(length)?.to_vec()).ok()
    }
}

impl Curves {
    /// The installation's curves, read once; `None` without the file or when it is damaged.
    pub fn installed() -> Option<&'static Curves> {
        static CURVES: OnceLock<Option<Curves>> = OnceLock::new();
        CURVES
            .get_or_init(|| {
                let bytes = std::fs::read(crate::local_assets::asset_dir().join(FILE)).ok()?;
                Self::parse(&bytes)
            })
            .as_ref()
    }

    fn parse(bytes: &[u8]) -> Option<Curves> {
        let mut file = Reader(bytes);
        if file.bytes(MAGIC.len())? != MAGIC {
            return None;
        }
        let joint_count = file.u32()? as usize;
        let mut joints = Vec::new();
        let mut by_name = HashMap::new();
        for index in 0..joint_count {
            let name = file.text()?;
            let parent = usize::try_from(file.i32()?).ok().filter(|p| *p < index);
            let compensated = file.u8()? != 0;
            // The bind pose: translation, rotation, scale (only the scale is used).
            let mut bind = [0.; 10];
            for value in &mut bind {
                *value = file.f32()?;
            }
            by_name.entry(name.clone()).or_insert(index);
            joints.push(JointInfo {
                name,
                parent,
                compensated,
                bind_scale: [bind[7], bind[8], bind[9]],
            });
        }
        let clip_count = file.u32()? as usize;
        let mut clips = HashMap::new();
        for _ in 0..clip_count {
            let name = file.text()?;
            let _frames = file.f32()?;
            let record_count = file.u32()? as usize;
            let mut records: Vec<Option<Record>> = (0..joints.len()).map(|_| None).collect();
            for _ in 0..record_count {
                let joint = file.u32()? as usize;
                let quaternion = file.u8()? != 0;
                let mut channels = Vec::with_capacity(CHANNELS);
                for _ in 0..CHANNELS {
                    let key_count = file.u32()? as usize;
                    channels.push(if key_count == 0 {
                        Channel::Constant(file.f32()?)
                    } else {
                        let mut keys = Vec::with_capacity(key_count.min(4096));
                        for _ in 0..key_count {
                            let frame = file.f32()?;
                            let mut coefficients = [0.; 4];
                            for c in &mut coefficients {
                                *c = file.f32()?;
                            }
                            keys.push(Key {
                                frame,
                                coefficients,
                            });
                        }
                        Channel::Keys(keys)
                    });
                }
                *records.get_mut(joint)? = Some(Record {
                    quaternion,
                    channels: channels.try_into().ok()?,
                });
            }
            clips.insert(name, Clip { records });
        }
        Some(Curves {
            joints,
            by_name,
            clips,
        })
    }

    /// The curve file's index of a joint, by its name in the model.
    pub fn joint(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn joint_name(&self, joint: usize) -> Option<&str> {
        self.joints.get(joint).map(|j| j.name.as_str())
    }

    pub fn clip(&self, name: &str) -> Option<&Clip> {
        self.clips.get(name)
    }

    /// The joint's pose in the clip at a (fractional) frame; `None` when the clip does not
    /// animate it.
    pub fn sample(&self, clip: &Clip, joint: usize, frame: f32) -> Option<Sample> {
        let record = clip.records.get(joint)?.as_ref()?;
        let value = |channel: usize| f64::from(record.channels[channel].at(frame));
        let rotation = if record.quaternion {
            let q = [value(3), value(4), value(5), value(6)];
            let length = q.iter().map(|c| c * c).sum::<f64>().sqrt();
            q.map(|c| c / length)
        } else {
            euler_xyz_quaternion([value(3), value(4), value(5)])
        };
        Some(Sample {
            translation: [value(7), value(8), value(9)],
            rotation,
            scale: [value(0), value(1), value(2)],
        })
    }

    /// The joint's own scale in the clip: what it animates, or the scale it has without a clip.
    pub fn scale(&self, clip: &Clip, joint: usize, frame: f32) -> [f64; 3] {
        match self.sample(clip, joint, frame) {
            Some(sample) => sample.scale,
            None => self.joints[joint].bind_scale.map(f64::from),
        }
    }

    /// The pose as `Mario.glb` stores it: the same translation and rotation, and for a joint with
    /// segment scale compensation the scale over its parent's (a stretched arm must not inflate
    /// its hand, but glTF always inherits).
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn sample_for_file(&self, clip: &Clip, joint: usize, frame: f32) -> Option<Sample> {
        let mut sample = self.sample(clip, joint, frame)?;
        let info = &self.joints[joint];
        if let Some(parent) = info.parent.filter(|_| info.compensated) {
            let above = self.scale(clip, parent, frame);
            for (scale, above) in sample.scale.iter_mut().zip(above) {
                if above != 0. {
                    *scale /= above;
                }
            }
        }
        Some(sample)
    }
}

/// The quaternion of Euler angles that turn about X first, then Y, then Z (all fixed axes).
fn euler_xyz_quaternion([x, y, z]: [f64; 3]) -> [f64; 4] {
    let (sx, cx) = (x / 2.).sin_cos();
    let (sy, cy) = (y / 2.).sin_cos();
    let (sz, cz) = (z / 2.).sin_cos();
    // qz * qy * qx
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

#[path = "tests/clip_curves.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
