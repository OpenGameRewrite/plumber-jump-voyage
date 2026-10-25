# Local asset metadata

Local exports use standard glTF `extras` to describe rendering behavior and
animation identities. Geometry, textures and clips remain ordinary glTF data.
The game reads this metadata once when a scene loads and attaches typed Bevy
components. Display names can then change without changing rendering or animation
binding.

Every mesh primitive has `extras.pjv_part`, with `version: 1`:

| Field | Meaning |
| --- | --- |
| `role` | Visibility rule: `unchanged`, `hidden`, `visible`, `closed_left`, `closed_right`, `pupil_left`, `pupil_right`, `track`, `worn_cap`, or `uncapped_hair`. |
| `track` | Visibility-track identity for `track` and `worn_cap`; otherwise null. |
| `pupil` | Texture-animation track identity, or null. |
| `surface` | `ordinary`, `glass`, or `hair`. |
| `stain` | `none`, `body`, or `cap`. |
| `exported` | Whether the local model's material finishing rules apply. |
| `metal` | Whether the reflection-based metal material applies. |

Every named node has `extras.pjv_bone`, with `version: 1`. Its `key` is a pair of
stable source identities for the bone and its parent. These identities are kept
when display names change. `leg` controls the upper-body animation mask; `hip`
identifies the synchronization point used when binding the scene.

The exporter translates source naming conventions into these explicit fields.
It preserves existing metadata overrides and rejects ambiguous identities and
invalid hierarchies. The runtime rejects unsupported metadata versions and
unknown enum values instead of silently guessing their meaning.

Older local packs remain supported through a compatibility adapter that
interprets their original names once at scene binding. To upgrade existing
packs without extracting or converting their geometry again:

```powershell
python tools/annotate-assets.py assets/local
```

This changes only JSON metadata. Binary chunks containing geometry, textures
and animation samples are preserved. All generated game data stays local and
ignored by Git.

## Animation curves

`Mario.glb` holds every clip baked at whole frames, but the game evaluates the archive's
curves at the fractional frame of the animation. `assets/local/Mario.curves.bin` keeps those
curves for the clips `Mario.glb` holds; the game uses it when present (older builds ignore it)
and falls back to the baked tracks otherwise. Write it with `python tools/convert-outfits.py
--curves` (the converter's `--curves-only` mode); it changes nothing else. Little endian:

| Part | Content |
| --- | --- |
| Header | `PJVCURV1` |
| Joints | `u32` count; each: name (`u16` length, UTF-8), `i32` parent (or -1), `u8` segment scale compensation, bind pose `f32` translation[3], rotation[4] (x y z w), scale[3] |
| Clips | `u32` count; each: name, `f32` frames, `u32` records |
| Record | `u32` joint, `u8` rotation kind (0 Euler XYZ in radians, 1 quaternion), then 10 channels in the order scale xyz, rotation xyzw, translation xyz |
| Channel | `u32` keys; 0 is a constant (`f32` value), otherwise per key `f32` frame and `f32` c0, c1, c2, c3 |

Between a key and the next, `t` runs from 0 to 1 and the value is `((c3 t + c2) t + c1) t + c0`;
before the first key and after the last it is that key's value. Joints and clips the file does
not mention (the finger and face joints, which the converter poses from other archives) keep
their baked tracks.

