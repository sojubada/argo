#!/usr/bin/env python3
"""테마 아이콘 만들기.

Argo 의 배(아르고호)를 테마마다 다른 하늘·바다·돛 색으로 그린다.
tvOS 아이콘은 겹친 층(뒤·가운데·앞)이라 리모컨을 굴리면 입체로 움직인다.

  python3 tools/make_theme_icons.py Spartan/Assets.xcassets

- <이름>.imagestack        : 다른 아이콘 (400x240 @1x, 800x480 @2x)
- ThemePreview-<이름>.imageset : 설정 화면에 보일 한 장짜리 미리보기
기본 테마(Midnight)는 지금의 주 아이콘을 그대로 쓰고 미리보기만 만든다.
"""
import json, math, os, random, sys
from PIL import Image, ImageDraw, ImageFilter

S = 4                      # 크게 그려 줄여서 가장자리를 부드럽게
W2, H2 = 800, 480          # @2x
W, H = W2 * S, H2 * S

THEMES = {
    # 이름: (하늘 위, 하늘 아래, 바다, 해/별빛, 돛 밝은 쪽, 돛 어두운 쪽, 별 여부)
    "Midnight": ((4, 8, 22), (34, 78, 150), (14, 32, 64), (110, 160, 255), (232, 186, 102), (186, 140, 70), True),
    "Ocean":    ((6, 30, 48), (40, 150, 170), (8, 52, 70), (170, 240, 235), (240, 244, 240), (180, 200, 205), True),
    "Forest":   ((4, 16, 12), (30, 96, 70), (8, 38, 30), (140, 240, 170), (226, 214, 160), (170, 160, 110), True),
    "Ember":    ((40, 10, 22), (240, 120, 60), (60, 20, 26), (255, 196, 110), (250, 236, 214), (210, 180, 150), False),
    "Orchid":   ((18, 8, 34), (150, 80, 190), (34, 16, 56), (230, 170, 255), (245, 225, 250), (190, 160, 210), True),
    "Gold":     ((30, 22, 6), (236, 180, 70), (54, 40, 14), (255, 230, 150), (255, 248, 230), (220, 200, 160), False),
}


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def back_layer(name, t):
    top, bottom, sea, glow, _, _, stars = t
    im = Image.new("RGBA", (W, H))
    d = ImageDraw.Draw(im)
    horizon = int(H * 0.74)
    for y in range(horizon):
        f = (y / horizon) ** 1.4
        d.line([(0, y), (W, y)], fill=lerp(top, bottom, f) + (255,))
    for y in range(horizon, H):
        f = (y - horizon) / (H - horizon)
        d.line([(0, y), (W, y)], fill=lerp(sea, (0, 0, 0), f * 0.7) + (255,))

    # 수평선 위의 빛 (해 또는 별빛 번짐)
    glowimg = Image.new("RGBA", (W, H))
    gd = ImageDraw.Draw(glowimg)
    cx, cy = W // 2, horizon
    for r in range(int(H * 0.55), 0, -S * 6):
        a = int(70 * (1 - r / (H * 0.55)) ** 2)
        gd.ellipse([cx - r * 1.6, cy - r, cx + r * 1.6, cy + r], fill=glow + (a,))
    if not stars:
        # 해. 수평선에 반쯤 걸친다.
        r = int(H * 0.12)
        sx, sy = cx - int(W * 0.24), horizon - int(H * 0.06)
        gd.ellipse([sx - r, sy - r, sx + r, sy + r], fill=glow + (240,))
    glowimg = glowimg.filter(ImageFilter.GaussianBlur(S * 6))
    im.alpha_composite(glowimg)

    # 별·반짝임은 따로 그려 겹친다. ImageDraw 로 반투명 색을 바로 찍으면 섞이지 않고
    # 그 자리의 불투명도가 바뀐다 — tvOS 는 맨 뒤 층에 투명한 점이 있으면 거부한다.
    over = Image.new("RGBA", (W, H))
    if stars:
        rnd = random.Random(name)
        sd = ImageDraw.Draw(over)
        for _ in range(140):
            x = rnd.randrange(W)
            y = rnd.randrange(int(horizon * 0.92))
            rr = rnd.choice([1, 1, 1, 2, 2, 3]) * S * 0.6
            a = rnd.randrange(120, 255)
            sd.ellipse([x - rr, y - rr, x + rr, y + rr], fill=(255, 255, 255, a))

    # 바다 위 반짝임
    rd = ImageDraw.Draw(over)
    rnd = random.Random(name + "sea")
    for i in range(16):
        y = horizon + int((H - horizon) * (i / 16) ** 1.2) + S * 3
        half = int(W * (0.06 + 0.10 * rnd.random()) * (1 - i / 24))
        x0 = W // 2 - half + rnd.randrange(-S * 40, S * 40)
        rd.line([(x0, y), (x0 + half * 2, y)], fill=glow + (int(80 * (1 - i / 20)),), width=S * 2)
    rd.line([(0, horizon), (W, horizon)], fill=glow + (60,), width=S)
    im.alpha_composite(over)
    # 맨 뒤 층은 완전히 불투명해야 한다.
    solid = Image.new("RGBA", (W, H), (0, 0, 0, 255))
    solid.alpha_composite(im)
    return solid


