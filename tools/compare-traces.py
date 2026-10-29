"""Compare two per-frame traces (the real game's and the rewrite's) frame by frame.

    py tools/compare-traces.py game.csv rewrite.csv --out diff.md
        [--lockstep] [--tol column=value ...] [--ignore-events se,voice] [--strict]
        [--allow-contaminated] [--geometry export.json] [--json report.json]

Both files are CSV with a header row and one row per player update. Frames are
aligned on the `frame` column. A column that is empty in every row of one file
is reported as not comparable (neither pass nor fail); inside a column that
exists on both sides, an empty cell means "none" and must match "none".

Column kinds:
  numeric      absolute error against a tolerance (angles compare around 360)
  categorical  exact text; reported as a timeline of runs for each side
  set          order-insensitive items (events, buttons, live effects)
  keyed        per-slot items (animation layers, parts, bones, ...)
  key=value    counters, collision and triggers: every key is compared on its
               own and shown as `column.key`. A key only one side ever writes
               is not comparable (the rewrite has no ceiling flag, the game has
               no `coyote` counter), never a FAIL.

Environment: next to the game CSV, `game.env.json` / `game.meta.json` say whether
the real-game run saw only flat, empty ground. The rule in tools/environment_rule.py
judges every listed frame: eye overlaps, harmless actor overlaps and static map
geometry (replayed with --geometry) are accepted and listed in the report; any
other contamination refuses the run (exit status 3) unless --allow-contaminated is
given. Then the refused frames are left out of every column, and the position
columns are left out after the first of them, where drift from the contact is expected.

Probe-only columns (game_frame, tick, cam_real_*, late_sample, other_events,
anim_slots, floor_y, cap_step, cap_hidden) and event kinds that only log the
probe's own animator calls are ignored. The cap columns are compared only on
frames where both sides have the cap out; `cap_out` reports who has it out.

Use --lockstep when the rewrite was run with `--lockstep game.csv`: every frame
then starts from the game's previous state, so the report also counts how many
frames disagree and ranks the worst ones rather than looking for drift.
"""
import argparse
import csv
import json
import math
import os
import re
import sys
from collections import Counter, OrderedDict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from environment_rule import judge_run, ranges  # noqa: E402
from environment_rule import self_check as environment_self_check  # noqa: E402
from reaction_names import english as reaction_name  # noqa: E402

# Default numeric tolerances, in the trace's units (cm, cm/frame, degrees, ...).
NUMERIC = OrderedDict(
    [
        ("px", 0.5), ("py", 0.5), ("pz", 0.5),
        ("vx", 0.05), ("vy", 0.05), ("vz", 0.05),
        ("speed_h", 0.05),
        ("fx", 0.01), ("fz", 0.01),
        ("qx", 0.01), ("qy", 0.01), ("qz", 0.01), ("qw", 0.01),
        ("ux", 0.01), ("uy", 0.01), ("uz", 0.01),
        ("anim_frame", 0.05), ("anim_rate", 0.05), ("anim_len", 0.5),
        ("sub_frame", 0.05),
        ("state_step", 0), ("sub_step", 0),
        ("cam_yaw", 0.5), ("cam_pitch", 0.5),
        ("cam_px", 0.5), ("cam_py", 0.5), ("cam_pz", 0.5), ("cam_fov", 0.5),
    ]
)
ANGLES = {"cam_yaw", "cam_pitch"}
CATEGORICAL = [
    "ground", "state", "state_nerve", "anim", "anim_loop", "sub_anim",
    "cam_mode", "face", "model",
]
SETS = OrderedDict([("events", ";"), ("effects_live", "|"), ("in_buttons", "+")])
# Per-slot columns: name -> tolerance of the numbers inside.
KEYED = OrderedDict(
    [
        ("blend", 0.02), ("interp", 0.02), ("anim_layers", 0.02), ("parts", 0.05),
        ("random", 0),
    ]
)
# The skeleton: every joint's position (cm) in the `bones` cell, compared as one column. A joint is
# off when its position is more than this far (straight-line distance) from the game's; a frame
# fails when any joint is. The animation, the ground pose and the head's look-at all end up here,
# and where they agree the joints are within a few millimetres (the ground pose is bit-exact
# against the executable's own code), so a couple of centimetres leaves room only for the clips'
# own sampling and the position's float rounding.
BONES_TOLERANCE = 2.0
# `key=value` columns: name -> (item separator, tolerance of the numbers inside).
KEY_VALUE = OrderedDict([
    ("counters", ("|", 0)), ("collision", (";", 0.01)), ("triggers", (";", 0)),
    # The head look-at: the keeper's state and the sensors it picked (host/sensor/type).
    ("look_keeper", (";", 0)), ("look_sensors", (";", 0)),
])
VECTORS = OrderedDict([
    ("in_stick", 0.01), ("move_input", 0.01), ("floor_normal", 0.01),
    # The point the head was asked to look at (the sensor's position in the stage, cm).
    ("look_target", 0.05),
])
# Rotations as x/y/z/w quaternions: the angle between the two, in degrees (the sign of a
# quaternion does not matter). `head_q` is the Head joint's world rotation after the look-at.
ROTATIONS = OrderedDict([("head_q", 1.0)])
# head_q is compared only where the rewrite's head is where the game's is. The rewrite poses the
# skeleton with the ground pose (the lean, the sway of a landing, the planted legs) and the
# look-at; on the ground the head then agrees to within a few millimetres, so the gate only
# leaves out the states whose joint controllers are not ported (the swim pose, the cap throw,
# the pole climb and the grab), where the game's head is somewhere else by more than the
# look-at turns it. The head joint's position does not depend on the look-at, so it tells
# which frames are comparable.
HEAD_POSITION_GATE = 1.0  # cm, rewrite `head_pos` against the game's `bones` Head
# Outside the look-at the head's rotation is the skeleton's animation alone, which this column is not
# about: head_q is compared where the game asked for a look (`requested=1` in look_keeper) or the
# rewrite's head is turned by more than this many degrees (`head_turn`, which also covers the
# head returning to its animation).
HEAD_TURN_ACTIVE = 0.5
# The cap columns, compared only while both sides have the cap out.
CAP_NUMERIC = OrderedDict([("cap_px", 0.5), ("cap_py", 0.5), ("cap_pz", 0.5)])
CAP_CATEGORICAL = ["cap_state", "cap_anim"]
# Columns whose cells the rewrite fills only on some rows (the length of a clip
# it has no data for stays empty): compared where both sides have a value.
PARTIAL = {"anim_len"}
# Columns that are never comparable, with the reason.
NEVER = {
    "pre_input": "the game logs the raw words of its pre-input judges, the rewrite its own pending counts",
}
# Keys of a key=value column that are never comparable, with the reason.
# The look_sensors keys that name another actor's sensor (`host/sensor/type`).
LOOK_HOST_KEYS = ("candidate", "target")
NEVER_KEYS = {
    "triggers": "raw trigger flag words of the game's player trigger object have no defined mapping",
}
# Counters that read the same whatever the timer says while the count is zero.
ZERO_COUNT_TIMER = {"cjump", "clong"}
# Event kinds that only log the probe's own diagnostics of animator/effect calls.
PROBE_EVENT_KINDS = {"anim", "parts", "subanim", "upper", "endsub", "effect_stop", "contact"}
# Probe-only columns the rewrite has no counterpart for.
PROBE_ONLY = [
    "game_frame", "tick", "cam_real_yaw", "cam_real_pitch", "late_sample", "other_events",
    "anim_slots", "floor_y", "cap_step", "cap_hidden", "contacts", "hit_hosts",
    "fx_sets", "fx_kills",
    # Bit-exact copies of the state and of every calcMoveInput call (the rewrite writes no equivalents;
    # the replay reads bits_cam_view, the oracle tests read the rest).
    "bits_stick", "bits_move", "bits_cam_view", "bits_pos", "bits_vel", "bits_front", "bits_up", "bits_quat",
]
# Position columns: a contact changes them for good, so they are left out after it.
DRIFT = {"px", "py", "pz", "cap_px", "cap_py", "cap_pz"}

