"""Dashboard of how far the rewrite is from the real game, for every recorded run.

    py tools/fidelity-report.py <runs dir> [--exe path] [--no-build] [--only name ...]
        [--scenarios dir] [--allow-contaminated] [--profile debug|release]

<runs dir> holds one folder per scenario with the probe's `game.csv` (and its
`game.env.json` / `game.meta.json`). For each of them this

  1. replays the scenario's input script in the rewrite with `--camera game.csv`
     and writes `rewrite.csv` next to the game trace,
  2. compares the two (tools/compare-traces.py) and writes `diff.md` and
     `diff.json` there too, and
  3. writes `<runs dir>/SUMMARY.md`: per scenario the environment status, the
     first frame each group of columns diverges, and the three worst columns
     with their values.

The input script is found as `<scenario>/<name>.input`, else the scenario file
named in `game.meta.json`, else `<name>.input`, in `--scenarios` (default: the
`scenarios` folder beside the runs dir). Folders without a game trace or a
script are listed as skipped. A contaminated real-game run is reported as
refused, as compare-traces.py does, unless --allow-contaminated is given.
Run from the repository root: the replay finds the local sound and effect
tables through `assets/`.
"""
import argparse
import importlib.util
import json
import os
import subprocess
import sys
from datetime import date

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GROUP_ORDER = ["movement", "state", "animation", "cap", "events/sound", "effects", "input", "camera"]
# Groups the dashboard leads with; the others are listed after them.
SHOWN = ["movement", "state", "animation", "cap", "events/sound", "effects"]


_COMPARE = None


def load_compare():
    """tools/compare-traces.py as a module (its file name is not importable)."""
    global _COMPARE
    if _COMPARE is None:
        spec = importlib.util.spec_from_file_location(
            "compare_traces", os.path.join(ROOT, "tools", "compare-traces.py"))
        _COMPARE = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(_COMPARE)
    return _COMPARE


def find_exe(profile):
    name = "plumber-jump-voyage.exe" if os.name == "nt" else "plumber-jump-voyage"
    path = os.path.join(ROOT, "target", profile, name)
    return path


def build(profile):
    command = ["cargo", "build", "--quiet", "--all-features"]
    if profile == "release":
        command.append("--release")
    print("building: " + " ".join(command), flush=True)
    done = subprocess.run(command, cwd=ROOT)
    if done.returncode != 0:
        sys.exit("cargo build failed")


def find_script(run_dir, name, scenarios):
    candidates = [os.path.join(run_dir, f"{name}.input")]
    meta_path = os.path.join(run_dir, "game.meta.json")
    if os.path.exists(meta_path):
        with open(meta_path, encoding="utf-8") as f:
            named = json.load(f).get("scenario_file")
        if named:
            candidates.append(os.path.join(scenarios, named))
    candidates.append(os.path.join(scenarios, f"{name}.input"))
    return next((c for c in candidates if os.path.exists(c)), None)


def replay(exe, script, game, out):
    """Run the rewrite's headless replay; returns an error text or None."""
    done = subprocess.run(
        [exe, "--replay", script, "--trace", out, "--camera", game],
        cwd=ROOT, capture_output=True, text=True,
    )
    if done.returncode != 0:
        return (done.stderr or done.stdout).strip()[-400:]
    return None


def environment_text(compare, game):
    clean, frames, notes, known = compare.read_environment(game)
    if not known:
        return "unknown (no env files)", clean, frames
    if clean:
        return "clean", clean, frames
    reasons = sorted({r for rs in frames.values() for r in rs})
    return f"CONTAMINATED ({len(frames)} frames; {'; '.join(reasons)[:120]})", clean, frames


def worst_columns(report, count=3):
    """The failing columns that disagree on the most frames (derived ones skipped)."""
    failing = [r for r in report.failing() if r.column != "state_nerve"]
    failing.sort(key=lambda r: (-len(r.bad) / max(r.compared, 1), r.first_bad()))
    return failing[:count]


def first_divergences(report):
    """For each group the earliest failing column: (frame, column, detail)."""
    out = {}
    for res in report.failing():
        if res.column == "state_nerve":
            continue
        current = out.get(res.group)
        if current is None or res.first_bad() < current[0]:
            out[res.group] = (res.first_bad(), res.column, res.first_detail())
    return out


