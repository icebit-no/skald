#!/usr/bin/env python3
"""Render Skald's icon files: the granite app icon and the flat rune for the menu bar.

    python3 assets/icon/render.py [--out DIR]

The stone is a height map (face, V-groove, paint) lit from the top left. All of it comes from
seeded noise and the standard library alone, so every run gives the same pixels.
"""

from __future__ import annotations

import argparse
import math
import struct
import zlib
from array import array
from dataclasses import dataclass, replace
from multiprocessing import Pool
from pathlib import Path

# --- Noise -----------------------------------------------------------------------------------

_WORD = 0xFFFFFFFF


def lattice_value(x: int, y: int, seed: int) -> float:
    """A fixed pseudo-random value in [0, 1) for an integer lattice point."""
    h = ((x * 374761393) ^ (y * 668265263) ^ (seed * 1274126177)) & _WORD
    h = ((h ^ (h >> 13)) * 1274126177) & _WORD
    return (h ^ (h >> 16)) / 4294967296


def value_noise(x: float, y: float, seed: int) -> float:
    xi, yi = math.floor(x), math.floor(y)
    xf, yf = x - xi, y - yi
    u, v = xf * xf * (3 - 2 * xf), yf * yf * (3 - 2 * yf)
    a = lattice_value(xi, yi, seed)
    b = lattice_value(xi + 1, yi, seed)
    c = lattice_value(xi, yi + 1, seed)
    d = lattice_value(xi + 1, yi + 1, seed)
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v


def fractal_noise(x: float, y: float, seed: int, octaves: int) -> float:
    total, amplitude, frequency = 0.0, 0.5, 1
    for octave in range(octaves):
        total += amplitude * value_noise(x * frequency, y * frequency, seed + octave * 17)
        amplitude *= 0.5
        frequency *= 2
    return total


def clamp(x: float, low: float, high: float) -> float:
    return low if x < low else high if x > high else x


def smoothstep(edge0: float, edge1: float, x: float) -> float:
    t = clamp((x - edge0) / (edge1 - edge0), 0, 1)
    return t * t * (3 - 2 * t)


# --- The drawing -----------------------------------------------------------------------------


@dataclass(frozen=True)
class Stroke:
    """One stroke of the rune, by its centre line."""

    x0: float
    y0: float
    x1: float
    y1: float
    # A stroke that starts on another stroke's centre line ends butt there.
    starts_on_stroke: bool = False

    def scaled(self, factor: float) -> Stroke:
        return replace(self, x0=self.x0 * factor, y0=self.y0 * factor, x1=self.x1 * factor, y1=self.y1 * factor)


@dataclass(frozen=True)
class Stone:
    """A cut and painted stone in the unit square."""

    half_side: float
    strokes: tuple[Stroke, ...]
    groove_half_width: float
    # Square ends follow the stroke rule exactly: flat at free ends, butt where a twig starts.
    # Round ends are what a groove of even depth around the centre line gives.
    square_ends: bool
    # 1 is the stone as chosen at the large sizes. A lower grain flattens pits, speckles and the
    # rough groove floor; a higher grain size makes each grain cover more of the stone.
    grain: float = 1.0
    grain_size: float = 1.0
    # How unsteady the cut is and how unevenly the paint lies; 1 as chosen, 0 for a ruler-straight cut.
    unsteadiness: float = 1.0


# ᚨ in the unit square: a stave and two twigs that start on the stave's centre line.
ANSUZ = (
    Stroke(0.39, 0.21, 0.39, 0.79),
    Stroke(0.39, 0.24, 0.61, 0.41, starts_on_stroke=True),
    Stroke(0.39, 0.43, 0.61, 0.60, starts_on_stroke=True),
)

GRANITE = Stone(half_side=0.402, strokes=ANSUZ, groove_half_width=0.045, square_ends=False)


def ansuz_on_grid(side: float, stave_x: float, top: float, bottom: float, twig_tops: tuple, run: float, drop: float):
    """ᚨ placed on a pixel grid `side` wide, so the stave's walls and ends can fall on pixel edges."""
    stave = Stroke(stave_x, top, stave_x, bottom)
    twigs = [Stroke(stave_x, y, stave_x + run, y + drop, starts_on_stroke=True) for y in twig_tops]
    return tuple(stroke.scaled(1 / side) for stroke in (stave, *twigs))