GROUPS = OrderedDict(
    [
        ("movement", {"px", "py", "pz", "vx", "vy", "vz", "speed_h", "fx", "fz", "qx", "qy", "qz", "qw",
                      "ux", "uy", "uz", "ground", "floor_normal", "collision"}),
        ("state", {"state", "state_nerve", "state_step", "sub_step", "counters", "triggers", "pre_input"}),
        ("animation", {"anim", "anim_frame", "anim_rate", "anim_len", "anim_loop", "blend", "interp",
                       "anim_layers", "sub_anim", "sub_frame", "parts", "face", "model"}),
        ("skeleton", {"bones"}),
        ("cap", {"cap_state", "cap_anim", "cap_px", "cap_py", "cap_pz", "cap_out"}),
        ("events/sound", {"events", "sounds"}),
        ("effects", {"effects_live"}),
        ("input", {"in_stick", "in_buttons", "move_input", "cam_yaw"}),
        ("camera", {"cam_pitch", "cam_px", "cam_py", "cam_pz", "cam_fov", "cam_mode"}),
        ("look-at", {"head_q", "look_target", "look_keeper", "look_sensors"}),
        ("other", {"random"}),
    ]
)


def group_of(column):
    base = column.split(".")[0]
    for group, members in GROUPS.items():
        if base in members:
            return group
    return "other"


