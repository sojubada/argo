#!/usr/bin/env python3
"""
Generates Argo's tvOS brand assets: the layered (parallax) app icon and the
top shelf images.

Everything is drawn procedurally at 4x and downsampled, so the output is
reproducible and resolution independent.

    python3 tools/make_assets.py

Scene: a Greek galley under a night sky, with a stylised Argo Navis asterism
and a moon low on the horizon.
"""

import math
import os
import random

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BRAND = os.path.join(
    ROOT, "Spartan", "Assets.xcassets", "App Icon & Top Shelf Image.brandassets"
)

SS = 3  # supersampling factor, lowered automatically for very large canvases


def _ss_for(w, h):
    px = w * h
    if px > 3_000_000:
        return 2
    if px > 700_000:
        return 3
    return 4


def _with_ss(w, h):
    """Context-free helper: sets the global SS to something the box can render."""
    global SS
    SS = _ss_for(w, h)
    return SS

# ---------------------------------------------------------------- palette

SKY_TOP = (4, 8, 20)
SKY_MID = (12, 26, 58)
SKY_HORIZON = (38, 74, 124)
SEA_TOP = (14, 32, 66)
SEA_BOTTOM = (3, 6, 16)

GOLD = (226, 179, 95)
GOLD_BRIGHT = (247, 227, 176)
GOLD_DEEP = (156, 106, 42)
PARCHMENT = (243, 237, 224)
HULL = (10, 14, 28)


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


# ---------------------------------------------------------------- gradients


def vertical_gradient(size, stops):
    """stops: [(position 0-1, (r,g,b)), ...] sorted by position. Built one pixel
    wide and stretched, which keeps it cheap at poster resolutions."""
    w, h = size
    column = np.zeros((h, 3), dtype=np.float32)
    for y in range(h):
        t = y / max(h - 1, 1)
        lo, hi = stops[0], stops[-1]
        for i in range(len(stops) - 1):
            if stops[i][0] <= t <= stops[i + 1][0]:
                lo, hi = stops[i], stops[i + 1]
                break
        span = max(hi[0] - lo[0], 1e-6)
        k = (t - lo[0]) / span
        k = k * k * (3 - 2 * k)  # smoothstep
        column[y] = [lo[1][c] + (hi[1][c] - lo[1][c]) * k for c in range(3)]
    strip = Image.fromarray(column.reshape(h, 1, 3).astype(np.uint8), "RGB")
    return strip.resize((w, h), Image.BILINEAR).convert("RGBA")