# The small sizes are drawn for their own pixel grid: the stone's sides and the stave on pixel edges,
# a groove a quarter of the stone wide, twigs moved apart so stone shows between them, a straight cut,
# and grain that is coarser and fainter (32 px) or left out (16 px).
SMALL_DRAWINGS = {
    32: Stone(
        half_side=13 / 32,
        strokes=ansuz_on_grid(32, 12, 7, 25, (8.5, 16), 7, 5.25),
        groove_half_width=2 / 32,
        square_ends=True,
        grain=0.5,
        grain_size=6,
        unsteadiness=0,
    ),
    16: Stone(
        half_side=7 / 16,
        strokes=ansuz_on_grid(16, 6, 4, 12.5, (4.25, 7.75), 3.5, 2.6),
        groove_half_width=1 / 16,
        square_ends=True,
        grain=0,
        unsteadiness=0,
    ),
}

# The menu bar rune on an 18 pt square: a 2 pt stroke, the stave on whole points so it is sharp at 1x.
MENU_BAR_POINTS = 18
MENU_BAR_STROKE = 2.0
MENU_BAR_ANSUZ = ansuz_on_grid(1, 7, 3, 15, (3.75, 7.75), 4.5, 3.5)

STONE_COLOUR = (150, 146, 139)
PAINT_COLOUR = (176, 58, 38)
SEED = 3
# Points towards the light: up, left and out of the stone.
LIGHT = (-0.46, -0.66, 0.59)
# Heights are counted in pixels of a render this wide, so slopes stay the same at every size.
RELIEF_WIDTH = 512
OUTLINE_POWER = 5


def outline_distance(stone: Stone, u: float, v: float) -> float:
    """Distance to the rounded-square outline in units of the side; negative inside."""
    a = stone.half_side
    reach = (abs(u - 0.5) / a) ** OUTLINE_POWER + (abs(v - 0.5) / a) ** OUTLINE_POWER
    return (reach ** (1 / OUTLINE_POWER) - 1) * a


def groove_distance(stone: Stone, u: float, v: float) -> float:
    """Distance from a point to the nearest stroke; the groove is cut where it is below the half width."""
    nearest = 1.0
    for s in stone.strokes:
        dx, dy = s.x1 - s.x0, s.y1 - s.y0
        if stone.square_ends:
            length = math.hypot(dx, dy)
            along = ((u - s.x0) * dx + (v - s.y0) * dy) / length
            across = abs((v - s.y0) * dx - (u - s.x0) * dy) / length
            before_start = stone.groove_half_width - along if s.starts_on_stroke else -along
            d = max(across, before_start, along - length)
        else:
            t = clamp(((u - s.x0) * dx + (v - s.y0) * dy) / (dx * dx + dy * dy), 0, 1)
            d = math.hypot(u - s.x0 - dx * t, v - s.y0 - dy * t)
        nearest = min(nearest, d)
    return nearest


def relief_row(stone: Stone, n: int, y: int) -> tuple[array, array, array, array]:
    """Height, groove depth (0 to 1), paint cover (0 to 1) and outline distance for one pixel row."""
    height, groove, paint, outline = (array("f", bytes(4 * n)) for _ in range(4))
    half_width = stone.groove_half_width
    fine = 1 / stone.grain_size
    v = (y + 0.5) / n
    for x in range(n):
        u = (x + 0.5) / n
        sd = outline_distance(stone, u, v)
        outline[x] = sd
        if sd > 2 / n:
            continue
        # The face: a slow swell, grain, a few pits and a rounded edge.
        h = (fractal_noise(u * 4, v * 4, SEED, 4) - 0.5) * 11
        h += (fractal_noise(u * 42 * fine, v * 42 * fine, SEED + 40, 3) - 0.5) * 2.4 * stone.grain
        pit = value_noise(u * 95 * fine, v * 95 * fine, SEED + 80)
        if pit > 0.87:
            h -= (pit - 0.87) * 16 * stone.grain
        e = clamp(-sd / 0.055, 0, 1)
        h -= (1 - math.sqrt(1 - (1 - e) * (1 - e))) * 24
        # The cut: V-shaped and a little unsteady, as from a pick and chisel.
        wobble = (fractal_noise(u * 11, v * 11, SEED + 120, 3) - 0.5) * 0.012 * stone.unsteadiness
        d = groove_distance(stone, u, v) + wobble
        g = clamp(1 - d / half_width, 0, 1)
        blunted = g if g < 0.86 else 0.86 + (g - 0.86) * 0.45
        floor_roughness = (value_noise(u * 130 * fine, v * 130 * fine, SEED + 160) - 0.5) * 2.2 * stone.grain
        h -= blunted * 17 * (half_width / 0.045) + g * floor_roughness
        height[x] = h * (n / RELIEF_WIDTH)
        groove[x] = g
        # Paint lies in the cut and a little over its edge, thinner in places.
        spill = (fractal_noise(u * 28, v * 28, SEED + 200, 2) - 0.5) * half_width * 0.3 * stone.unsteadiness
        thin = smoothstep(0.6, 0.74, fractal_noise(u * 60 * fine, v * 60 * fine, SEED + 240, 3))
        paint[x] = smoothstep(half_width * 1.08, half_width * 0.9, d + spill) * (1 - thin * 0.35 * stone.unsteadiness)
    return height, groove, paint, outline