def scenario_section(name, env_text, report, run_note=""):
    lines = [f"## {name}\n"]
    lines.append(f"- environment: {env_text}")
    if run_note:
        lines.append(f"- {run_note}")
    lines.append(
        f"- verdict: **{report.verdict()}**, {len(report.frames)} frames compared"
        + (f" ({len(report.excluded)} contaminated frames left out)" if report.excluded else "")
        + f"; {len(report.failing())} failing, {len(report.passing())} passing, "
        f"{len(report.not_comparable())} not comparable columns"
    )
    for note in report.notes:
        if note.startswith("CONTAMINATED"):
            lines.append(f"- {note}")
    lines.append("")
    divergences = first_divergences(report)
    lines.append("| group | first divergence | column | game vs rewrite there |")
    lines.append("|---|---|---|---|")
    for group in GROUP_ORDER:
        if group not in SHOWN and group not in divergences:
            continue
        if group in divergences:
            frame, column, detail = divergences[group]
            lines.append(f"| {group} | frame {frame} | {column} | {detail.replace('|', '/')} |")
        else:
            lines.append(f"| {group} | none | | |")
    lines.append("")
    worst = worst_columns(report)
    if worst:
        lines.append("Worst columns (most frames in disagreement):\n")
        for i, res in enumerate(worst, 1):
            err = res.max_err()
            lines.append(
                f"{i}. `{res.column}`: {len(res.bad)} of {res.compared} frames, first at frame "
                f"{res.first_bad()}: {res.first_detail().replace('|', '/')}"
                + ("" if err is None else f" (max error {err:.4g})")
            )
        lines.append("")
    state, anim = report.result("state"), report.result("anim")
    for label, res in (("state", state), ("anim", anim)):
        if res is not None and res.status != "N/C":
            frames = sorted(res.game)
            lines.append(f"{label} timeline, game:    `{compact(report, res.game, frames)}`  ")
            lines.append(f"{label} timeline, rewrite: `{compact(report, res.rewrite, frames)}`\n")
    return "\n".join(lines)


def compact(report, values, frames, limit=14):
    """Run-length timeline of one column, shortened."""
    compare = load_compare()
    return compare.timeline([(f, values[f]) for f in frames], limit)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("runs", help="folder with one sub-folder per scenario, each with a game.csv")
    ap.add_argument("--exe", help="the rewrite's executable (default: target/<profile>/plumber-jump-voyage)")
    ap.add_argument("--profile", default="debug", choices=["debug", "release"])
    ap.add_argument("--no-build", action="store_true", help="do not run cargo build first")
    ap.add_argument("--scenarios", help="folder with the .input scripts (default: <runs>/../scenarios)")
    ap.add_argument("--only", nargs="*", default=[], help="only these scenario folders")
    ap.add_argument("--allow-contaminated", action="store_true",
                    help="compare contaminated runs too (their listed frames are left out)")
    args = ap.parse_args()

    runs = os.path.abspath(args.runs)
    scenarios = os.path.abspath(args.scenarios or os.path.join(runs, "..", "scenarios"))
    compare = load_compare()
    exe = args.exe or find_exe(args.profile)
    if not args.exe and not args.no_build:
        build(args.profile)
    if not os.path.exists(exe):
        sys.exit(f"{exe} does not exist: build it (cargo build --all-features) or pass --exe")

    names = sorted(
        d for d in os.listdir(runs)
        if os.path.isfile(os.path.join(runs, d, "game.csv")) and (not args.only or d in args.only)
    )
    if not names:
        sys.exit(f"{runs}: no scenario folder with a game.csv")

    sections, table, skipped = [], [], []
    for name in names:
        folder = os.path.join(runs, name)
        game = os.path.join(folder, "game.csv")
        script = find_script(folder, name, scenarios)
        if script is None:
            skipped.append((name, "no input script (looked beside the trace and in the scenarios folder)"))
            continue
        env_text, clean, _ = environment_text(compare, game)
        if not clean and not args.allow_contaminated:
            skipped.append((name, f"REFUSED, the real-game run is contaminated: {env_text}"))
            table.append((name, env_text, "REFUSED", "", ""))
            continue
        rewrite = os.path.join(folder, "rewrite.csv")
        error = replay(exe, script, game, rewrite)
        if error:
            skipped.append((name, f"the replay failed: {error}"))
            continue
        report = compare.compare(game, rewrite, allow_contaminated=args.allow_contaminated)
        with open(os.path.join(folder, "diff.md"), "w", encoding="utf-8") as f:
            f.write(compare.render(report))
        with open(os.path.join(folder, "diff.json"), "w", encoding="utf-8") as f:
            json.dump(report.as_dict(), f, indent=1)
        divergences = first_divergences(report)
        earliest = min((d[0] for d in divergences.values()), default=None)
        table.append((name, env_text, report.verdict(), str(len(report.failing())),
                      "" if earliest is None else str(earliest)))
        sections.append(scenario_section(name, env_text, report, f"script: `{os.path.relpath(script, runs)}`"))
        print(f"{name}: {report.verdict()} ({len(report.failing())} failing columns)", flush=True)

    out = ["# Fidelity summary\n",
           f"Generated {date.today().isoformat()} by `tools/fidelity-report.py` from `{os.path.basename(runs)}/*/game.csv`; "
           "each scenario's `rewrite.csv`, `diff.md` and `diff.json` sit beside its game trace. "
           "Columns a side never writes are not comparable and are neither failures nor passes "
           "(see the scenario's `diff.md`). Both sides change state the way the game's nerves do: "
           "a state is chosen in one frame and runs its first step in the next "
           "(docs/fidelity-harness.md, Sample point).\n",
           "| scenario | environment | verdict | failing columns | first divergence |",
           "|---|---|---|---|---|"]
    for name, env_text, verdict, failing, earliest in table:
        out.append(f"| {name} | {env_text} | {verdict} | {failing} | {earliest} |")
    out.append("")
    out.extend(sections)
    if skipped:
        out.append("## Skipped\n")
        out.extend(f"- {name}: {why}" for name, why in skipped)
        out.append("")
    path = os.path.join(runs, "SUMMARY.md")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")
    print(f"-> {path}")


if __name__ == "__main__":
    main()