def bezier(p0, p1, p2, n=60):
    return [((1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * p1[0] + t * t * p2[0],
             (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * p1[1] + t * t * p2[1])
            for t in (i / n for i in range(n + 1))]


K = 1.25   # 배 크기. tvOS 는 아이콘 가장자리를 잘라 보이므로 가운데를 키운다.
CY = 0.56 # 배의 세로 중심


def sy(f):
    """세로 비율을 배 크기에 맞춰 옮긴다."""
    return H * (CY + (f - CY) * K)


def middle_layer(t):
    """돛. 배 위에서 바람을 받은 모양."""
    top_c, _, _, glow, light, dark, _ = t
    im = Image.new("RGBA", (W, H))
    d = ImageDraw.Draw(im)
    cx = W // 2
    # 돛대는 돛 뒤에 둔다. 앞에 두면 돛의 별 문장을 두 동강 낸다.
    m = int(S * 3 * K)
    d.rectangle([cx - m, int(sy(0.27)), cx + m, int(sy(0.70))], fill=(240, 240, 236, 255))
    top, bot = int(sy(0.30)), int(sy(0.64))
    half = int(W * 0.075 * K)
    left = bezier((cx - half, top), (cx - half * 1.12, (top + bot) / 2), (cx - half * 0.86, bot))
    bottom = bezier((cx - half * 0.86, bot), (cx, bot + H * 0.035 * K), (cx + half * 0.86, bot))
    right = bezier((cx + half * 0.86, bot), (cx + half * 1.12, (top + bot) / 2), (cx + half, top))
    poly = left + bottom[1:] + right[1:] + [(cx - half, top)]
    d.polygon(poly, fill=light + (255,))
    # 오른쪽 그늘 — 바람에 부푼 느낌
    shade = Image.new("L", (W, H), 0)
    sd = ImageDraw.Draw(shade)
    sd.polygon(poly, fill=255)
    mask = Image.new("L", (W, H), 0)
    ImageDraw.Draw(mask).rectangle([cx + half * 0.15, 0, W, H], fill=200)
    mask = mask.filter(ImageFilter.GaussianBlur(S * 10))
    from PIL import ImageChops
    m = ImageChops.multiply(shade, mask)
    im.paste(Image.new("RGBA", (W, H), dark + (255,)), (0, 0), m)
    # 돛의 별 문장
    star = []
    cy = (top + bot) / 2
    R, r = H * 0.055 * K, H * 0.022 * K
    for i in range(16):
        a = math.pi / 8 * i - math.pi / 2
        rad = R if i % 2 == 0 else r
        star.append((cx + rad * math.cos(a), cy + rad * math.sin(a)))
    # 흰 돛에는 흰 별이 안 보인다. 돛이 밝으면 하늘 색으로 찍는다.
    bright = sum(light) / 3 > 215
    d.polygon(star, fill=(lerp(top_c, glow, 0.45) if bright else (255, 255, 255)) + (230,))
    return im


def front_layer(t):
    """배 몸통·돛대·활대."""
    _, _, _, glow, light, _, _ = t
    im = Image.new("RGBA", (W, H))
    d = ImageDraw.Draw(im)
    cx = W // 2
    mast_top, deck = int(sy(0.27)), int(sy(0.70))
    # 돛대와 활대
    # 돛 아래로 보이는 돛대 밑동만 앞에 그린다.
    m = int(S * 3 * K)
    d.rectangle([cx - m, int(sy(0.655)), cx + m, deck], fill=(240, 240, 236, 255))
    yard = int(sy(0.29))
    d.rectangle([cx - W * 0.085 * K, yard, cx + W * 0.085 * K, yard + S * 6 * K], fill=(240, 240, 236, 255))
    # 몸통: 위는 완만한 곡선, 아래는 깊은 곡선
    hl, hr = cx - W * 0.13 * K, cx + W * 0.12 * K
    upper = bezier((hl, deck - H * 0.03 * K), (cx, deck + H * 0.03 * K), (hr, deck - H * 0.035 * K))
    lower = bezier((hr, deck - H * 0.035 * K), (cx + W * 0.02 * K, deck + H * 0.17 * K), (hl, deck - H * 0.03 * K))
    d.polygon(upper + lower[1:], fill=(10, 14, 26, 255))
    d.line(upper, fill=light + (255,), width=int(S * 5 * K))
    # 뱃머리 장식 (말린 끝)
    tip = (hr, deck - H * 0.035 * K)
    d.arc([tip[0] - S * 4 * K, tip[1] - H * 0.09 * K, tip[0] + W * 0.035 * K, tip[1] + S * 6 * K],
          start=180, end=80, fill=(10, 14, 26, 255), width=int(S * 7 * K))
    return im


def save_imageset(path, base, img2x):
    os.makedirs(path, exist_ok=True)
    img1x = img2x.resize((W2 // 2, H2 // 2), Image.LANCZOS)
    img2x.save(os.path.join(path, base + "@2x.png"))
    img1x.save(os.path.join(path, base + "@1x.png"))
    json.dump({"images": [{"filename": base + "@1x.png", "idiom": "tv", "scale": "1x"},
                          {"filename": base + "@2x.png", "idiom": "tv", "scale": "2x"}],
               "info": {"author": "xcode", "version": 1}},
              open(os.path.join(path, "Contents.json"), "w"), indent=2)


def info(extra=None):
    d = {"info": {"author": "xcode", "version": 1}}
    if extra: d.update(extra)
    return d


def main(catalog):
    for name, t in THEMES.items():
        layers = {
            "Back": back_layer(name, t),
            "Middle": middle_layer(t),
            "Front": front_layer(t),
        }
        small = {k: v.resize((W2, H2), Image.LANCZOS) for k, v in layers.items()}

        if name != "Midnight":
            stack = os.path.join(catalog, f"Theme{name}.imagestack")
            os.makedirs(stack, exist_ok=True)
            json.dump(info({"layers": [{"filename": "Front.imagestacklayer"},
                                       {"filename": "Middle.imagestacklayer"},
                                       {"filename": "Back.imagestacklayer"}]}),
                      open(os.path.join(stack, "Contents.json"), "w"), indent=2)
            for lname, img in small.items():
                layer = os.path.join(stack, f"{lname}.imagestacklayer")
                os.makedirs(layer, exist_ok=True)
                json.dump(info(), open(os.path.join(layer, "Contents.json"), "w"), indent=2)
                save_imageset(os.path.join(layer, "Content.imageset"), f"{name.lower()}-{lname.lower()}", img)

        flat = small["Back"].copy()
        flat.alpha_composite(small["Middle"])
        flat.alpha_composite(small["Front"])
        save_imageset(os.path.join(catalog, f"ThemePreview-{name}.imageset"),
                      f"preview-{name.lower()}", flat.convert("RGB").convert("RGBA"))
        print("made", name)


if __name__ == "__main__":
    main(sys.argv[1])