def radial_glow(size, center, radius, color, strength=1.0, falloff=2.2):
    """A soft additive glow. Computed small and upscaled - it is smooth anyway."""
    w, h = size
    scale = max(1, int(max(w, h) / 360))
    sw, sh = max(2, w // scale), max(2, h // scale)
    ys, xs = np.mgrid[0:sh, 0:sw]
    cx, cy = center[0] / scale, center[1] / scale
    d = np.sqrt((xs - cx) ** 2 + (ys - cy) ** 2) / max(radius / scale, 1e-6)
    a = (np.clip(1.0 - d, 0.0, 1.0) ** falloff * strength * 255.0).astype(np.uint8)
    small = Image.new("RGBA", (sw, sh), color + (0,))
    small.putalpha(Image.fromarray(a, "L"))
    return small.resize((w, h), Image.BILINEAR)


def premultiplied(layer):
    """RGB of an RGBA layer multiplied by its own alpha (i.e. composited on black)."""
    base = Image.new("RGB", layer.size, (0, 0, 0))
    base.paste(layer.convert("RGB"), (0, 0), layer.split()[3])
    return base


def screen(base, layer, strength=0.85):
    """Additive blend that keeps glows from muddying the background."""
    add = premultiplied(layer)
    if strength != 1.0:
        add = add.point(lambda v: int(v * strength))
    out = ImageChops.add(base.convert("RGB"), add)
    return out.convert("RGBA")


def over(base, layer):
    return Image.alpha_composite(base.convert("RGBA"), layer)


# ---------------------------------------------------------------- sky


class Lights:
    """Accumulates every bright element, so the expensive blur-and-screen pass
    happens once per image instead of once per star."""

    def __init__(self, size):
        self.size = size
        self.halo = Image.new("RGBA", size, (0, 0, 0, 0))     # blurred a lot
        self.soft = Image.new("RGBA", size, (0, 0, 0, 0))     # blurred a little
        self.sharp = Image.new("RGBA", size, (0, 0, 0, 0))    # left crisp
        self.dh = ImageDraw.Draw(self.halo)
        self.ds = ImageDraw.Draw(self.soft)
        self.dc = ImageDraw.Draw(self.sharp)

    def apply(self, img, halo_blur, soft_blur):
        out = screen(img, self.halo.filter(ImageFilter.GaussianBlur(halo_blur)), 0.9)
        out = screen(out, self.soft.filter(ImageFilter.GaussianBlur(soft_blur)), 0.9)
        return screen(out, self.sharp, 1.0)

    def compose(self, img, halo_blur, soft_blur):
        """Same, but alpha-preserving - for layers that must stay transparent."""
        out = over(img, self.halo.filter(ImageFilter.GaussianBlur(halo_blur)))
        out = over(out, self.soft.filter(ImageFilter.GaussianBlur(soft_blur)))
        return over(out, self.sharp)


def draw_stars(lights, rng, count, y_limit):
    w, h = lights.size
    for _ in range(count):
        x = rng.uniform(0, w)
        # bias stars toward the upper sky
        y = y_limit * (rng.uniform(0, 1) ** 1.35)
        r = rng.uniform(0.6, 2.6) * SS * 0.5
        bright = rng.uniform(0.25, 1.0)
        if rng.random() < 0.08:
            bright = 1.0
            r *= 1.5
        # stars fade out as they approach the horizon
        fade = min(1.0, (1.0 - y / max(y_limit, 1)) * 1.9 + 0.15)
        a = int(255 * bright * fade)
        tint = lerp((198, 214, 255), (255, 248, 232), rng.random())
        lights.dc.ellipse([x - r, y - r, x + r, y + r], fill=tint + (a,))
        lights.ds.ellipse([x - r * 1.8, y - r * 1.8, x + r * 1.8, y + r * 1.8],
                          fill=tint + (int(a * 0.55),))


def add_glint(lights, x, y, r, color=(255, 252, 240), strength=1.0):
    """A four-ray star glint with a halo around it."""
    a = int(235 * strength)
    lights.dc.ellipse([x - r, y - r, x + r, y + r], fill=color + (a,))
    for dx, dy, length in ((1, 0, 5.4), (0, 1, 3.4)):
        lights.ds.polygon(
            [
                (x - dx * r * length, y - dy * r * length),
                (x + dy * r * 0.5, y + dx * r * 0.5),
                (x + dx * r * length, y + dy * r * length),
                (x - dy * r * 0.5, y - dx * r * 0.5),
            ],
            fill=color + (int(a * 0.8),),
        )
    hr = r * 5.5
    lights.dh.ellipse([x - hr, y - hr, x + hr, y + hr],
                      fill=color + (int(120 * strength),))


# A stylised Argo Navis / Carina asterism: the keel and the sweep of the hull.
ASTERISM = [
    (0.055, 0.300),
    (0.175, 0.205),
    (0.305, 0.255),
    (0.420, 0.160),
    (0.545, 0.235),
    (0.655, 0.135),
]


def draw_asterism(lights, box, strength=1.0):
    """box = (x0, y0, x1, y1) in pixels; the asterism is fitted inside it."""
    x0, y0, x1, y1 = box
    pts = [
        (x0 + (x1 - x0) * (p[0] / 0.655), y0 + (y1 - y0) * (p[1] / 0.30))
        for p in ASTERISM
    ]
    for i in range(len(pts) - 1):
        lights.ds.line([pts[i], pts[i + 1]], fill=(150, 182, 236, int(64 * strength)),
                       width=max(1, int(0.9 * SS)))
    for i, (x, y) in enumerate(pts):
        r = (2.4 if i in (1, 3, 5) else 1.6) * SS * 0.55
        add_glint(lights, x, y, r, strength=min(1.0, 0.72 * strength))


# ---------------------------------------------------------------- the sea


def draw_sea(img, horizon, glow_x, rng, ripple_rows=26, reflect=True,
             split=False):
    w, h = img.size
    sea_h = h - horizon
    if sea_h <= 0:
        return img

    sea = vertical_gradient(
        (w, sea_h),
        [(0.0, lerp(SEA_TOP, SKY_HORIZON, 0.42)), (0.50, SEA_TOP), (1.0, SEA_BOTTOM)],
    )
    # feather the waterline so sky and sea meet in haze, not on a seam
    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).rectangle([0, horizon, w, h], fill=255)
    mask = mask.filter(ImageFilter.GaussianBlur(1.1 * SS))
    full = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    full.paste(sea, (0, horizon))
    img = Image.composite(full, img.convert("RGBA"), mask)

    # soft haze right on the horizon, instead of a hard seam
    haze = radial_glow((w, h), (glow_x, horizon), max(w * 0.42, 1), (120, 168, 226), 0.42, 2.6)
    img = screen(img, haze)

    ripples = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(ripples)
    for i in range(ripple_rows):
        t = (i / max(ripple_rows - 1, 1)) ** 1.5
        y = horizon + sea_h * (0.03 + 0.97 * t)
        length = w * (0.03 + 0.16 * t)
        thickness = max(1, int((0.7 + 1.9 * t) * SS))
        for _ in range(int(4 + 9 * (1 - t))):
            x = rng.uniform(-length, w)
            a = int(34 * (1 - t) + 10)
            d.line([(x, y), (x + length * rng.uniform(0.5, 1.2), y)],
                   fill=(176, 206, 248, a), width=thickness)

    if reflect:
        # moonlit path, widening as it comes toward the viewer
        for i in range(ripple_rows * 2):
            t = (i / max(ripple_rows * 2 - 1, 1)) ** 1.25
            y = horizon + sea_h * (0.01 + 0.99 * t)
            spread = w * (0.012 + 0.10 * t)
            for _ in range(3):
                cx = glow_x + rng.gauss(0, spread * 0.65)
                length = spread * rng.uniform(0.5, 1.5)
                a = int(138 * (1 - t) ** 1.15 + 12)
                d.line([(cx - length / 2, y), (cx + length / 2, y)],
                       fill=GOLD_BRIGHT + (a,), width=max(1, int((0.8 + 2.2 * t) * SS)))

    ripples = ripples.filter(ImageFilter.GaussianBlur(1.25 * SS))
    if split:
        return img, ripples
    return screen(img, ripples)