def shade_row(stone: Stone, n: int, y: int, above: tuple, here: tuple, below: tuple) -> array:
    """One lit pixel row as red, green, blue (0 to 255, not premultiplied) and alpha (0 to 1)."""
    row = array("d", bytes(8 * 4 * n))
    height, groove, paint, outline = here
    fine = 1 / stone.grain_size
    v = (y + 0.5) / n
    for x in range(n):
        alpha = clamp(0.5 - outline[x] * n, 0, 1)
        if alpha <= 0:
            continue
        u = (x + 0.5) / n
        hx = (height[min(n - 1, x + 1)] - height[max(0, x - 1)]) / 2
        hy = (below[0][x] - above[0][x]) / 2
        diffuse = (-hx * LIGHT[0] - hy * LIGHT[1] + LIGHT[2]) / math.hypot(hx, hy, 1)
        shade = clamp(diffuse / LIGHT[2], 0.22, 1.55) * (1.07 - 0.14 * v) * (1 - 0.22 * groove[x])
        # Stone colour: mottling, then single light and dark grains.
        tone = 0.86 + 0.28 * fractal_noise(u * 6, v * 6, SEED + 300, 3)
        speck = value_noise(u * 150 * fine, v * 150 * fine, SEED + 340)
        if speck > 0.8:
            tone *= 1 + (speck - 0.8) * 1.5 * stone.grain
        elif speck < 0.16:
            tone *= 1 + (speck * 1.7 - 0.28) * stone.grain
        warmth = fractal_noise(u * 9, v * 9, SEED + 420, 2) - 0.5
        r = STONE_COLOUR[0] * tone * (1 + warmth * 0.07)
        g = STONE_COLOUR[1] * tone
        b = STONE_COLOUR[2] * tone * (1 - warmth * 0.08)
        p = paint[x]
        paint_tone = 0.88 + 0.24 * fractal_noise(u * 14, v * 14, SEED + 460, 2)
        r += (PAINT_COLOUR[0] * paint_tone - r) * p
        g += (PAINT_COLOUR[1] * paint_tone - g) * p
        b += (PAINT_COLOUR[2] * paint_tone - b) * p
        # Paint is matte: it neither flares on the lit wall nor goes black on the far one.
        shade += (clamp(shade, 0.45, 1.1) - shade) * p
        k = 4 * x
        row[k] = clamp(r * shade, 0, 255)
        row[k + 1] = clamp(g * shade, 0, 255)
        row[k + 2] = clamp(b * shade, 0, 255)
        row[k + 3] = alpha
    return row


def _carve_band(job: tuple[Stone, int, int, int]) -> bytes:
    stone, n, first, last = job
    relief = {y: relief_row(stone, n, y) for y in range(max(0, first - 1), min(n, last + 1))}
    band = array("d")
    for y in range(first, last):
        band.extend(shade_row(stone, n, y, relief[max(0, y - 1)], relief[y], relief[min(n - 1, y + 1)]))
    return band.tobytes()


# --- Rasters ---------------------------------------------------------------------------------


@dataclass
class Raster:
    """A square image: red, green, blue (0 to 255, not premultiplied) and alpha (0 to 1) per pixel."""

    size: int
    samples: array

    def halved(self) -> Raster:
        """Half the size, each pixel the mean of four, with colour weighted by alpha."""
        n, half, src = self.size, self.size // 2, self.samples
        out = array("d", bytes(8 * 4 * half * half))
        for y in range(half):
            top, bottom = 4 * n * 2 * y, 4 * n * (2 * y + 1)
            for x in range(half):
                quad = (top + 8 * x, top + 8 * x + 4, bottom + 8 * x, bottom + 8 * x + 4)
                cover = sum(src[i + 3] for i in quad)
                if cover <= 0:
                    continue
                k = 4 * (half * y + x)
                for channel in range(3):
                    out[k + channel] = sum(src[i + channel] * src[i + 3] for i in quad) / cover
                out[k + 3] = cover / 4
        return Raster(half, out)

    def reduced_to(self, size: int) -> Raster:
        raster = self
        while raster.size > size:
            raster = raster.halved()
        return raster

    def rgba8(self) -> bytes:
        src = self.samples
        out = bytearray(len(src))
        for i in range(0, len(src), 4):
            out[i] = round(src[i])
            out[i + 1] = round(src[i + 1])
            out[i + 2] = round(src[i + 2])
            out[i + 3] = round(src[i + 3] * 255)
        return bytes(out)


