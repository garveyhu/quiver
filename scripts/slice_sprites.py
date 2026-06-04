"""Pre-slice the Quiver character sheets into the per-pose PNGs the pixel office needs.

Only sheets 1-4 are used: they are 6-col x 3-row grids of fantasy ARCHERS that
slice cleanly into self-contained per-cell frames. Sheets 5-7 turned out to be a
different 7-col layout of modern office workers whose wide computer/run frames
bleed across cell borders AND are off-theme (not archers), so they are skipped
for v1 — worker slots cycle through the 4 archers (index mod 4).

For each archer we cut the 6 poses the office needs (see ARCHER_POSES), key out
the baked-in checkerboard background to real transparency (flood-fill from the
edges), trim the shared margin, half-scale, and write to frontend/public/sprites/
so Vite serves them.

Run: ~/.venvs/current/bin/python scripts/slice_sprites.py
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image

SRC_DIR = Path("/Users/links/Documents/image/character/quiver")

# The source sheets ship a baked-in opaque grey checkerboard (no real alpha):
# two neutral shades ~106/~152 plus anti-aliased seams in between (~98..160).
# Colour-keying alone left seam residue, so we FLOOD-FILL the background from the
# frame edges through the grey band: the checker is edge-connected, while the
# character art is walled off by its coloured outline — so interior grey art
# (white hair, grey robe, the wooden stool) is never reached.
# the checker shades differ per sheet (dark square ~96..125, light square
# ~139..195), so the band is wide; flood-fill connectivity keeps it from eating
# interior grey art even though pure-white highlights (~210+) fall just above.
CHECKER_MIN = 85
CHECKER_MAX = 202
CHECKER_NEUTRAL_TOL = 18


def _is_checker(px: tuple[int, int, int, int]) -> bool:
    r, g, b, a = px
    if a == 0:
        return True
    if max(r, g, b) - min(r, g, b) > CHECKER_NEUTRAL_TOL:
        return False
    bright = (r + g + b) // 3
    return CHECKER_MIN <= bright <= CHECKER_MAX


def key_out_checker(img: Image.Image) -> Image.Image:
    """Return a copy with the edge-connected checkerboard flood-filled to transparent."""
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


OUT_DIR = Path(__file__).resolve().parent.parent / "frontend" / "public" / "sprites"

ROWS = 3
COLS = 6
NUM_CHARS = 4  # sheets 1-4 (the clean archer sheets)

# pose output name -> source frame index (row-major, 0-based) in the 6x3 grid.
ARCHER_POSES: dict[str, int] = {
    "idle": 16,       # idle front portrait
    "reading": 17,    # sitting reading a book (focused work)
    "bow": 8,         # drawing the bow (tense)
    "celebrate": 15,  # fist-pump
    "sick": 13,       # hooded turned away (dejected)
    "portrait": 12,   # idle portrait (waiting)
}

# downscale factor for the output frames (native frame ~469x512 is overkill on a
# cozy canvas; half-res stays crisp with pixelArt nearest-neighbour upscaling).
SCALE = 0.5


def slice_sheet(sheet_path: Path, char_index: int) -> None:
    sheet = Image.open(sheet_path).convert("RGBA")
    sheet_w, sheet_h = sheet.size
    fw = sheet_w // COLS
    fh = sheet_h // ROWS

    # crop each needed frame, keying the checkerboard out to real transparency
    frames: dict[str, Image.Image] = {}
    for pose, idx in ARCHER_POSES.items():
        row, col = divmod(idx, COLS)
        box = (col * fw, row * fh, col * fw + fw, row * fh + fh)
        frames[pose] = key_out_checker(sheet.crop(box))

    # compute one shared bounding box across this character's frames so poses
    # stay aligned (anchored to the same origin) while trimming dead margin.
    bbox = None
    for img in frames.values():
        b = img.getbbox()
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

    # add a small uniform padding so feet/hands never touch the edge
    pad = 8
    bbox[0] = max(0, bbox[0] - pad)
    bbox[1] = max(0, bbox[1] - pad)
    bbox[2] = min(fw, bbox[2] + pad)
    bbox[3] = min(fh, bbox[3] + pad)
    crop_box = tuple(bbox)

    out_w = int((crop_box[2] - crop_box[0]) * SCALE)
    out_h = int((crop_box[3] - crop_box[1]) * SCALE)

    for pose, img in frames.items():
        cropped = img.crop(crop_box).resize((out_w, out_h), Image.NEAREST)
        out_path = OUT_DIR / f"char{char_index}-{pose}.png"
        cropped.save(out_path, optimize=True)

    print(f"char{char_index}: frame {fw}x{fh} -> crop {crop_box} -> {out_w}x{out_h}")


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for i in range(1, NUM_CHARS + 1):
        sheet_path = SRC_DIR / f"Quiver-人物贴图{i}.png"
        if not sheet_path.exists():
            raise FileNotFoundError(sheet_path)
        slice_sheet(sheet_path, char_index=i - 1)  # 0-based to match worker index mod NUM_CHARS


if __name__ == "__main__":
    main()