# ---------------------------------------------------------------- the ship


def galley(size, hull_width, cx, deck_y):
    """Draws the ship on its own transparent layer.

    cx / deck_y are the centre of the deck line; hull_width sets everything else.
    Coordinates are in hull widths, with y growing downward.
    """
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    u = hull_width

    def P(x, y):
        return (cx + (x - 0.5) * u, deck_y + y * u)

    def sheer(x):          # the deck line: level amidships, sweeping up at both ends
        return -0.17 * (2 * x - 1) ** 2

    def keel(x):           # the bottom of the hull
        return -0.17 + 0.37 * math.sin(math.pi * x) ** 0.95

    steps = 200
    xs = [i / steps for i in range(steps + 1)]

    # --- hull
    d.polygon([P(x, sheer(x)) for x in xs] + [P(x, keel(x)) for x in reversed(xs)],
              fill=HULL + (255,))

    # stern ornament: a curl rising off the back
    curl = []
    for i in range(44):
        t = i / 43
        a = math.pi * 1.25 * t
        r = 0.115 * (1 - 0.50 * t)
        curl.append(P(0.985 + r * math.sin(a) * 0.85,
                      -0.165 - r * (1 - math.cos(a)) * 1.20))
    d.line(curl, fill=HULL + (255,), width=max(1, int(u * 0.040)), joint="curve")

    # gold sheer stripe along the deck
    d.line([P(x, sheer(x)) for x in xs], fill=GOLD + (235,),
           width=max(1, int(u * 0.030)), joint="curve")

    # --- mast and yard
    mast_top = -1.06
    d.line([P(0.5, 0.02), P(0.5, mast_top)],
           fill=PARCHMENT + (230,), width=max(1, int(u * 0.024)))
    yard_y = mast_top + 0.09
    d.line([P(0.175, yard_y), P(0.825, yard_y)],
           fill=PARCHMENT + (230,), width=max(1, int(u * 0.026)))

    # --- square sail, bellied out by the wind
    sail_top, sail_bot = yard_y + 0.015, -0.300
    left, right = 0.225, 0.775
    belly = 0.068
    n = 70

    def edge_points():
        pts = []
        for i in range(n + 1):  # top, slightly scalloped under the yard
            t = i / n
            pts.append(P(left + (right - left) * t,
                         sail_top + belly * 0.14 * math.sin(math.pi * t)))
        for i in range(n + 1):  # leech, bowing outward
            t = i / n
            pts.append(P(right + belly * math.sin(math.pi * t) * 0.62,
                         sail_top + (sail_bot - sail_top) * t))
        for i in range(n + 1):  # foot, hanging in a deep curve
            t = i / n
            pts.append(P(right - (right - left) * t,
                         sail_bot + belly * 1.25 * math.sin(math.pi * t)))
        for i in range(n + 1):  # luff
            t = i / n
            pts.append(P(left - belly * math.sin(math.pi * (1 - t)) * 0.62,
                         sail_bot + (sail_top - sail_bot) * t))
        return pts

    pts = edge_points()
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).polygon(pts, fill=255)

    sail = vertical_gradient(size, [(0.0, GOLD_BRIGHT), (0.5, GOLD), (1.0, GOLD_DEEP)])
    sail.putalpha(mask)

    # shading: the cloth falls away toward the leech
    shade = Image.new("RGBA", size, (0, 0, 0, 0))
    sd = ImageDraw.Draw(shade)
    for i in range(n + 1):
        t = i / n
        y = sail_top + (sail_bot - sail_top) * t
        sd.line([P(right + belly * math.sin(math.pi * t) * 0.62, y),
                 P(right - (right - left) * 0.34, y)],
                fill=(70, 40, 8, 78), width=max(1, int(u * 0.025)))
    shade = shade.filter(ImageFilter.GaussianBlur(u * 0.035))

    sail = Image.alpha_composite(sail, shade)
    sail.putalpha(ImageChops.multiply(sail.split()[3], mask))
    layer = Image.alpha_composite(layer, sail)
    d = ImageDraw.Draw(layer)

    # emblem: an eight-pointed star, the one they steer by
    ex, ey = P(0.5, sail_top + (sail_bot - sail_top) * 0.5 + belly * 0.5)
    er = u * 0.135
    star = []
    for i in range(16):
        a = math.pi * 2 * i / 16 - math.pi / 2
        rr = er if i % 2 == 0 else er * 0.38
        star.append((ex + rr * math.cos(a), ey + rr * math.sin(a)))
    d.polygon(star, fill=(255, 250, 235, 185))

    return layer, P(0.5, mast_top)


