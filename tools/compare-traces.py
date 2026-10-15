"""Compare two per-frame traces (the real game's and the rewrite's) frame by frame.

    py tools/compare-traces.py game.csv rewrite.csv --out diff.md
        [--lockstep] [--tol column=value ...] [--ignore-events se,voice] [--strict]

Both files are CSV with a header row and one row per player update. Frames are
aligned on the `frame` column. A column that is empty in every row of one file
is reported as `missing` (neither pass nor fail); inside a column that exists on
both sides, an empty cell means "none" and must match "none".

Column kinds:
  numeric      absolute error against a tolerance (angles compare around 360)
  categorical  exact text; reported as a timeline of runs for each side
  set          order-insensitive items (events, buttons, live effects)
  keyed        `key=value` or per-slot items compared slot by slot
               (animation layers, parts, bones, counters, sounds, ...)

Use --lockstep when the rewrite was run with `--lockstep game.csv`: every frame
then starts from the game's previous state, so the report also counts how many
frames disagree and ranks the worst ones rather than looking for drift.
"""
import argparse
import csv
import re
import sys
from collections import Counter, OrderedDict

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
        ("cap_px", 0.5), ("cap_py", 0.5), ("cap_pz", 0.5),
    ]
)
ANGLES = {"cam_yaw", "cam_pitch"}
CATEGORICAL = [
    "ground", "state", "state_nerve", "anim", "anim_loop", "sub_anim", "cap_state", "cap_anim",
    "cam_mode", "face", "model",
]
SETS = OrderedDict([("events", ";"), ("effects_live", "|"), ("triggers", "|"), ("in_buttons", "+")])
# Per-slot columns: name -> (item separator, tolerance of the numbers inside).
KEYED = OrderedDict(
    [
        ("blend", 0.02), ("interp", 0.02), ("anim_layers", 0.02), ("parts", 0.05),
        ("bones", 2.0), ("counters", 0), ("pre_input", 0), ("collision", 0),
        ("random", 0), ("sounds", 0.02),
    ]
)
VECTORS = OrderedDict([("in_stick", 0.01), ("move_input", 0.01), ("floor_normal", 0.01)])

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
    return columns, by_frame


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


NUMBER = re.compile(r"-?\d+(?:\.\d+)?")


def numbers_in(item):
    """The numbers in an item, and the item's text with each number replaced by #."""
    return [float(n) for n in NUMBER.findall(item)], NUMBER.sub("#", item)


class Result:
    def __init__(self, column, kind):
        self.column = column
        self.kind = kind
        self.status = "PASS"
        self.errors = []  # one worst error per compared frame (numeric kinds)
        self.bad = []  # (frame, [problem text])
        self.game = {}
        self.rewrite = {}
        self.note = ""

    def add(self, frame, err, problems):
        if err is not None:
            self.errors.append((frame, err))
        if problems:
            self.bad.append((frame, problems))

    def finish(self):
        if self.status == "MISSING":
            return
        self.status = "FAIL" if self.bad else "PASS"

    def max_err(self):
        return max((e for _, e in self.errors), default=None)

    def mean_err(self):
        return sum(e for _, e in self.errors) / len(self.errors) if self.errors else None


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


def compare_keyed(g, r, tol):
    gi, ri = keyed_items(g), keyed_items(r)
    problems, worst = [], None
    for key in gi.keys() - ri.keys():
        problems.append(f"{key}: only in game ({gi[key]})")
    for key in ri.keys() - gi.keys():
        problems.append(f"{key}: only in rewrite ({ri[key]})")
    for key in gi.keys() & ri.keys():
        gn, gt = numbers_in(gi[key])
        rn, rt = numbers_in(ri[key])
        if gt != rt or len(gn) != len(rn):
            problems.append(f"{key}: {gi[key]} vs {ri[key]}")
            continue
        for a, b in zip(gn, rn):
            err = abs(a - b)
            worst = err if worst is None else max(worst, err)
            if err > tol:
                problems.append(f"{key}: {gi[key]} vs {ri[key]}")
                break
    return worst, problems


def compare_vector(g, r, tol):
    if g == "" and r == "":
        return None, []
    gv = [as_float(x) for x in g.split("/")] if g else []
    rv = [as_float(x) for x in r.split("/")] if r else []
    if len(gv) != len(rv) or None in gv + rv:
        return None, [f"{g or 'none'} vs {r or 'none'}"]
    err = max((abs(a - b) for a, b in zip(gv, rv)), default=0.0)
    return err, ([f"{g} vs {r}"] if err > tol else [])


