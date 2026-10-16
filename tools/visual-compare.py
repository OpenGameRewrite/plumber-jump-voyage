#!/usr/bin/env python3
"""Side-by-side pictures of the real game and the rewrite, frame by frame.

    py tools/visual-compare.py <run dir> [--crop W H] [--scale S]

A run directory holds `game.csv` and `rewrite.csv` (traces) and, for each
frame listed by the scenario's `shot` line, `game-<f>.png` (the real game's
framebuffer) and `rewrite-<f>.png` (`plumber-jump-voyage --replay run.input
--shots <dir> --camera game.csv`). For every frame that has both pictures it
writes

  side-<f>.png   game on the left, rewrite on the right, each under a label
                 with the frame, state, clip and clip frame of its own trace;
  crop-<f>.png   the same two pictures cut around Mario and enlarged. Mario's
                 screen position is projected from the game trace's position
                 and camera (`cam_px/py/pz`, `cam_real_yaw/pitch`, `cam_fov`),
                 so both crops show the same part of the screen even when the
                 rewrite's Mario stands somewhere else;

and `visual.md`, a table of the frames with both traces' values. Standard
library and Pillow only. Nothing here reads game data other than the pictures
and traces the harness already wrote.
"""
import argparse
import csv
import math
import os
import re
import sys

from PIL import Image, ImageDraw, ImageFont

WIDTH, HEIGHT = 1600, 900
# Height above the feet that the crop centres on (roughly Mario's waist), cm.
CROP_HEIGHT_CM = 80.0
LABEL_COLUMNS = ("state", "anim", "anim_frame")