def water_reflection(layer, waterline, squash=0.52, strength=0.30, blur=6.0):
    """A squashed, faded mirror of `layer` below the waterline."""
    bbox = layer.getbbox()
    if bbox is None:
        return Image.new("RGBA", layer.size, (0, 0, 0, 0))
    sub = layer.crop(bbox)
    w, h = sub.size
    mirror = sub.transpose(Image.FLIP_TOP_BOTTOM).resize(
        (w, max(1, int(h * squash))), Image.LANCZOS
    )
    out = Image.new("RGBA", layer.size, (0, 0, 0, 0))
    out.paste(mirror, (bbox[0], int(waterline)))

    # fade with depth, and clip everything above the water
    mh, mw = out.size[1], out.size[0]
    ramp = np.linspace(0, 1, mh, dtype=np.float32)[:, None]
    fade = np.clip(1.0 - (ramp * mh - waterline) / max(mirror.size[1], 1), 0, 1) ** 1.6
    fade = np.where(np.arange(mh)[:, None] < waterline, 0.0, fade) * strength
    alpha = (np.asarray(out.split()[3], dtype=np.float32) * np.repeat(fade, mw, axis=1))
    out.putalpha(Image.fromarray(np.clip(alpha, 0, 255).astype(np.uint8), "L"))
    return out.filter(ImageFilter.GaussianBlur(blur))


