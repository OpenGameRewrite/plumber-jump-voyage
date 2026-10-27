"""Exact recording repeat checks, separating visual observations with unfixed seeds.

This does not relax game-vs-rewrite comparisons. The recording runner calls it.
"""
import csv
from collections import Counter


def split_rows(rows):
    strict, visual = [], []
    for row in rows:
        row = dict(row)
        for key in ("game_frame", "tick"):
            row.pop(key, None)  # boot-relative counter / wall clock
        residual = {}
        if "effects_live" in row:
            # Emitter::Initialize uses nn::vfx::detail::Random, separately from
            # the probe's reset of sead::Random; natural particle death varies.
            residual["effects_live"] = row.pop("effects_live")
        if "fx_kills" in row:
            kept, observed = [], []
            for item in filter(None, row["fx_kills"].split("|")):
                fields = dict(part.partition("=")[::2] for part in item.split(";"))
                # The probe logs graphics updates of all actors. Other owners
                # are outside the player replay; their precise source of repeat
                # variation is unproved. Keep explicit Mario/cap calls exact;
                # only natural set lifetime is seed-dependent for them.
                # dropped=N counts overflow of the all-owner diagnostic buffer,
                # not a player action. Report that lost coverage separately.
                graphics_kinds = {"tkill", "delall", "delclr", "stopcd", "tupd", "setkill", "fin", "queue", "invalid"}
                by_design = set(fields) == {"dropped"} or (
                    fields.get("o") == "other" and fields.get("k") in graphics_kinds) or (
                    fields.get("o") in ("player", "model", "deco", "cap")
                    and fields.get("k") in ("queue", "invalid", "fin"))
                (observed if by_design else kept).append(item)
            row["fx_kills"] = "|".join(kept)
            residual["fx_kills"] = "|".join(observed)
        strict.append(row)
        visual.append(residual)
    return strict, visual


def compare_repeats(first, second):
    """(exact match outside by-design observations, residual differing-row counts).

    CSV headers, row counts and every remaining cell must match exactly, including
    animation, effects starts/stops, fx_sets and the player's explicit effect kills.
    """
    def read(path):
        with open(path, newline="", encoding="utf-8") as f:
            reader = csv.DictReader(f)
            return reader.fieldnames, list(reader)

    acols, a = read(first)
    bcols, b = read(second)
    sa, va = split_rows(a)
    sb, vb = split_rows(b)
    differences = Counter(k for x, y in zip(va, vb) for k in x.keys() | y.keys() if x.get(k) != y.get(k))
    incomplete = 0
    for x, y, sx, sy in zip(a, b, sa, sb):
        if "dropped=" in x.get("fx_kills", "") or "dropped=" in y.get("fx_kills", ""):
            incomplete += 1
            # A shared all-owner buffer can truncate different Mario calls in
            # otherwise matching rows. Missing observations prove neither a
            # match nor a mismatch: expose the coverage gap, still check every
            # other cell exactly. Complete rows retain explicit-call equality.
            sx.pop("fx_kills", None)
            sy.pop("fx_kills", None)
    if incomplete:
        differences["fx_kills_incomplete"] = incomplete
    return acols == bcols and sa == sb, dict(differences)


def self_check():
    import os
    import tempfile

    row = dict(frame="0", px="1.000000", state="DamageFire/Dead", events="effect:model:DeadFire",
               game_frame="100", tick="1", effects_live="model:DeadFire", fx_sets="fixed",
               fx_kills="ph=tail;k=queue;o=model;n=DeadFire|ph=mov;k=tkill;o=player;n=Fire")
    other = dict(row, game_frame="200", tick="2", effects_live="",
                 fx_kills="ph=tail;k=stopcd;o=other;n=?|ph=mov;k=tkill;o=player;n=Fire")
    with tempfile.TemporaryDirectory() as folder:
        def write(name, rows):
            path = os.path.join(folder, name)
            with open(path, "w", newline="", encoding="utf-8") as f:
                writer = csv.DictWriter(f, fieldnames=list(row))
                writer.writeheader()
                writer.writerows(rows)
            return path
        first = write("a.csv", [row])
        assert compare_repeats(first, write("b.csv", [other])) == (True, {"effects_live": 1, "fx_kills": 1})
        for key in ("px", "state", "events", "fx_sets", "fx_kills"):
            changed = dict(other)
            changed[key] = "different"
            assert not compare_repeats(first, write("b.csv", [changed]))[0], key
        dropped = dict(other, fx_kills=other["fx_kills"] + "|dropped=2")
        assert compare_repeats(first, write("b.csv", [dropped]))[1]["fx_kills_incomplete"] == 1
        assert compare_repeats(first, write("b.csv", [dict(dropped, px="1.000001")]))[0] is False
        assert not compare_repeats(first, write("b.csv", []))[0]
        assert split_rows([dict(fx_kills="k=new-kind;o=other")])[0][0]["fx_kills"] == "k=new-kind;o=other"
    print("repeat-trace checks passed.")


if __name__ == "__main__":
    self_check()
