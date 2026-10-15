# Cuts the 30-expression character sheet (6 x 5 grid, navy background) into one transparent PNG per
# expression plus manifest.json. Usage: python3 slice-oma-sheet.py <sheet.png> <output folder>
# The pictures go in oma/character/sprites. Needs numpy, scipy and Pillow.
import sys, json
import numpy as np
from PIL import Image
from scipy import ndimage as ndi

SRC = sys.argv[1]; OUT = sys.argv[2]
im = np.asarray(Image.open(SRC).convert("RGB")).astype(np.float32) / 255.0
H, W, _ = im.shape
NAMES = ["idle","listening","thinking","searching","speaking","greeting",
         "happy","laughing","surprised","curious","confused","shy",
         "sad","worried","annoyed","sleepy","sleeping","winking",
         "love","proud","celebrating","apologetic","error","success",
         "uploading","dancing","sunglasses","winter-scarf","party-hat","crown"]
BANDS = [(118, 303), (330, 518), (552, 738), (772, 952), (990, 1182)]
CW = W / 6.0
OW, OH = 256, 220          # output canvas
ANCHOR_X, TOP = 128, 14     # body centre x; every cell keeps its own height, exactly as drawn on the sheet

meta = {}
for idx, name in enumerate(NAMES):
    r, c = divmod(idx, 6)
    y0, y1 = BANDS[r]
    x0, x1 = int(round(c * CW)), int(round((c + 1) * CW))
    cell = im[y0:y1, x0:x1]
    h, w, _ = cell.shape
    # Background colour: the quietest border pixels of this cell.
    ring = np.concatenate([cell[0], cell[-1], cell[:, 0], cell[:, -1]])
    lum = ring.mean(axis=1)
    bg = np.median(ring[lum <= np.percentile(lum, 60)], axis=0)

    # Body silhouette: bright pixels, biggest piece, holes filled (the dark visor sits inside the shell).
    bright = cell.mean(axis=2) > 0.62
    lab, n = ndi.label(ndi.binary_closing(bright, iterations=2))
    if n == 0:
        raise SystemExit(f"{name}: nothing found")
    sizes = ndi.sum(bright, lab, range(1, n + 1))
    body = lab == (1 + int(np.argmax(sizes)))
    body = ndi.binary_fill_holes(body)
    interior = ndi.binary_erosion(body, iterations=3)

    # Everything else is light over the dark background: recover alpha and colour from that.
    a = np.max((cell - bg) / np.maximum(1e-3, 1.0 - bg), axis=2)
    a = np.clip(a, 0, 1)
    a[a < 0.05] = 0
    a = np.where(interior, 1.0, a)
    safe = np.maximum(a, 1e-3)[..., None]
    color = np.clip((cell - (1 - a)[..., None] * bg) / safe, 0, 1)
    color = np.where(interior[..., None], cell, color)

    # Specks from the neighbouring picture that touch the edge of the cell are not part of this one.
    keep = ndi.binary_dilation(body, iterations=2) | (a > 0.05)
    pieces, count = ndi.label(a > 0.05)
    for k in range(1, count + 1):
        piece = pieces == k
        ys_, xs_ = np.nonzero(piece)
        touches = xs_.min() == 0 or xs_.max() == w - 1 or ys_.min() == 0 or ys_.max() == h - 1
        if touches and piece.sum() < 200 and not (piece & ndi.binary_dilation(body, iterations=3)).any():
            a[piece] = 0
            color[piece] = 0
    # Where she stands: centre of the body, bottom of the feet. Effects (hearts, rings) do not move her.
    ys, xs = np.nonzero(body)
    cx = float(np.median(xs)); feet = int(ys.max())
    rgba = np.dstack([color, a])
    img = Image.fromarray((rgba * 255 + 0.5).astype(np.uint8), "RGBA")
    out = Image.new("RGBA", (OW, OH), (0, 0, 0, 0))
    out.alpha_composite(img, (int(round(ANCHOR_X - cx)), TOP))
    out.save(f"{OUT}/{idx+1:02d}-{name}.png", optimize=True)
    # Where the body is on the shared canvas, so an app can stand every expression on the same spot.
    meta[name] = {
        "file": f"{idx+1:02d}-{name}.png",
        "bodyWidth": int(xs.max() - xs.min()),
        "bodyHeight": int(ys.max() - ys.min()),
        "bodyTop": int(ys.min() + TOP),
        "bodyBottom": int(ys.max() + TOP),
        "centerX": ANCHOR_X,
    }
json.dump(meta, open(f"{OUT}/manifest.json", "w"), indent=1)
print("ok", len(meta))