# ---------------------------------------------------------------- scenes


def render_icon_layers(w, h):
    _with_ss(w, h)
    """Returns (back, middle, front) RGBA layers at the requested size."""
    W, H = w * SS, h * SS
    rng = random.Random(20261018)
    horizon = int(H * 0.745)
    ship_cx = W * 0.5
    hull_w = W * 0.235

    # ---- back: the sky
    sky = vertical_gradient((W, H), [(0.0, SKY_TOP), (0.52, SKY_MID), (1.0, SKY_HORIZON)])
    sky = screen(sky, radial_glow((W, H), (ship_cx, horizon), W * 0.52,
                                  (56, 100, 166), 0.52, 2.4))
    lights = Lights((W, H))
    draw_stars(lights, rng, 150, horizon * 0.98)
    draw_asterism(lights, (W * 0.10, H * 0.10, W * 0.90, H * 0.34), 0.95)
    back = lights.apply(sky, 5.0 * SS, 1.3 * SS)

    # ---- the sea. The back layer carries the whole seascape so that parallax
    # can never shift a hole into view; the middle layer is just the glitter on
    # the water, which is what gives the icon its depth when it moves.
    back, ripples = draw_sea(back, horizon, ship_cx, rng, ripple_rows=20, split=True)
    middle = ripples

    # ---- front: the ship and the star it steers by
    front = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ship, mast_top = galley((W, H), hull_w, ship_cx, horizon)
    front = over(front, water_reflection(ship, horizon + hull_w * 0.18,
                                         squash=0.36, strength=0.20, blur=3.2 * SS))
    front = over(front, ship)
    star = Lights((W, H))
    add_glint(star, mast_top[0], mast_top[1] - H * 0.052, 2.6 * SS, strength=1.0)
    front = star.compose(front, 5.0 * SS, 1.3 * SS)

    return [layer.resize((w, h), Image.LANCZOS) for layer in (back, middle, front)]


def render_topshelf(w, h, with_text=True):
    _with_ss(w, h)
    W, H = w * SS, h * SS
    rng = random.Random(7334)
    horizon = int(H * 0.615)
    moon_x, moon_y = W * 0.775, H * 0.200
    ship_cx = W * 0.600
    hull_w = W * 0.110

    img = vertical_gradient((W, H), [(0.0, SKY_TOP), (0.48, SKY_MID), (1.0, SKY_HORIZON)])
    img = screen(img, radial_glow((W, H), (moon_x, horizon), W * 0.40,
                                  (52, 94, 158), 0.55, 2.3))

    lights = Lights((W, H))
    draw_stars(lights, rng, 360, horizon * 0.98)
    draw_asterism(lights, (W * 0.295, H * 0.095, W * 0.600, H * 0.285), 1.25)
    img = lights.apply(img, 5.0 * SS, 1.3 * SS)

    # the moon
    mr = H * 0.050
    img = screen(img, radial_glow((W, H), (moon_x, moon_y), mr * 7.0,
                                  (196, 212, 244), 0.34, 2.8))
    moon = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ImageDraw.Draw(moon).ellipse(
        [moon_x - mr, moon_y - mr, moon_x + mr, moon_y + mr], fill=(246, 248, 255, 255)
    )
    img = over(img, moon.filter(ImageFilter.GaussianBlur(0.7 * SS)))

    img = draw_sea(img, horizon, moon_x, rng, ripple_rows=30)

    ship, mast_top = galley((W, H), hull_w, ship_cx, horizon)
    img = over(img, water_reflection(ship, horizon + hull_w * 0.18,
                                     squash=0.36, strength=0.20, blur=4.0 * SS))
    img = over(img, ship)
    star = Lights((W, H))
    add_glint(star, mast_top[0], mast_top[1] - H * 0.042, 2.2 * SS, strength=0.9)
    img = star.apply(img, 5.0 * SS, 1.3 * SS)

    if with_text:
        img = draw_wordmark(img, W, H)

    return img.resize((w, h), Image.LANCZOS)