def carve(stone: Stone, n: int, pool: Pool) -> Raster:
    rows_per_band = 16
    jobs = [(stone, n, first, min(n, first + rows_per_band)) for first in range(0, n, rows_per_band)]
    samples = array("d")
    for band in pool.imap(_carve_band, jobs):
        samples.frombytes(band)
    return Raster(n, samples)


# --- The flat rune ---------------------------------------------------------------------------


def stroke_outline(stroke: Stroke, width: float) -> list[tuple[float, float]]:
    """The stroke as a quadrilateral: square at free ends, butt where it starts on another stroke."""
    dx, dy = stroke.x1 - stroke.x0, stroke.y1 - stroke.y0
    length = math.hypot(dx, dy)
    ax, ay = dx / length, dy / length
    half = width / 2
    back = 0 if stroke.starts_on_stroke else half
    sx, sy = stroke.x0 - ax * back, stroke.y0 - ay * back
    ex, ey = stroke.x1 + ax * half, stroke.y1 + ay * half
    nx, ny = -ay * half, ax * half
    return [(sx - nx, sy - ny), (ex - nx, ey - ny), (ex + nx, ey + ny), (sx + nx, sy + ny)]


def _inside(outline: list[tuple[float, float]], x: float, y: float) -> bool:
    """Whether a point lies in a convex outline whose corners run clockwise on screen."""
    sides = zip(outline, outline[1:] + outline[:1])
    return all((bx - ax) * (y - ay) - (by - ay) * (x - ax) >= 0 for (ax, ay), (bx, by) in sides)


def flat_rune(strokes: tuple[Stroke, ...], width: float, size: int) -> Raster:
    """The strokes filled black on a transparent square, for a macOS template image."""
    outlines = [stroke_outline(s, width) for s in strokes]
    grid = 8
    offsets = [(i + 0.5) / grid for i in range(grid)]
    samples = array("d", bytes(8 * 4 * size * size))
    for y in range(size):
        for x in range(size):
            hits = sum(any(_inside(o, x + ox, y + oy) for o in outlines) for oy in offsets for ox in offsets)
            samples[4 * (size * y + x) + 3] = hits / (grid * grid)
    return Raster(size, samples)


# --- PNG -------------------------------------------------------------------------------------


def _chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def write_png(path: Path, raster: Raster) -> None:
    n, pixels = raster.size, raster.rgba8()
    scanlines = b"".join(b"\x00" + pixels[4 * n * y : 4 * n * (y + 1)] for y in range(n))
    header = struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0)
    perceptual_srgb = b"\x00"
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + _chunk(b"IHDR", header)
        + _chunk(b"sRGB", perceptual_srgb)
        + _chunk(b"IDAT", zlib.compress(scanlines, 9))
        + _chunk(b"IEND", b"")
    )


# --- The files -------------------------------------------------------------------------------

LARGE_SIZES = (1024, 512, 256, 128, 64)
# Every size is the mean of whole blocks of a larger render. Rendered at its own size, the grain is
# finer than a pixel from 256 px down and turns into salt-and-pepper noise.
LARGE_RENDER_WIDTH = 2048
SMALL_RENDER_WIDTH = 256


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=Path(__file__).parent, help="where to write the PNG files")
    out = parser.parse_args().out
    out.mkdir(parents=True, exist_ok=True)
    with Pool() as pool:
        granite = carve(GRANITE, LARGE_RENDER_WIDTH, pool)
        for size in LARGE_SIZES:
            granite = granite.reduced_to(size)
            write_png(out / f"app-icon-{size}.png", granite)
        for size, stone in SMALL_DRAWINGS.items():
            write_png(out / f"app-icon-{size}.png", carve(stone, SMALL_RENDER_WIDTH, pool).reduced_to(size))
    for scale, suffix in ((1, ""), (2, "@2x")):
        strokes = tuple(stroke.scaled(scale) for stroke in MENU_BAR_ANSUZ)
        rune = flat_rune(strokes, MENU_BAR_STROKE * scale, MENU_BAR_POINTS * scale)
        write_png(out / f"menu-bar-icon{suffix}.png", rune)


if __name__ == "__main__":
    main()