def compare_set(g, r, sep, ignore):
    keep = lambda text: Counter(i for i in split_items(text, sep) if i.split(":", 1)[0] not in ignore)
    gc, rc = keep(g), keep(r)
    problems = []
    only_g = sorted((gc - rc).elements())
    only_r = sorted((rc - gc).elements())
    if only_g:
        problems.append("game only: " + ", ".join(only_g))
    if only_r:
        problems.append("rewrite only: " + ", ".join(only_r))
    return None, problems


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


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("game")
    ap.add_argument("rewrite")
    ap.add_argument("--out", help="write the report here (default: print it)")
    ap.add_argument("--tol", action="append", default=[], metavar="COLUMN=VALUE",
                    help="override a tolerance (repeatable)")
    ap.add_argument("--ignore-events", default="",
                    help="comma-separated event kinds to leave out, e.g. se,voice")
    ap.add_argument("--lockstep", action="store_true", help="rewrite was run in lockstep")
    ap.add_argument("--strict", action="store_true", help="exit with status 1 if any column fails")
    args = ap.parse_args()

    tolerances = dict(NUMERIC)
    tolerances.update({k: v for k, v in KEYED.items()})
    tolerances.update(VECTORS)
    for item in args.tol:
        name, _, value = item.partition("=")
        try:
            tolerances[name] = float(value)
        except ValueError:
            sys.exit(f"bad --tol {item!r}")
    ignore = {k for k in args.ignore_events.split(",") if k}

    gcols, game = read_trace(args.game)
    rcols, rew = read_trace(args.rewrite)
    frames = [f for f in game if f in rew]
    notes = []
    if not frames:
        sys.exit("no frame numbers in common")
    for label, trace in (("game", game), ("rewrite", rew)):
        gaps = [f for f in trace if f - 1 not in trace and f != min(trace)]
        if gaps:
            notes.append(f"{label} trace skips frames before {gaps[:5]}")
        counter = [int(r["game_frame"]) for r in trace.values() if r.get("game_frame", "").lstrip("-").isdigit()]
        steps = {b - a for a, b in zip(counter, counter[1:])}
        if steps - {1}:
            notes.append(f"{label} game_frame steps are not all 1: {sorted(steps)[:6]}")
    only_g = len(game) - len(frames)
    only_r = len(rew) - len(frames)
    if only_g or only_r:
        notes.append(f"{only_g} game frames and {only_r} rewrite frames have no partner")

    results = []

    def column(name):
        return [r.get(name, "") for r in (game[f] for f in frames)], [r.get(name, "") for r in (rew[f] for f in frames)]

    def prepare(name, kind):
        res = Result(name, kind)
        gvals, rvals = column(name)
        res.game, res.rewrite = dict(zip(frames, gvals)), dict(zip(frames, rvals))
        gempty, rempty = not any(gvals), not any(rvals)
        in_g, in_r = name in gcols, name in rcols
        if not (in_g or in_r):
            return None
        if gempty or rempty:
            res.status = "MISSING"
            res.note = "missing in " + ("both" if gempty and rempty else "game" if gempty else "rewrite")
        results.append(res)
        return res

    for name in list(NUMERIC):
        res = prepare(name, "numeric")
        if res is None or res.status == "MISSING":
            continue
        for f in frames:
            mod = None
            if name == "anim_frame":
                # Looping clips wrap at their length; compare around the circle.
                length = as_float(game[f].get("anim_len", "")) or 0
                if length > 0 and (game[f].get("anim_loop") == "1" or rew[f].get("anim_loop") == "1"):
                    mod = length
            err, problems = compare_numeric(f, res.game[f], res.rewrite[f], tolerances[name], name in ANGLES, mod)
            res.add(f, err, problems)
    for name in CATEGORICAL:
        res = prepare(name, "categorical")
        if res is None or res.status == "MISSING":
            continue
        for f in frames:
            if res.game[f] != res.rewrite[f]:
                res.add(f, None, [f"{res.game[f] or 'none'} vs {res.rewrite[f] or 'none'}"])
    for name, sep in SETS.items():
        res = prepare(name, "set")
        if res is None or res.status == "MISSING":
            continue
        for f in frames:
            _, problems = compare_set(res.game[f], res.rewrite[f], sep, ignore if name == "events" else set())
            res.add(f, None, problems)
    for name in KEYED:
        res = prepare(name, "keyed")
        if res is None or res.status == "MISSING":
            continue
        for f in frames:
            err, problems = compare_keyed(res.game[f], res.rewrite[f], tolerances[name])
            res.add(f, err, problems)
    for name in VECTORS:
        res = prepare(name, "vector")
        if res is None or res.status == "MISSING":
            continue
        for f in frames:
            err, problems = compare_vector(res.game[f], res.rewrite[f], tolerances[name])
            res.add(f, err, problems)
    for res in results:
        res.finish()

    out = []
    write = out.append
    write(f"# Trace comparison{' (lockstep)' if args.lockstep else ''}\n")
    write(f"- game: `{args.game}`  \n- rewrite: `{args.rewrite}`  \n- frames compared: {len(frames)} "
          f"({frames[0]}-{frames[-1]})\n")
    for note in notes:
        write(f"- note: {note}\n")
    failing = [r for r in results if r.status == "FAIL"]
    write(f"\n**Overall: {'FAIL' if failing else 'PASS'}** "
          f"({len(failing)} failing, {sum(r.status == 'PASS' for r in results)} passing, "
          f"{sum(r.status == 'MISSING' for r in results)} missing columns)\n")
    write("\n## Summary\n")
    write("| column | result | tolerance | max error | mean error | first bad frame | bad frames |")
    write("|---|---|---|---|---|---|---|")
    for res in results:
        tol = tolerances.get(res.column)
        tol_text = "" if tol is None else f"{tol:g}"
        max_e, mean_e = res.max_err(), res.mean_err()
        first = res.bad[0][0] if res.bad else ""
        write(
            f"| {res.column} | {res.status}{' (' + res.note + ')' if res.note else ''} | {tol_text} | "
            f"{'' if max_e is None else format(max_e, '.4g')} | {'' if mean_e is None else format(mean_e, '.4g')} | "
            f"{first} | {len(res.bad) if res.status != 'MISSING' else ''} |"
        )
    write("\n## Details\n")
    for res in failing:
        write(f"### {res.column}\n")
        first_frame, first_problems = res.bad[0]
        write(f"First mismatch at frame {first_frame}: {'; '.join(first_problems)}\n")
        if res.kind in ("categorical",):
            write("```\ngame:    " + timeline([(f, res.game[f]) for f in frames]) +
                  "\nrewrite: " + timeline([(f, res.rewrite[f]) for f in frames]) + "\n```\n")
        else:
            write("| frame | detail |\n|---|---|")
            for frame, problems in res.bad[:8]:
                write(f"| {frame} | {'; '.join(problems)[:300]} |")
            if len(res.bad) > 8:
                write(f"| ... | {len(res.bad) - 8} more frames |")
            write("")
        if args.lockstep and res.errors:
            worst = sorted(res.errors, key=lambda e: -e[1])[:5]
            write("Worst frames: " + ", ".join(f"{f} ({e:.4g})" for f, e in worst) + "\n")
    # What the timelines look like even when everything passes: the state/anim runs.
    if not failing or any(r.column in ("state", "anim") for r in results):
        for name in ("state", "anim"):
            res = next((r for r in results if r.column == name and r.status != "MISSING"), None)
            if res and res.status == "PASS":
                write(f"### {name} timeline (identical)\n")
                write("```\n" + timeline([(f, res.game[f]) for f in frames]) + "\n```\n")
    if args.lockstep:
        state = next((r for r in results if r.column == "state" and r.status != "MISSING"), None)
        if state and state.bad:
            confusion = Counter((state.game[f], state.rewrite[f]) for f, _ in state.bad)
            write("### State mismatches in lockstep (game -> rewrite, frames)\n")
            write("| game state | rewrite state | frames |\n|---|---|---|")
            for (g, r), n in confusion.most_common(20):
                write(f"| {g or 'none'} | {r or 'none'} | {n} |")
            write("")
    text = "\n".join(out) + "\n"
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"{'FAIL' if failing else 'PASS'}: {len(failing)} failing columns -> {args.out}")
    else:
        sys.stdout.write(text)
    if args.strict and failing:
        sys.exit(1)


if __name__ == "__main__":
    main()
