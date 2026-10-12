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