def read_trace(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def number(row, key, default=None):
    try:
        return float(row[key])
    except (KeyError, TypeError, ValueError):
        return default


def frame_row(rows, frame):
    """The row of `frame` (rows are numbered from 0, but trust the column)."""
    if 0 <= frame < len(rows) and number(rows[frame], "frame") == frame:
        return rows[frame]
    for row in rows:
        if number(row, "frame") == frame:
            return row
    return None


def project(camera, point):
    """Screen pixel of a world point (cm) seen by the game's camera row.

    Right-handed, +Y up; a camera looking along +Z has screen-right = -X.
    Returns None when the point is behind the camera.
    """
    px, py, pz = (number(camera, k) for k in ("cam_px", "cam_py", "cam_pz"))
    yaw = math.radians(number(camera, "cam_real_yaw", number(camera, "cam_yaw", 0.0)))
    pitch = math.radians(number(camera, "cam_real_pitch", 0.0))
    fov = math.radians(number(camera, "cam_fov", 40.0))
    if None in (px, py, pz):
        return None
    forward = (math.sin(yaw) * math.cos(pitch), math.sin(pitch), math.cos(yaw) * math.cos(pitch))
    right = (-forward[2], 0.0, forward[0])  # forward x up(Y), normalised below
    length = math.hypot(right[0], right[2]) or 1.0
    right = (right[0] / length, 0.0, right[2] / length)
    up = (
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    )
    d = (point[0] - px, point[1] - py, point[2] - pz)
    depth = sum(a * b for a, b in zip(d, forward))
    if depth <= 1e-3:
        return None
    x = sum(a * b for a, b in zip(d, right))
    y = sum(a * b for a, b in zip(d, up))
    half = math.tan(fov / 2.0)
    sx = WIDTH / 2 + x / depth / (half * WIDTH / HEIGHT) * (WIDTH / 2)
    sy = HEIGHT / 2 - y / depth / half * (HEIGHT / 2)
    return sx, sy


def mario_point(row):
    return (number(row, "px", 0.0), number(row, "py", 0.0) + CROP_HEIGHT_CM, number(row, "pz", 0.0))


def font(size):
    for name in ("arial.ttf", "DejaVuSans.ttf", "segoeui.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def describe(row):
    """One label line from a trace row: state, clip and its frame."""
    if row is None:
        return "no trace row"
    clip_frame = number(row, "anim_frame")
    where = "" if clip_frame is None else f"@{clip_frame:g}"
    return f"{row.get('state', '?')}  {row.get('anim', '?')}{where}"


def labelled(image, title, row, frame, scale):
    """The picture under a strip with its title and trace values."""
    image = image.resize((round(image.width * scale), round(image.height * scale)), Image.LANCZOS)
    strip = 54
    out = Image.new("RGB", (image.width, image.height + strip), (24, 24, 28))
    draw = ImageDraw.Draw(out)
    draw.text((10, 6), f"{title}   frame {frame}", fill=(255, 255, 255), font=font(20))
    draw.text((10, 29), describe(row), fill=(190, 220, 255), font=font(18))
    out.paste(image, (0, strip))
    return out


def join(left, right, gap=6):
    out = Image.new("RGB", (left.width + gap + right.width, max(left.height, right.height)), (255, 0, 255))
    out.paste(left, (0, 0))
    out.paste(right, (left.width + gap, 0))
    return out


def crop_around(image, centre, size):
    width, height = size
    left = round(centre[0] - width / 2)
    top = round(centre[1] - height / 2)
    left = max(0, min(WIDTH - width, left))
    top = max(0, min(HEIGHT - height, top))
    return image.crop((left, top, left + width, top + height)), (left, top)


def frames_with_pictures(run):
    found = {}
    for name in os.listdir(run):
        match = re.fullmatch(r"game-(\d+)\.png", name)
        if match and os.path.isfile(os.path.join(run, f"rewrite-{match.group(1)}.png")):
            found[int(match.group(1))] = None
    return sorted(found)


def cell(row, key):
    return "" if row is None else (row.get(key) or "")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("run", help="run directory with game.csv, rewrite.csv and the pictures")
    parser.add_argument("--crop", nargs=2, type=int, metavar=("W", "H"), default=(420, 460),
                        help="size in pixels of the crop around Mario (default 420 460)")
    parser.add_argument("--zoom", type=float, default=2.0, help="enlargement of the crops (default 2)")
    parser.add_argument("--scale", type=float, default=1.0, help="scale of the full side-by-side pictures")
    args = parser.parse_args()

    run = args.run
    game = read_trace(os.path.join(run, "game.csv"))
    try:
        rewrite = read_trace(os.path.join(run, "rewrite.csv"))
    except FileNotFoundError:
        rewrite = []
    frames = frames_with_pictures(run)
    if not frames:
        sys.exit(f"{run}: no frame has both game-<f>.png and rewrite-<f>.png")

    lines = [
        f"# Visual comparison: {os.path.basename(os.path.normpath(run))}",
        "",
        "Left: the real game. Right: the rewrite, rendered with the game's camera for that frame.",
        "`side-<f>.png` is the whole picture, `crop-<f>.png` the area around Mario enlarged.",
        "",
        "| frame | game state / clip | rewrite state / clip | game effects_live | rewrite effects_live | pictures |",
        "|---|---|---|---|---|---|",
    ]
    notes = []
    for frame in frames:
        game_row = frame_row(game, frame)
        rewrite_row = frame_row(rewrite, frame)
        game_image = Image.open(os.path.join(run, f"game-{frame}.png")).convert("RGB")
        rewrite_image = Image.open(os.path.join(run, f"rewrite-{frame}.png")).convert("RGB")
        if game_image.size != rewrite_image.size:
            print(f"frame {frame}: sizes differ, game {game_image.size}, rewrite {rewrite_image.size}", file=sys.stderr)
            rewrite_image = rewrite_image.resize(game_image.size, Image.LANCZOS)

        side = join(
            labelled(game_image, "GAME", game_row, frame, args.scale),
            labelled(rewrite_image, "REWRITE", rewrite_row, frame, args.scale),
        )
        side.save(os.path.join(run, f"side-{frame}.png"))

        # Both crops are cut where the game's trace puts Mario, through the game's camera.
        game_at = project(game_row, mario_point(game_row)) if game_row else None
        rewrite_at = project(game_row, mario_point(rewrite_row)) if game_row and rewrite_row else None
        centre = game_at or (WIDTH / 2, HEIGHT / 2)
        # The dust trail lies below and behind Mario: shift the window a little down.
        centre = (centre[0], centre[1] + 40)
        crop_size = tuple(args.crop)
        pair = []
        for title, image, row in (("GAME", game_image, game_row), ("REWRITE", rewrite_image, rewrite_row)):
            cut, _ = crop_around(image, centre, crop_size)
            cut = cut.resize((round(cut.width * args.zoom), round(cut.height * args.zoom)), Image.LANCZOS)
            pair.append(labelled(cut, title, row, frame, 1.0))
        join(*pair).save(os.path.join(run, f"crop-{frame}.png"))

        def at(point):
            return "-" if point is None else f"({point[0]:.0f},{point[1]:.0f})"

        lines.append(
            f"| {frame} | {describe(game_row)} | {describe(rewrite_row)} "
            f"| {cell(game_row, 'effects_live')} | {cell(rewrite_row, 'effects_live')} "
            f"| side-{frame}.png, crop-{frame}.png |"
        )
        notes.append(
            f"- Frame {frame}: Mario's waist projects to {at(game_at)} (game trace) and {at(rewrite_at)} "
            f"(rewrite trace) through the game's camera. Game events: `{cell(game_row, 'events')}`; "
            f"rewrite events: `{cell(rewrite_row, 'events')}`. Game blend `{cell(game_row, 'blend')}` "
            f"interp `{cell(game_row, 'interp')}`; rewrite blend `{cell(rewrite_row, 'blend')}` "
            f"interp `{cell(rewrite_row, 'interp')}`."
        )
        print(f"frame {frame}: side-{frame}.png crop-{frame}.png")
    lines += ["", *notes]
    with open(os.path.join(run, "visual.md"), "w", encoding="utf-8") as out:
        out.write("\n".join(lines) + "\n")
    print(f"{os.path.join(run, 'visual.md')}")


if __name__ == "__main__":
    main()
