"""Slice the Quiver cozy-room art into the assets the pixel office needs.

Produces (under frontend/public/):
  bg/room.png              the room backdrop (half-res of 背景图)
  props/station.png        one trimmed isometric workbench+stool+rug (桌子图)
  props/fire.png           3-frame horizontal strip of the plain campfire (元素集2)
  sprites/char{N}-walk.png      6-frame walk cycle  (sheet row 0)
  sprites/char{N}-bow.png       6-frame bow draw/shoot (sheet row 1, the "tense" loop)
  sprites/char{N}-idle.png      idle-front standing frame (row 2 col 4)
  sprites/char{N}-reading.png   sitting reading a book — the "work" frame (row 2 col 5)
  sprites/char{N}-celebrate.png fist-pump jump (row 2 col 3)
  sprites/char{N}-sick.png      cloak-wrapped slump (row 2 col 2)
  sprites/char{N}-portrait.png  idle-front portrait for the detail panel (row 2 col 0)

Backgrounds in the source art:
  * Character sheets + 元素集2 ship a BAKED grey/white checkerboard (no real alpha).
    We key it out by flood-filling the edge-connected checker; the art is walled off
    by its coloured outline so interior grey art is never reached.
  * 桌子图 + 元素集1 already have real alpha — just trim + scale.

Run: ~/.venvs/current/bin/python scripts/slice_assets.py
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image

SRC = Path("/Users/links/Documents/image/resources/quiver")
PUB = Path(__file__).resolve().parent.parent / "frontend" / "public"

# ---------------------------------------------------------------------------
# checkerboard key-out (shared by character sheets and 元素集2)
# ---------------------------------------------------------------------------
# The checker is two neutral greys with anti-aliased seams. It spans a wide
# brightness band, so colour-keying alone leaves seam residue. We flood-fill
# from the edges: the checker is edge-connected, the art is walled off by its
# coloured outline, so interior neutral art (white hair, grey kettle) survives.
CHECKER_BRIGHT_MIN = 80
CHECKER_BRIGHT_MAX = 240
CHECKER_NEUTRAL_TOL = 16


def _is_checker(px: tuple[int, int, int, int]) -> bool:
    r, g, b, a = px
    if a == 0:
        return True
    if max(r, g, b) - min(r, g, b) > CHECKER_NEUTRAL_TOL:
        return False
    bright = (r + g + b) // 3
    return CHECKER_BRIGHT_MIN <= bright <= CHECKER_BRIGHT_MAX


def key_out_checker(img: Image.Image) -> Image.Image:
    """Flood-fill the edge-connected checkerboard to transparency (returns a copy)."""
    img = img.convert("RGBA")
    px = img.load()
    w, h = img.size
    seen = bytearray(w * h)
    stack: list[tuple[int, int]] = []
    for x in range(w):
        stack.append((x, 0))
        stack.append((x, h - 1))
    for y in range(h):
        stack.append((0, y))
        stack.append((w - 1, y))
    while stack:
        x, y = stack.pop()
        if x < 0 or y < 0 or x >= w or y >= h:
            continue
        i = y * w + x
        if seen[i]:
            continue
        seen[i] = 1
        if not _is_checker(px[x, y]):
            continue
        px[x, y] = (0, 0, 0, 0)
        stack.extend(((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)))
    return img


def strip_bottom_bleed(cell: Image.Image) -> Image.Image:
    """Clear a thin content band at the very bottom of a frame cell.

    The source rows aren't cleanly separated: the NEXT row's characters' hoods
    overflow upward past the cell boundary, so a row-1 (bow) cell catches a strip
    of row-2 hood-tops below the archer's feet. That band is thin and separated
    from the body by a transparent gap, so we detect it (gap above + a real body
    above the gap + band thinner than ~22% of the cell) and clear it. A character
    whose feet are flush with the cell bottom has no gap, so it is left untouched
    (the held ember sits at hand height, well inside the body span, so it
    survives too).
    """
    w, h = cell.size
    a = cell.split()[3].load()
    rowfull = [any(a[x, y] > 0 for x in range(w)) for y in range(h)]
    if not rowfull[h - 1]:
        return cell
    y = h - 1
    while y >= 0 and rowfull[y]:
        y -= 1
    band_h = (h - 1) - y
    if y < 0 or band_h > h * 0.22 or not any(rowfull[:y]):
        return cell
    cell = cell.copy()
    px = cell.load()
    for yy in range(y + 1, h):
        for xx in range(w):
            px[xx, yy] = (0, 0, 0, 0)
    return cell


def trim(img: Image.Image, pad: int = 4) -> Image.Image:
    """Crop to the non-transparent bounding box plus a little padding."""
    bbox = img.getbbox()
    if bbox is None:
        return img
    l, t, r, b = bbox
    w, h = img.size
    return img.crop((max(0, l - pad), max(0, t - pad), min(w, r + pad), min(h, b + pad)))


def scaled(img: Image.Image, factor: float) -> Image.Image:
    return img.resize((max(1, int(img.width * factor)), max(1, int(img.height * factor))), Image.NEAREST)


# ---------------------------------------------------------------------------
# background + station
# ---------------------------------------------------------------------------
def slice_background() -> None:
    (PUB / "bg").mkdir(parents=True, exist_ok=True)
    im = Image.open(SRC / "背景图.png").convert("RGBA")
    # half-res keeps it crisp and shrinks 6.3MB -> ~1MB
    out = scaled(im, 0.5)
    out.save(PUB / "bg" / "room.png", optimize=True)
    print(f"bg/room.png {out.width}x{out.height}")


def slice_station() -> None:
    (PUB / "props").mkdir(parents=True, exist_ok=True)
    im = Image.open(SRC / "桌子图.png").convert("RGBA")  # clean alpha
    out = scaled(trim(im, pad=6), 0.45)
    out.save(PUB / "props" / "station.png", optimize=True)
    print(f"props/station.png {out.width}x{out.height}")


# ---------------------------------------------------------------------------
# campfire: 3 plain-fire frames -> one horizontal strip (uniform cell size)
# ---------------------------------------------------------------------------
# Hand-measured cells in 元素集2 (the 3 plain campfires, top-left). Each ~256px
# wide; fire+logs occupy y ~185..345.
FIRE_CELLS = [(70, 330), (330, 590), (600, 856)]
FIRE_Y = (180, 348)
FIRE_SCALE = 0.6


def slice_fire() -> None:
    (PUB / "props").mkdir(parents=True, exist_ok=True)
    im = key_out_checker(Image.open(SRC / "元素集2.png"))
    frames = [im.crop((s, FIRE_Y[0], e, FIRE_Y[1])) for s, e in FIRE_CELLS]
    # normalise to a common cell size so the strip animates without jitter
    cw = max(f.width for f in frames)
    ch = max(f.height for f in frames)
    cw_s, ch_s = int(cw * FIRE_SCALE), int(ch * FIRE_SCALE)
    strip = Image.new("RGBA", (cw_s * len(frames), ch_s), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        # center each frame in its cell, bottom-aligned (logs sit on the floor)
        canvas = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
        canvas.paste(f, ((cw - f.width) // 2, ch - f.height), f)
        strip.paste(scaled(canvas, FIRE_SCALE), (i * cw_s, 0))
    strip.save(PUB / "props" / "fire.png", optimize=True)
    print(f"props/fire.png {strip.width}x{strip.height} ({len(frames)} frames, cell {cw_s}x{ch_s})")


# ---------------------------------------------------------------------------
# characters
# ---------------------------------------------------------------------------
ROWS, COLS = 3, 6
NUM_CHARS = 4
CHAR_SCALE = 0.5
# Fixed output cell so EVERY char + frame is the same size — the loader uses a
# single frameWidth/frameHeight (HallScene/PreloaderScene CHAR_CELL), so per-char
# heights would mis-slice the strips. Content is scaled to fit inside the cell
# minus the top safety band, then bottom-centred. Keep in lockstep with CHAR_CELL.
OUT_W, OUT_H, TOP_BAND = 234, 268, 12

# row-major frame indices in the 6x3 grid
WALK_FRAMES = [0, 1, 2, 3, 4, 5]  # row 0: walk cycle
BOW_FRAMES = [6, 7, 8, 9, 10, 11]  # row 1: draw -> aim -> shoot (the tense loop)
SINGLE_FRAMES = {
    "portrait": 12,  # idle front (detail-panel portrait)
    "sick": 14,  # cloak-wrapped slump (dejected/fail)
    "celebrate": 15,  # fist-pump jump
    "idle": 16,  # idle front, relaxed (waiting)
    "reading": 17,  # sitting reading a book (the work loop frame)
}


def _frame_box(idx: int, fw: int, fh: int) -> tuple[int, int, int, int]:
    row, col = divmod(idx, COLS)
    return (col * fw, row * fh, col * fw + fw, row * fh + fh)


def slice_characters() -> None:
    (PUB / "sprites").mkdir(parents=True, exist_ok=True)
    for i in range(1, NUM_CHARS + 1):
        sheet = key_out_checker(Image.open(SRC / f"Quiver-人物贴图{i}.png"))
        w, h = sheet.size
        fw, fh = w // COLS, h // ROWS
        ci = i - 1  # 0-based to match worker slot % NUM_CHARS

        # one shared bbox across ALL used frames so every pose/animation frame is
        # registered to the same origin (feet don't jump between states).
        used = WALK_FRAMES + BOW_FRAMES + list(SINGLE_FRAMES.values())
        bbox: list[int] | None = None
        crops: dict[int, Image.Image] = {}
        for idx in used:
            c = strip_bottom_bleed(sheet.crop(_frame_box(idx, fw, fh)))
            crops[idx] = c
            b = c.getbbox()
            if b is None:
                continue
            if bbox is None:
                bbox = list(b)
            else:
                bbox[0] = min(bbox[0], b[0])
                bbox[1] = min(bbox[1], b[1])
                bbox[2] = max(bbox[2], b[2])
                bbox[3] = max(bbox[3], b[3])
        if bbox is None:
            bbox = [0, 0, fw, fh]
        pad = 6
        bbox = [
            max(0, bbox[0] - pad),
            max(0, bbox[1] - pad),
            min(fw, bbox[2] + pad),
            min(fh, bbox[3] + pad),
        ]
        crop_box = tuple(bbox)

        def cell(idx: int) -> Image.Image:
            # Scale the shared crop to fit the fixed cell minus the top band (the
            # same scale for every frame of a char, so feet stay registered), then
            # bottom-centre it. The top band keeps the hat crown off the cell edge.
            crop = crops[idx].crop(crop_box)
            fit = min(CHAR_SCALE, OUT_W / crop.width, (OUT_H - TOP_BAND) / crop.height)
            img = scaled(crop, fit)
            canvas = Image.new("RGBA", (OUT_W, OUT_H), (0, 0, 0, 0))
            canvas.paste(img, ((OUT_W - img.width) // 2, OUT_H - img.height), img)
            return canvas

        # animation strips
        for name, idxs in (("walk", WALK_FRAMES), ("bow", BOW_FRAMES)):
            strip = Image.new("RGBA", (OUT_W * len(idxs), OUT_H), (0, 0, 0, 0))
            for j, idx in enumerate(idxs):
                strip.paste(cell(idx), (j * OUT_W, 0))
            strip.save(PUB / "sprites" / f"char{ci}-{name}.png", optimize=True)

        # single frames
        for name, idx in SINGLE_FRAMES.items():
            cell(idx).save(PUB / "sprites" / f"char{ci}-{name}.png", optimize=True)

        print(f"char{ci}: frame {fw}x{fh} -> cell {OUT_W}x{OUT_H}")


def main() -> None:
    slice_background()
    slice_station()
    slice_fire()
    slice_characters()


if __name__ == "__main__":
    main()