def font_or_default(paths, size):
    for p in paths:
        if os.path.exists(p):
            try:
                return ImageFont.truetype(p, size)
            except Exception:
                pass
    return ImageFont.load_default()


def tracked_text(draw, xy, text, font, fill, tracking):
    x, y = xy
    for ch in text:
        draw.text((x, y), ch, font=font, fill=fill)
        x += draw.textlength(ch, font=font) + tracking
    return x - tracking


def tracked_width(draw, text, font, tracking):
    return sum(draw.textlength(c, font=font) for c in text) + tracking * (len(text) - 1)


def draw_wordmark(img, W, H):
    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)

    serif = font_or_default(
        [
            "/usr/share/texmf/fonts/opentype/public/tex-gyre/texgyrepagella-regular.otf",
            "/usr/share/texmf/fonts/opentype/public/tex-gyre/texgyrebonum-regular.otf",
            "/usr/share/fonts/truetype/google-fonts/Lora-Variable.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        ],
        int(H * 0.165),
    )
    sans = font_or_default(
        [
            "/usr/share/fonts/opentype/inter/Inter-Light.otf",
            "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ],
        int(H * 0.040),
    )

    x0 = W * 0.085
    title_y = H * 0.300
    tracking = H * 0.030
    tracked_text(d, (x0, title_y), "ARGO", serif, PARCHMENT + (255,), tracking)
    title_w = tracked_width(d, "ARGO", serif, tracking)

    rule_y = title_y + H * 0.215
    d.line([(x0, rule_y), (x0 + title_w, rule_y)], fill=GOLD + (190,),
           width=max(1, int(H * 0.004)))

    sub_tracking = H * 0.021
    sub = "WEBDAV FOR APPLE TV"
    sub_w = tracked_width(d, sub, sans, sub_tracking)
    tracked_text(d, (x0 + (title_w - sub_w) / 2, rule_y + H * 0.045), sub, sans,
                 (196, 212, 240, 215), sub_tracking)

    shadow = layer.filter(ImageFilter.GaussianBlur(H * 0.012))
    img = Image.alpha_composite(img, shadow)
    return Image.alpha_composite(img, layer)


# ---------------------------------------------------------------- output


def save(img, path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img.save(path)
    print("wrote", os.path.relpath(path, ROOT), img.size)


def main():
    for stack, (w, h) in (
        ("App Icon.imagestack", (400, 240)),
        ("App Icon - App Store.imagestack", (1280, 768)),
    ):
        one = render_icon_layers(w, h)
        two = render_icon_layers(w * 2, h * 2)
        for name, l1, l2 in zip(("Back", "Middle", "Front"), one, two):
            base = os.path.join(BRAND, stack, f"{name}.imagestacklayer",
                                "Content.imageset")
            save(l1, os.path.join(base, f"argo-{name.lower()}@1x.png"))
            save(l2, os.path.join(base, f"argo-{name.lower()}@2x.png"))

    for folder, prefix, (w, h) in (
        ("Top Shelf Image.imageset", "argo-topshelf", (1920, 720)),
        ("Top Shelf Image Wide.imageset", "argo-topshelf-wide", (2320, 720)),
    ):
        save(render_topshelf(w, h), os.path.join(BRAND, folder, f"{prefix}@1x.png"))
        save(render_topshelf(w * 2, h * 2), os.path.join(BRAND, folder, f"{prefix}@2x.png"))


if __name__ == "__main__":
    main()