def read_trace(path):
    with open(path, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    if not rows:
        sys.exit(f"{path}: no rows")
    if "frame" not in rows[0]:
        sys.exit(f"{path}: no 'frame' column")
    by_frame = OrderedDict()
    for row in rows:
        try:
            by_frame[int(row["frame"])] = {k: (v or "").strip() for k, v in row.items() if k}
        except ValueError:
            sys.exit(f"{path}: bad frame value {row['frame']!r}")
    columns = list(rows[0].keys())
    if "state" in columns:
        # The nerve without its sub-state, so a naming difference in the
        # sub-state does not hide whether the main state agrees.
        columns.append("state_nerve")
        for row in by_frame.values():
            row["state_nerve"] = row["state"].split("/")[0]
    # Whether the cap is out, for comparing the cap columns only while it is.
    columns.append("cap_out")
    for row in by_frame.values():
        row["cap_out"] = "1" if row.get("cap_state") else "0"
    for row in by_frame.values():
        # The game appends the clip's frame to the cap clip (`StayR:3f800000`).
        if "cap_anim" in row:
            row["cap_anim"] = row["cap_anim"].split(":")[0]
    return columns, by_frame


# The probe flags Cappy's `Trample`/`TrampleReturn` nerves as "hit something". Those
# nerves are the player standing on his own thrown cap (the cap jump), a contact
# between the player and his own cap, not a foreign object: the run stays
# comparable. Every other actor still shows up through its own reasons (cap
# `Rebound`/`LockOn`/`Hack`, `hit-sensor contact <actor>`, wall and floor changes).
OWN_CAP_REASON = re.compile(r"^Cappy (Trample|TrampleReturn) \(hit/captured something\)$")


def read_environment(game_path):
    """What the probe recorded about the real run's surroundings.

    Returns (clean, contaminated frames {frame: [reasons]}, notes). A run with
    no environment files next to it is of unknown cleanliness, which is noted
    and not refused (older runs have none). A frame whose only reasons are the
    player's own cap being trampled is clean (`OWN_CAP_REASON`).
    """
    folder = os.path.dirname(os.path.abspath(game_path))
    clean, frames, notes, found = True, OrderedDict(), [], False
    env_path = os.path.join(folder, "game.env.json")
    meta_path = os.path.join(folder, "game.meta.json")
    flagged, listed = False, 0
    if os.path.exists(env_path):
        found = True
        with open(env_path, encoding="utf-8") as f:
            env = json.load(f)
        flagged = env.get("clean") is False
        for item in env.get("contaminated", []):
            listed += 1
            reasons = [r for r in item.get("reasons", []) if not OWN_CAP_REASON.match(r)]
            if reasons or not item.get("reasons"):
                frames[int(item["frame"])] = reasons
    if os.path.exists(meta_path):
        found = True
        with open(meta_path, encoding="utf-8") as f:
            meta = json.load(f)
        flagged = flagged or meta.get("env_clean") is False
        count = meta.get("env_contaminated_frames")
        if count and not listed:
            notes.append(f"game.meta.json counts {count} contaminated frames but game.env.json lists none")
    # Flagged by the probe and no frame left once the player's own cap is
    # discounted: clean. Flagged with no frame listed at all stays unclean.
    if flagged and (frames or not listed):
        clean = False
    if not found:
        notes.append("environment unknown: no game.env.json or game.meta.json next to the game trace")
    return clean, frames, notes, found


class EnvironmentRefused(Exception):
    """The real-game run is contaminated in a way the environment rule does not accept."""

    def __init__(self, summary, banner):
        super().__init__(banner)
        self.summary = summary
        self.banner = banner


def as_float(text):
    try:
        return float(text)
    except ValueError:
        return None


def angle_error(a, b):
    d = abs(a - b) % 360.0
    return min(d, 360.0 - d)


def split_items(text, sep):
    return [i for i in text.split(sep) if i != ""] if text else []


def keyed_items(text, sep="|"):
    """Items keyed by their name; repeated names get a #n suffix to stay distinct."""
    seen, out = Counter(), OrderedDict()
    for item in split_items(text, sep):
        key = item
        for mark in ("@", "=", ":", "/"):
            if mark in item:
                key = item.split(mark, 1)[0]
                break
        seen[key] += 1
        out[key if seen[key] == 1 else f"{key}#{seen[key]}"] = item
    return out


def key_values(text, sep):
    """`key=value` items of a cell; an item without `=` is a key with no value."""
    out = OrderedDict()
    for item in split_items(text, sep):
        key, _, value = item.partition("=")
        out[key] = value
    return out


NUMBER = re.compile(r"-?\d+(?:\.\d+)?")


def numbers_in(item):
    """The numbers in an item, and the item's text with each number replaced by #."""
    return [float(n) for n in NUMBER.findall(item)], NUMBER.sub("#", item)


class Result:
    def __init__(self, column, kind, tol=None):
        self.column = column
        self.group = group_of(column)
        self.kind = kind
        self.tol = tol
        self.status = "PASS"
        self.errors = []  # one worst error per compared frame (numeric kinds)
        self.bad = []  # (frame, [problem text])
        self.game = {}
        self.rewrite = {}
        self.note = ""
        self.compared = 0

    def not_comparable(self, why):
        self.status = "N/C"
        self.note = why

    def add(self, frame, err, problems):
        self.compared += 1
        if err is not None:
            self.errors.append((frame, err))
        if problems:
            self.bad.append((frame, problems))

    def finish(self):
        if self.status == "N/C":
            return
        self.status = "FAIL" if self.bad else "PASS"

    def max_err(self):
        return max((e for _, e in self.errors), default=None)

    def mean_err(self):
        return sum(e for _, e in self.errors) / len(self.errors) if self.errors else None

    def first_bad(self):
        return self.bad[0][0] if self.bad else None

    def first_detail(self):
        return "; ".join(self.bad[0][1])[:200] if self.bad else ""

    def as_dict(self):
        return {
            "column": self.column, "group": self.group, "kind": self.kind, "status": self.status,
            "note": self.note, "tolerance": self.tol, "compared_frames": self.compared,
            "bad_frames": len(self.bad), "first_bad_frame": self.first_bad(),
            "first_detail": self.first_detail(), "max_error": self.max_err(),
            "mean_error": self.mean_err(),
        }


def compare_numeric(frame, g, r, tol, angle, mod=None):
    if g == "" and r == "":
        return None, []
    gv, rv = as_float(g), as_float(r)
    if gv is None or rv is None:
        return None, [f"{g or 'none'} vs {r or 'none'}"]
    if angle:
        err = angle_error(gv, rv)
    elif mod:
        d = abs(gv - rv) % mod
        err = min(d, mod - d)
    else:
        err = abs(gv - rv)
    return err, ([f"{g} vs {r} (error {err:.4g})"] if err > tol else [])


def compare_value(g, r, tol):
    """Two values of one key: the same text around numbers that agree within tol."""
    gn, gt = numbers_in(g)
    rn, rt = numbers_in(r)
    if gt != rt or len(gn) != len(rn):
        return None, False
    worst = max((abs(a - b) for a, b in zip(gn, rn)), default=None)
    return worst, worst is None or worst <= tol


def compare_keyed(g, r, tol):
    gi, ri = keyed_items(g), keyed_items(r)
    problems, worst = [], None
    for key in gi.keys() - ri.keys():
        problems.append(f"{key}: only in game ({gi[key]})")
    for key in ri.keys() - gi.keys():
        problems.append(f"{key}: only in rewrite ({ri[key]})")
    for key in gi.keys() & ri.keys():
        err, ok = compare_value(gi[key], ri[key], tol)
        if err is not None:
            worst = err if worst is None else max(worst, err)
        if not ok:
            problems.append(f"{key}: {gi[key]} vs {ri[key]}")
    return worst, problems


def compare_bones(g, r, tol):
    """The joints both sides log, by straight-line distance; the worst distance and the joints
    that are further than `tol` apart. A joint only one side has (an older probe did not log
    `nw4f_root` and `Mario`) is left out, but a side with no joint at all is a mismatch."""
    gp = {k: bone_position(g, k) for k in joint_names(g)}
    rp = {k: bone_position(r, k) for k in joint_names(r)}
    shared = [k for k in gp if k in rp and gp[k] is not None and rp[k] is not None]
    if not shared:
        return None, [f"no joint in common ({g[:60] or 'none'} vs {r[:60] or 'none'})"]
    worst, problems = 0.0, []
    for key in shared:
        err = math.dist(gp[key], rp[key])
        worst = max(worst, err)
        if err > tol:
            problems.append(f"{key}: {err:.1f} cm ({'/'.join(f'{v:g}' for v in gp[key])} vs "
                            f"{'/'.join(f'{v:g}' for v in rp[key])})")
    return worst, problems


def joint_names(bones):
    return [item.partition(":")[0] for item in split_items(bones, "|")]


def quaternion_angle(g, r):
    """Degrees between two x/y/z/w quaternions (None when a cell is not one)."""
    gv = [as_float(x) for x in g.split("/")] if g else []
    rv = [as_float(x) for x in r.split("/")] if r else []
    if len(gv) != 4 or len(rv) != 4 or None in gv + rv:
        return None
    dot = sum(a * b for a, b in zip(gv, rv))
    norm = (sum(a * a for a in gv) * sum(b * b for b in rv)) ** 0.5
    if norm == 0:
        return None
    return math.degrees(2 * math.acos(min(1.0, abs(dot) / norm)))


def compare_rotation(g, r, tol):
    if g == "" and r == "":
        return None, []
    err = quaternion_angle(g, r)
    if err is None:
        return None, [f"{g or 'none'} vs {r or 'none'}"]
    return err, ([f"{g} vs {r} (error {err:.4g} degrees)"] if err > tol else [])


def bone_position(bones, name):
    """The x/y/z of one joint in a `bones` cell (`Joint:x/y/z|...`), or None."""
    for item in split_items(bones, "|"):
        key, _, value = item.partition(":")
        if key == name:
            parts = [as_float(v) for v in value.split("/")]
            if len(parts) == 3 and None not in parts:
                return parts
    return None


def compare_vector(g, r, tol):
    if g == "" and r == "":
        return None, []
    gv = [as_float(x) for x in g.split("/")] if g else []
    rv = [as_float(x) for x in r.split("/")] if r else []
    if len(gv) != len(rv) or None in gv + rv:
        return None, [f"{g or 'none'} vs {r or 'none'}"]
    err = max((abs(a - b) for a, b in zip(gv, rv)), default=0.0)
    return err, ([f"{g} vs {r}"] if err > tol else [])


def event_item(item):
    """An event as both sides compare it: the game logs a hit reaction by its
    Japanese name (`reaction:着地`), the rewrite by its English one."""
    kind, _, name = item.partition(":")
    if kind != "reaction":
        return item
    # The game marks a reaction raised on another actor with `@owner`.
    base, at, owner = name.partition("@")
    return f"{kind}:{reaction_name(base)}{at}{owner}"


def compare_set(g, r, sep, ignore):
    keep = lambda text: Counter(
        event_item(i) for i in split_items(text, sep) if i.split(":", 1)[0] not in ignore
    )
    gc, rc = keep(g), keep(r)
    problems = []
    only_g = sorted((gc - rc).elements())
    only_r = sorted((rc - gc).elements())
    if only_g:
        problems.append("game only: " + ", ".join(only_g))
    if only_r:
        problems.append("rewrite only: " + ", ".join(only_r))
    return None, problems


def sound_names(text):
    """Sound names of a `name/gain/pitch|...` cell. The game logs the request's
    parameter where the rewrite logs the mixed gain and pitch, so only the names
    are comparable."""
    return Counter(i.split("/", 1)[0] for i in split_items(text, "|"))


def compare_sounds(g, r):
    gc, rc = sound_names(g), sound_names(r)
    problems = []
    if gc - rc:
        problems.append("game only: " + ", ".join(sorted((gc - rc).elements())))
    if rc - gc:
        problems.append("rewrite only: " + ", ".join(sorted((rc - gc).elements())))
    return None, problems


def normalise_counter(key, value):
    if key in ZERO_COUNT_TIMER:
        m = re.match(r"(-?\d+)/(-?\d+)$", value)
        if m and int(m.group(1)) == 0:
            return "0/-"
    return value


def runs(values):
    out = []
    for frame, v in values:
        if out and out[-1][0] == v and out[-1][2] == frame - 1:
            out[-1][2] = frame
        else:
            out.append([v, frame, frame])
    return out


def timeline(values, limit=60):
    items = [f"{v if v != '' else '-'} {a}" + (f"-{b}" if b != a else "") for v, a, b in runs(values)]
    if len(items) > limit:
        items = items[:limit] + [f"... ({len(items) - limit} more runs)"]
    return " | ".join(items)


class Report:
    """Everything a comparison found."""

    def __init__(self):
        self.game_path = self.rewrite_path = ""
        self.frames = []
        self.notes = []
        self.results = []
        self.env_clean = True
        self.env_known = False
        self.excluded = []  # contaminated frames left out
        self.first_contaminated = None
        self.env_lines = []  # what the environment rule accepted and refused, one line each
        self.env_accepted_frames = 0  # frames whose listed contamination was judged harmless
        self.lockstep = False
        self.ignored_probe_columns = []

    def failing(self):
        return [r for r in self.results if r.status == "FAIL"]

    def passing(self):
        return [r for r in self.results if r.status == "PASS"]

    def not_comparable(self):
        return [r for r in self.results if r.status == "N/C"]

    def verdict(self):
        return "FAIL" if self.failing() else "PASS"

    def result(self, column):
        return next((r for r in self.results if r.column == column), None)

    def as_dict(self):
        return {
            "game": self.game_path, "rewrite": self.rewrite_path, "verdict": self.verdict(),
            "frames_compared": len(self.frames), "excluded_frames": self.excluded,
            "env_clean": self.env_clean, "env_known": self.env_known, "env_rule": self.env_lines, "notes": self.notes,
            "columns": [r.as_dict() for r in self.results],
        }


def exported_look_hosts(game_path, geometry_path):
    """Hosts of the look-at sensors the replay's stage export carries, or None without a matching export.

    A replay knows other actors only through that export (static placements). The game's holder may
    also pick a host the export cannot place: an NPC that patrols, a pipe's bind sensor. Which one it
    picks there is the other actor's business, not Mario's.
    """
    if not geometry_path:
        return None
    with open(geometry_path, encoding="utf-8") as f:
        geometry = json.load(f)
    meta_path = os.path.join(os.path.dirname(game_path), "game.meta.json")
    if not os.path.isfile(meta_path):
        return None
    with open(meta_path, encoding="utf-8") as f:
        meta = json.load(f)
    if geometry.get("stage") != meta.get("stage") or geometry.get("scenario") != meta.get("scenario_no"):
        return None
    return {sensor["host"] for sensor in geometry.get("look_sensors", [])} | {source["host"] for source in geometry.get("rocket_flowers", [])}


def sensor_host(label):
    """`host/sensor/type` -> host."""
    return label.rsplit("/", 2)[0]


def compare(game_path, rewrite_path, tol_overrides=None, ignore_events=(), lockstep=False,
            allow_contaminated=False, geometry_path=None):
    """Compare two trace files. Raises EnvironmentRefused for a contaminated run unless allowed."""
    report = Report()
    report.game_path, report.rewrite_path, report.lockstep = game_path, rewrite_path, lockstep
    tolerances = dict(NUMERIC)
    tolerances.update(CAP_NUMERIC)
    tolerances.update(KEYED)
    tolerances["bones"] = BONES_TOLERANCE
    tolerances.update({k: v[1] for k, v in KEY_VALUE.items()})
    tolerances.update(VECTORS)
    tolerances.update(ROTATIONS)
    tolerances.update(tol_overrides or {})
    ignore = set(PROBE_EVENT_KINDS) | set(ignore_events)

    clean, listed, env_notes, known = read_environment(game_path)
    report.env_known = known
    report.notes.extend(env_notes)
    gcols, game = read_trace(game_path)
    rcols, rew = read_trace(rewrite_path)
    frames = [f for f in game if f in rew]
    if not frames:
        sys.exit("no frame numbers in common")

    # The environment rule: contamination the probe listed is judged frame by frame; a log that names
    # its contacts is judged even when nothing was listed (a handled message is contamination too).
    contaminated = OrderedDict()  # refused frames -> reasons
    verdict = None
    flagged_without_frames = not clean and not listed
    if (not clean and listed) or any(r.get("hit_hosts") or r.get("contacts") for r in game.values()):
        evidence = {}
        if geometry_path:
            with open(geometry_path, encoding="utf-8") as f:
                geometry = json.load(f)
            meta_path = os.path.join(os.path.dirname(game_path), "game.meta.json")
            with open(meta_path, encoding="utf-8") as f:
                meta = json.load(f)
            # A different stage/scenario cannot establish the cause of this trace.
            if geometry.get("stage") == meta.get("stage") and geometry.get("scenario") == meta.get("scenario_no"):
                pre = os.path.join(os.path.dirname(game_path), "game.preroll.csv")
                evidence = dict(geometry=geometry, start=meta.get("start_abs"),
                                preroll=read_trace(pre)[1] if os.path.isfile(pre) else {},
                                start_triangle=meta.get("floor_triangle_at_start"))
                actor_path = os.path.join(os.path.dirname(game_path), "game.geysers.csv")
                seed_path = os.path.join(os.path.dirname(game_path), "game.geyser_seed.json")
                if os.path.isfile(actor_path) and os.path.isfile(seed_path):
                    with open(actor_path, encoding="utf8") as actor_file:
                        evidence["geysers"] = list(csv.DictReader(actor_file))
                    message_path = os.path.join(os.path.dirname(game_path), "game.geyser_messages.csv")
                    if os.path.isfile(message_path):
                        with open(message_path, encoding="utf8") as message_file:
                            evidence["geyser_messages"] = list(csv.DictReader(message_file))
                    with open(seed_path, encoding="utf8") as seed_file:
                        evidence["geyser_seed"] = json.load(seed_file)
                for key, suffix in (("press_parts", "press_parts.csv"), ("press_mario", "press_mario.csv"), ("grabceil_bars", "grabceil_bars.csv"), ("grabceil_messages", "grabceil_messages.csv"), ("rocket_flowers", "rocket_flowers.csv"), ("rocket_watcher", "rocket_watcher.csv"), ("ceiling_check", "ceiling_check.csv")):
                    side = os.path.join(os.path.dirname(game_path), "game." + suffix)
                    if os.path.isfile(side):
                        with open(side, encoding="utf8") as source:
                            evidence[key] = list(csv.DictReader(source))
                side = os.path.join(os.path.dirname(game_path), "game.grabceil_seed.json")
                if os.path.isfile(side):
                    with open(side, encoding="utf8") as source:
                        evidence["grabceil_seed"] = json.load(source)
                side = os.path.join(os.path.dirname(game_path), "game.rocket_seed.json")
                if os.path.isfile(side):
                    with open(side, encoding="utf8") as source:
                        evidence["rocket_seed"] = json.load(source)
        verdict = judge_run(game, rew, listed, **evidence)
        for f, reasons in listed.items():
            if not reasons:
                verdict.refuse(f, "listed by the probe without a reason")
        in_range = set(frames)
        contaminated = OrderedDict((f, r) for f, r in sorted(verdict.refused.items()) if f in in_range)
        report.env_lines = verdict.lines()
        report.env_accepted_frames = len({f for e in verdict.accepted.values() for f in e["frames"]})
        for f in sorted(set(verdict.refused) - in_range):
            report.notes.append(f"contamination on frame {f}, outside the frames both traces have, is not needed")
        clean = not contaminated
    report.env_clean = clean and not flagged_without_frames
    if not report.env_clean and not allow_contaminated:
        n = len(contaminated)
        reasons = sorted({r for rs in contaminated.values() for r in rs})
        summary = (f"CONTAMINATED ({n} frames; {'; '.join(reasons)[:120]})" if n
                   else "CONTAMINATED (flagged unclean, no frame listed)")
        banner = (
            "REFUSED: the real-game run is contaminated beyond what the environment rule accepts "
            f"({n} frames)." + "\n".join("  " + line for line in [""] + [l for l in report.env_lines if l.startswith("REFUSED")])
            + "\nA diff against it would show geometry or another actor, not behaviour. Re-place the scenario "
            "on clean ground, or pass --allow-contaminated to compare only its clean frames."
        )
        raise EnvironmentRefused(summary, banner)
    if contaminated:
        first = min(contaminated)
        report.first_contaminated = first
        report.notes.append(
            f"CONTAMINATED ENVIRONMENT: {len(contaminated)} frames left out of every column "
            f"(first {first}); the position columns are also left out after frame {first}"
        )
    elif not report.env_clean:
        report.notes.append("CONTAMINATED ENVIRONMENT: the run is flagged unclean but lists no frames")
    if verdict is not None and report.env_accepted_frames:
        report.notes.append(
            f"ENVIRONMENT RULE: {report.env_accepted_frames} frames with listed contamination were accepted "
            "as harmless (see the Environment section)"
        )

    report.frames = frames
    report.excluded = sorted(f for f in contaminated if f in rew and f in game)
    for label, trace in (("game", game), ("rewrite", rew)):
        gaps = [f for f in trace if f - 1 not in trace and f != min(trace)]
        if gaps:
            report.notes.append(f"{label} trace skips frames before {gaps[:5]}")
    counter = [int(r["game_frame"]) for r in game.values() if r.get("game_frame", "").lstrip("-").isdigit()]
    steps = {b - a for a, b in zip(counter, counter[1:])}
    if steps - {1}:
        report.notes.append(f"game game_frame steps are not all 1: {sorted(steps)[:6]}")
    only_g, only_r = len(game) - len(frames), len(rew) - len(frames)
    if only_g or only_r:
        report.notes.append(f"{only_g} game frames and {only_r} rewrite frames have no partner")
    report.ignored_probe_columns = [c for c in PROBE_ONLY if c in gcols]

    skipped = set(report.excluded)

    def frames_for(name):
        """The frames a column is compared on."""
        base = name.split(".")[0]
        keep = [f for f in frames if f not in skipped]
        if base in DRIFT and report.first_contaminated is not None:
            keep = [f for f in keep if f < report.first_contaminated]
        return keep

    def new_result(name, kind, gvals=None, rvals=None):
        res = Result(name, kind, tolerances.get(name))
        use = frames_for(name)
        res.game = {f: game[f].get(name, "") for f in use} if gvals is None else gvals
        res.rewrite = {f: rew[f].get(name, "") for f in use} if rvals is None else rvals
        results.append(res)
        return res, use

    results = report.results

    def prepare(name, kind):
        """A result for a whole column, not comparable when a side never fills it."""
        if name not in gcols and name not in rcols:
            return None, []
        res, use = new_result(name, kind)
        if name in NEVER:
            res.not_comparable(NEVER[name])
        else:
            gempty = not any(res.game.values())
            rempty = not any(res.rewrite.values())
            if gempty or rempty:
                res.not_comparable(
                    "empty in " + ("both" if gempty and rempty else "game" if gempty else "rewrite")
                )
        return res, use

    for name in NUMERIC:
        res, use = prepare(name, "numeric")
        if res is None or res.status == "N/C":
            continue
        for f in use:
            if name in PARTIAL and (res.game[f] == "" or res.rewrite[f] == ""):
                continue
            mod = None
            if name == "anim_frame":
                # Looping clips wrap at their length; compare around the circle.
                length = as_float(game[f].get("anim_len", "")) or 0
                if length > 0 and (game[f].get("anim_loop") == "1" or rew[f].get("anim_loop") == "1"):
                    mod = length
            err, problems = compare_numeric(f, res.game[f], res.rewrite[f], tolerances[name], name in ANGLES, mod)
            res.add(f, err, problems)
    for name in CATEGORICAL:
        res, use = prepare(name, "categorical")
        if res is None or res.status == "N/C":
            continue
        for f in use:
            res.add(f, None, [] if res.game[f] == res.rewrite[f]
                    else [f"{res.game[f] or 'none'} vs {res.rewrite[f] or 'none'}"])
    for name, sep in SETS.items():
        res, use = prepare(name, "set")
        if res is None or res.status == "N/C":
            continue
        for f in use:
            _, problems = compare_set(res.game[f], res.rewrite[f], sep, ignore if name == "events" else set())
            res.add(f, None, problems)
    for name in KEYED:
        res, use = prepare(name, "keyed")
        if res is None or res.status == "N/C":
            continue
        for f in use:
            err, problems = compare_keyed(res.game[f], res.rewrite[f], tolerances[name])
            res.add(f, err, problems)
    res, use = prepare("bones", "bones")
    if res is not None and res.status != "N/C":
        for f in use:
            err, problems = compare_bones(res.game[f], res.rewrite[f], tolerances["bones"])
            res.add(f, err, problems)
    if "sounds" in gcols or "sounds" in rcols:
        res, use = prepare("sounds", "names")
        if res.status != "N/C":
            for f in use:
                _, problems = compare_sounds(res.game[f], res.rewrite[f])
                res.add(f, None, problems)
    for name in VECTORS:
        res, use = prepare(name, "vector")
        if res is None or res.status == "N/C":
            continue
        for f in use:
            err, problems = compare_vector(res.game[f], res.rewrite[f], tolerances[name])
            res.add(f, err, problems)

    # The head's rotation, where the animated head position agrees (see HEAD_POSITION_GATE).
    for name in ROTATIONS:
        res, use = prepare(name, "rotation")
        if res is None or res.status == "N/C":
            continue
        left_out = 0
        for f in use:
            if res.game[f] == "" and res.rewrite[f] == "":
                continue
            asked = "requested=1" in game[f].get("look_keeper", "").split(";")
            turned = (as_float(rew[f].get("head_turn", "")) or 0.0) > HEAD_TURN_ACTIVE
            if not (asked or turned):
                continue
            game_head = bone_position(game[f].get("bones", ""), "Head")
            own = [as_float(v) for v in rew[f].get("head_pos", "").split("/")]
            if game_head is None or len(own) != 3 or None in own:
                left_out += 1
                continue
            if max(abs(a - b) for a, b in zip(game_head, own)) > HEAD_POSITION_GATE:
                left_out += 1
                continue
            err, problems = compare_rotation(res.game[f], res.rewrite[f], tolerances[name])
            res.add(f, err, problems)
        res.note = (f"{left_out} frames of the look-at left out: the animated head position differs "
                    f"from the game's by more than {HEAD_POSITION_GATE:g} cm there; frames with no look-at "
                    "on either side are not compared (the head is then the skeleton's animation alone)")
        if res.compared == 0:
            res.not_comparable(res.note)
        elif left_out:
            report.notes.append(f"{name}: {res.note}")

    look_hosts = exported_look_hosts(game_path, geometry_path)
    # key=value columns: every key is its own result.
    for name, (sep, _) in KEY_VALUE.items():
        if name not in gcols and name not in rcols:
            continue
        use = frames_for(name)
        gitems = {f: key_values(game[f].get(name, ""), sep) for f in use}
        ritems = {f: key_values(rew[f].get(name, ""), sep) for f in use}
        keys = OrderedDict()
        for items in (gitems, ritems):
            for per_frame in items.values():
                for key in per_frame:
                    keys.setdefault(key, None)
        if not keys:
            res = Result(name, "key=value", tolerances.get(name))
            res.not_comparable("empty in both")
            results.append(res)
            continue
        for key in keys:
            res = Result(f"{name}.{key}", "key=value", tolerances.get(name))
            results.append(res)
            in_g = any(key in items for items in gitems.values())
            in_r = any(key in items for items in ritems.values())
            res.game = {f: gitems[f].get(key, "") for f in use}
            res.rewrite = {f: ritems[f].get(key, "") for f in use}
            if name in NEVER_KEYS and in_g:
                res.not_comparable(NEVER_KEYS[name])
                continue
            if not in_g or not in_r:
                res.not_comparable("only in " + ("rewrite" if in_r else "game"))
                continue
            unplaced = 0
            for f in use:
                g, r = gitems[f].get(key), ritems[f].get(key)
                if g is None and r is None:
                    continue
                if (name == "look_sensors" and key in LOOK_HOST_KEYS and look_hosts is not None
                        and g is not None and sensor_host(g) not in look_hosts):
                    unplaced += 1
                    continue
                if g is None or r is None:
                    res.game[f] = "absent" if g is None else g
                    res.rewrite[f] = "absent" if r is None else r
                    res.add(f, None, [f"{res.game[f]} vs {res.rewrite[f]}"])
                    continue
                if name == "counters":
                    g, r = normalise_counter(key, g), normalise_counter(key, r)
                err, ok = compare_value(g, r, tolerances[name])
                res.add(f, err, [] if ok else [f"{g} vs {r}"])
            if unplaced:
                res.note = (f"{unplaced} frames left out: the game's {key} is a host the stage export does not "
                            "place (it moves, or is a bind sensor), so the replay cannot know it")
                report.notes.append(f"{name}.{key}: {res.note}")

    # Cap columns: only on frames where both sides have the cap out; who has it out is its own column.
    if "cap_state" in gcols or "cap_state" in rcols:
        res, use = new_result("cap_out", "categorical")
        for f in use:
            res.add(f, None, [] if res.game[f] == res.rewrite[f]
                    else [f"{'out' if res.game[f] == '1' else 'on head'} vs "
                          f"{'out' if res.rewrite[f] == '1' else 'on head'}"])
        if not any(v == "1" for v in list(res.game.values()) + list(res.rewrite.values())):
            res.not_comparable("the cap is never out")
        both = [f for f in use if game[f].get("cap_out") == "1" and rew[f].get("cap_out") == "1"]
        for name in CAP_CATEGORICAL + list(CAP_NUMERIC):
            kind = "categorical" if name in CAP_CATEGORICAL else "numeric"
            cres, _ = new_result(name, kind)
            if not both:
                cres.not_comparable("no frame has the cap out on both sides")
                continue
            for f in both:
                if kind == "categorical":
                    cres.add(f, None, [] if cres.game[f] == cres.rewrite[f]
                             else [f"{cres.game[f] or 'none'} vs {cres.rewrite[f] or 'none'}"])
                else:
                    err, problems = compare_numeric(f, cres.game[f], cres.rewrite[f], tolerances[name], False)
                    cres.add(f, err, problems)

    for res in results:
        res.finish()
    return report


def render(report, lockstep_detail=True):
    out = []
    write = out.append
    write(f"# Trace comparison{' (lockstep)' if report.lockstep else ''}\n")
    write(f"- game: `{report.game_path}`  \n- rewrite: `{report.rewrite_path}`  \n- frames compared: "
          f"{len(report.frames)} ({report.frames[0]}-{report.frames[-1]})\n")
    for note in report.notes:
        write(f"- note: {note}\n")
    if report.env_lines:
        write("\n## Environment\n")
        write("The probe listed contamination in this real-game run; the environment rule "
              "(tools/environment_rule.py) judged it as follows.\n")
        for line in report.env_lines:
            write(f"- {line}")
        write("")
    failing, passing, nc = report.failing(), report.passing(), report.not_comparable()
    write(f"\n**Overall: {report.verdict()}** ({len(failing)} failing, {len(passing)} passing, "
          f"{len(nc)} not comparable)\n")
    write("\n## Summary\n")
    write("| column | result | tolerance | max error | mean error | first bad frame | bad frames |")
    write("|---|---|---|---|---|---|---|")
    for res in report.results:
        if res.status == "N/C":
            continue
        tol_text = "" if res.tol is None else f"{res.tol:g}"
        max_e, mean_e = res.max_err(), res.mean_err()
        first = "" if res.first_bad() is None else res.first_bad()
        write(
            f"| {res.column} | {res.status} | {tol_text} | "
            f"{'' if max_e is None else format(max_e, '.4g')} | {'' if mean_e is None else format(mean_e, '.4g')} | "
            f"{first} | {len(res.bad)} |"
        )
    if nc:
        write("\n## Not comparable\n")
        write("A side never writes the column or key, or the two sides' formats have no mapping: "
              "neither pass nor fail.\n")
        write("| column | why |\n|---|---|")
        for res in nc:
            write(f"| {res.column} | {res.note} |")
    write("\n## Details\n")
    frames = report.frames
    for res in failing:
        write(f"### {res.column}\n")
        first_frame, first_problems = res.bad[0]
        write(f"First mismatch at frame {first_frame}: {'; '.join(first_problems)}\n")
        if res.kind == "categorical":
            use = sorted(res.game)
            write("```\ngame:    " + timeline([(f, res.game[f]) for f in use]) +
                  "\nrewrite: " + timeline([(f, res.rewrite[f]) for f in use]) + "\n```\n")
        else:
            write("| frame | detail |\n|---|---|")
            for frame, problems in res.bad[:8]:
                write(f"| {frame} | {'; '.join(problems)[:300]} |")
            if len(res.bad) > 8:
                write(f"| ... | {len(res.bad) - 8} more frames |")
            write("")
        if report.lockstep and res.errors:
            worst = sorted(res.errors, key=lambda e: -e[1])[:5]
            write("Worst frames: " + ", ".join(f"{f} ({e:.4g})" for f, e in worst) + "\n")
    # What the timelines look like even when everything passes: the state/anim runs.
    for name in ("state", "anim"):
        res = report.result(name)
        if res and res.status == "PASS":
            write(f"### {name} timeline (identical)\n")
            write("```\n" + timeline(sorted(res.game.items())) + "\n```\n")
    if report.lockstep:
        state = report.result("state")
        if state and state.bad:
            confusion = Counter((state.game[f], state.rewrite[f]) for f, _ in state.bad)
            write("### State mismatches in lockstep (game -> rewrite, frames)\n")
            write("| game state | rewrite state | frames |\n|---|---|---|")
            for (g, r), n in confusion.most_common(20):
                write(f"| {g or 'none'} | {r or 'none'} | {n} |")
            write("")
    if report.ignored_probe_columns:
        write("Probe-only columns ignored: " + ", ".join(report.ignored_probe_columns) + "\n")
    return "\n".join(out) + "\n"


def self_check():
    """Small synthetic traces for the rules above: keys, environment, cap gating."""
    import tempfile

    def write_csv(path, rows):
        header = list(rows[0])
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(",".join(header) + "\n")
            for row in rows:
                f.write(",".join(row[k] for k in header) + "\n")

    def trace(frames):
        return [
            {"frame": str(i), "px": "0", "counters": c, "collision": col, "cap_state": cs, "cap_px": cp}
            for i, (c, col, cs, cp) in enumerate(frames)
        ]

    with tempfile.TemporaryDirectory() as folder:
        game, rewrite = os.path.join(folder, "game.csv"), os.path.join(folder, "rewrite.csv")
        write_csv(game, trace([
            ("cjump=0/0|upperpunch=5|prejump=0", "ground=1;wall=0;ceiling=0;gmat=A", "", ""),
            ("cjump=1/3|upperpunch=5|prejump=0", "ground=1;wall=0;ceiling=0;gmat=A", "Throw", "10"),
            ("cjump=1/4|upperpunch=5|prejump=2", "ground=1;wall=0;ceiling=0;gmat=A", "Throw", "20"),
        ]))
        write_csv(rewrite, trace([
            ("cjump=0/10|prejump=0|coyote=2", "ground=1;wall=0", "Throw", "0"),
            ("cjump=1/3|prejump=0|coyote=2", "ground=1;wall=0", "Throw", "10"),
            ("cjump=1/4|prejump=0|coyote=2", "ground=1;wall=0", "Throw", "21"),
        ]))
        report = compare(game, rewrite)
        status = {r.column: r.status for r in report.results}
        assert status["counters.cjump"] == "PASS", status  # the timer is ignored at count zero
        assert status["counters.prejump"] == "FAIL", status
        assert status["counters.upperpunch"] == "N/C" and status["counters.coyote"] == "N/C", status
        assert status["collision.ground"] == "PASS" and status["collision.gmat"] == "N/C", status
        assert report.result("counters.prejump").first_bad() == 2
        # The cap columns skip the frame where only the rewrite has the cap out.
        assert report.result("cap_out").first_bad() == 0
        assert report.result("cap_px").compared == 2 and report.result("cap_px").first_bad() == 2
        # A contaminated run is refused, or compared without its listed frames.
        with open(os.path.join(folder, "game.env.json"), "w", encoding="utf-8") as f:
            json.dump({"clean": False, "contaminated": [{"frame": 2, "reasons": ["wall contact"]}]}, f)
        try:
            compare(game, rewrite)
            raise AssertionError("a contaminated run must be refused")
        except EnvironmentRefused as refused:
            assert "wall contact" in refused.banner and refused.summary.startswith("CONTAMINATED"), refused.banner
        report = compare(game, rewrite, allow_contaminated=True)
        assert report.excluded == [2] and report.result("counters.prejump").status == "PASS"
        assert any(n.startswith("CONTAMINATED") for n in report.notes)
    # The comparator must pass actual replay geometry and require the recorded
    # stage/scenario, rather than trusting a metadata path to an unrelated file.
    with tempfile.TemporaryDirectory() as folder:
        game, rewrite = [os.path.join(folder, name) for name in ("game.csv", "rewrite.csv")]
        sample = dict(frame="0", state="DamageFire/Jump", px="0", py="0", pz="0", ground="1",
                      collision="ground=1;gcode=DamageFire;gmat=Fire", contacts="",
                      hit_hosts="ground=Floor/Collision/15/1/0/0/0/0")
        write_csv(game, [sample])
        write_csv(rewrite, [sample])
        mesh = dict(stage="Test", scenario=1, objects=[dict(static=True, translate=[0, 0, 0])],
                    attributes=[dict(FloorCode="DamageFire", MaterialCode="Fire")],
                    triangles=[[0] * 16], areas=[])
        def write_json(name, value):
            path = os.path.join(folder, name)
            with open(path, "w", encoding="utf-8") as f:
                json.dump(value, f)
            return path
        geometry = write_json("geometry.json", mesh)
        write_json("game.env.json", {"clean": False, "contaminated": [
            {"frame": 0, "reasons": ["floor material changed A -> Fire"]}]})
        for stage, scenario, expected in (("Test", 1, True), ("Other", 1, False), ("Test", 2, False)):
            write_json("game.meta.json", dict(stage=stage, scenario_no=scenario, start_abs=[0, 0, 0]))
            try:
                assert compare(game, rewrite, geometry_path=geometry).env_clean and expected
            except EnvironmentRefused:
                assert not expected
    # The holder's candidate is compared only where the game's host is one the stage export places.
    with tempfile.TemporaryDirectory() as folder:
        game, rewrite = [os.path.join(folder, name) for name in ("game.csv", "rewrite.csv")]
        write_csv(game, [{"frame": "0", "look_sensors": "candidate=Coin/Body/12"},
                         {"frame": "1", "look_sensors": "candidate=Walker/Body/18"},
                         {"frame": "2", "look_sensors": "candidate=Coin/Body/12"}])
        write_csv(rewrite, [{"frame": str(f), "look_sensors": "candidate=Coin/Body/12"} for f in range(3)])
        geometry = os.path.join(folder, "geometry.json")
        with open(geometry, "w", encoding="utf-8") as f:
            json.dump(dict(stage="Test", scenario=1, look_sensors=[dict(host="Coin")]), f)
        with open(os.path.join(folder, "game.meta.json"), "w", encoding="utf-8") as f:
            json.dump(dict(stage="Test", scenario_no=1), f)
        assert compare(game, rewrite).result("look_sensors.candidate").first_bad() == 1
        placed = compare(game, rewrite, geometry_path=geometry).result("look_sensors.candidate")
        assert placed.status == "PASS" and placed.compared == 2 and "1 frames left out" in placed.note
    # The head's rotation: only where a look was asked for or shown, and the animated head agrees.
    import math as _math
    half = lambda degrees: f"0/{_math.sin(_math.radians(degrees) / 2):.6f}/0/{_math.cos(_math.radians(degrees) / 2):.6f}"
    keeper = lambda asked: f"mode=1;count=0;requested={asked}"
    with tempfile.TemporaryDirectory() as folder:
        game, rewrite = os.path.join(folder, "game.csv"), os.path.join(folder, "rewrite.csv")
        write_csv(game, [
            {"frame": "0", "head_q": half(0), "bones": "Head:1/2/3", "look_keeper": keeper(0)},
            {"frame": "1", "head_q": half(10), "bones": "Head:1/2/3", "look_keeper": keeper(1)},
            {"frame": "2", "head_q": half(10), "bones": "Head:1/2/3", "look_keeper": keeper(1)},
            {"frame": "3", "head_q": half(10), "bones": "Head:1/2/3", "look_keeper": keeper(1)},
        ])
        write_csv(rewrite, [
            {"frame": "0", "head_q": half(40), "head_pos": "1/2/3", "head_turn": "0", "look_keeper": keeper(0)},
            {"frame": "1", "head_q": half(10.5), "head_pos": "1/2/3.5", "head_turn": "9", "look_keeper": keeper(1)},
            {"frame": "2", "head_q": half(10), "head_pos": "1/2/8", "head_turn": "9", "look_keeper": keeper(1)},
            {"frame": "3", "head_q": half(15), "head_pos": "1/2/3", "head_turn": "9", "look_keeper": keeper(1)},
        ])
        report = compare(game, rewrite)
        head = report.result("head_q")
        # Frame 0 has no look, frame 2 a head 5 cm away: only frames 1 (half a degree) and 3 (5 degrees).
        assert head.compared == 2 and head.first_bad() == 3 and len(head.bad) == 1, (head.compared, head.bad)
        negated = "/".join(v[1:] if v.startswith("-") else "-" + v for v in half(10).split("/"))
        assert abs(quaternion_angle(half(0), half(10)) - 10) < 1e-3 and quaternion_angle(half(10), negated) < 1e-3
        assert quaternion_angle("", half(10)) is None
    # The skeleton: joints by straight-line distance; a joint only one side logs is left out.
    worst, problems = compare_bones("Head:1/2/3|Hip:0/0/0|Mario:0/0/0", "Head:1/2/4|Hip:0/0/3|nw4f_root:0/0/0", 2.0)
    assert abs(worst - 3.0) < 1e-6 and len(problems) == 1 and problems[0].startswith("Hip: 3.0 cm"), (worst, problems)
    assert compare_bones("Head:1/2/3", "Head:1/2/4", 2.0) == (1.0, [])
    assert compare_bones("Head:1/2/3", "Hip:1/2/3", 2.0)[0] is None
    # The game's Japanese reaction names compare with the rewrite's English ones.
    jp, en = "reaction:着地[走り];se:Land", "se:Land;reaction:LandRun"
    assert compare_set(jp, en, ";", set())[1] == [], compare_set(jp, en, ";", set())
    assert compare_set("reaction:着地", "reaction:LandRun", ";", set())[1] != []
    environment_self_check()
    print("compare-traces checks passed.")


def main():
    if "--self-check" in sys.argv[1:]:
        self_check()
        return
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("game")
    ap.add_argument("rewrite")
    ap.add_argument("--out", help="write the report here (default: print it)")
    ap.add_argument("--json", help="also write the results as JSON here")
    ap.add_argument("--tol", action="append", default=[], metavar="COLUMN=VALUE",
                    help="override a tolerance (repeatable)")
    ap.add_argument("--ignore-events", default="",
                    help="comma-separated event kinds to leave out besides the probe's own, e.g. se,voice")
    ap.add_argument("--lockstep", action="store_true", help="rewrite was run in lockstep")
    ap.add_argument("--geometry", help="the exact stage export used by the rewrite replay (damage/death cause evidence)")
    ap.add_argument("--strict", action="store_true", help="exit with status 1 if any column fails")
    ap.add_argument("--allow-contaminated", action="store_true",
                    help="compare a run whose environment was not clean (its listed frames are left out)")
    args = ap.parse_args()

    overrides = {}
    for item in args.tol:
        name, _, value = item.partition("=")
        try:
            overrides[name] = float(value)
        except ValueError:
            sys.exit(f"bad --tol {item!r}")
    ignore = [k for k in args.ignore_events.split(",") if k]
    try:
        report = compare(args.game, args.rewrite, overrides, ignore, args.lockstep, args.allow_contaminated, args.geometry)
    except EnvironmentRefused as refused:
        print(refused.banner, file=sys.stderr)
        sys.exit(3)
    text = render(report)
    if report.env_known and not report.env_clean:
        text = "> **CONTAMINATED ENVIRONMENT** (compared with --allow-contaminated): see the notes.\n\n" + text
    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump(report.as_dict(), f, indent=1)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"{report.verdict()}: {len(report.failing())} failing columns -> {args.out}")
    else:
        sys.stdout.write(text)
    if args.strict and report.failing():
        sys.exit(1)


if __name__ == "__main__":
    main()
