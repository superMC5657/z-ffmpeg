#!/usr/bin/env python3
"""z-ffmpeg 图标生成脚本。

设计:品牌渐变(indigo -> violet,精确复刻 src/index.css 的 bg-gradient-brand)
圆角方形背景 + 居中白色播放三角(视频编码主题)+ 左上柔和高光。

输出到 src-tauri/icons/:
  - 32x32.png / 128x128.png / 128x128@2x.png(圆角版,Windows/Linux 直接使用)
  - icon.ico(16/24/32/48/64/128/256 多尺寸,圆角版)
  - icon.icns(全方形版,macOS 系统自动磨圆)

仅依赖 Pillow。用法:python scripts/gen_icons.py
"""

import math
import os
import sys

from PIL import Image, ImageDraw, ImageFilter

# ---------------------------------------------------------------------------
# 品牌色(oklch -> sRGB,取自 src/index.css)
# ---------------------------------------------------------------------------

C1_OKLCH = (0.55, 0.23, 277)  # indigo
C2_OKLCH = (0.60, 0.25, 293)  # violet


def oklch_to_srgb(L, C, H_deg):
    a = C * math.cos(math.radians(H_deg))
    b = C * math.sin(math.radians(H_deg))
    l_ = L + 0.3963377774 * a + 0.2158037573 * b
    m_ = L - 0.1055613458 * a - 0.0638541728 * b
    s_ = L - 0.0894841775 * a - 1.2914855480 * b
    l, m, s = l_ ** 3, m_ ** 3, s_ ** 3
    r = +4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
    g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
    b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s

    def gamma(c):
        c = max(0.0, min(1.0, c))
        return 1.055 * (c ** (1 / 2.4)) - 0.055 if c > 0.0031308 else 12.92 * c

    return (gamma(r), gamma(g), gamma(b))


C1 = tuple(int(round(v * 255)) for v in oklch_to_srgb(*C1_OKLCH))
C2 = tuple(int(round(v * 255)) for v in oklch_to_srgb(*C2_OKLCH))
print(f"brand gradient: {C1_OKLCH} -> #{C1[0]:02x}{C1[1]:02x}{C1[2]:02x}  "
      f"{C2_OKLCH} -> #{C2[0]:02x}{C2[1]:02x}{C2[2]:02x}")


# ---------------------------------------------------------------------------
# 母版绘制(1024 x 1024)
# ---------------------------------------------------------------------------

SIZE = 1024
RADIUS = int(SIZE * 0.22)  # 圆角半径

# 播放三角:左边竖直边,右侧尖角,几何中心在画布中心
TRI_W = 330   # 水平宽度
TRI_H = 310   # 垂直高度
CX = CY = SIZE // 2
TRI_X0 = CX - TRI_W // 3        # 竖直边 x
TRI_Y0 = CY - TRI_H // 2        # 竖直边顶部 y
TRI_Y1 = CY + TRI_H // 2        # 竖直边底部 y
TRI_X1 = TRI_X0 + TRI_W         # 尖角 x


def rounded_rect_mask(size, radius):
    """返回 size 尺寸的圆角矩形 alpha 掩码(用于背景裁剪)。"""
    mask = Image.new("L", size, 0)
    d = ImageDraw.Draw(mask)
    d.rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius=radius, fill=255)
    return mask


def make_master(rounded=True):
    """绘制 1024 母版。rounded=False 时背景铺满全幅(macOS icns 用)。"""
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))

    # --- 135deg 渐变背景(左上 -> 右下)---
    for y in range(SIZE):
        for x in range(SIZE):
            t = (x + y) / (2 * (SIZE - 1))
            t = max(0.0, min(1.0, t))
            r = int(C1[0] + (C2[0] - C1[0]) * t)
            g = int(C1[1] + (C2[1] - C1[1]) * t)
            b = int(C1[2] + (C2[2] - C1[2]) * t)
            img.putpixel((x, y), (r, g, b, 255))

    # --- 左上柔和高光(两层高斯模糊椭圆)---
    glow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse([-SIZE * 0.35, -SIZE * 0.45, SIZE * 0.55, SIZE * 0.45], fill=(255, 255, 255, 110))
    gd.ellipse([-SIZE * 0.10, -SIZE * 0.15, SIZE * 0.22, SIZE * 0.12], fill=(255, 255, 255, 70))
    glow = glow.filter(ImageFilter.GaussianBlur(SIZE * 0.09))
    img = Image.alpha_composite(img, glow)

    # --- 白色播放三角 ---
    tri = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    td = ImageDraw.Draw(tri)
    td.polygon(
        [(TRI_X0, TRI_Y0), (TRI_X0, TRI_Y1), (TRI_X1, CY)],
        fill=(255, 255, 255, 255),
    )
    img = Image.alpha_composite(img, tri)

    if rounded:
        img.putalpha(rounded_rect_mask((SIZE, SIZE), RADIUS))
    return img


def render(size, rounded=True):
    """从母版缩放到目标尺寸(超采样保证边缘平滑)。"""
    master = make_master(rounded=rounded)
    return master.resize((size, size), Image.LANCZOS)


# ---------------------------------------------------------------------------
# 输出
# ---------------------------------------------------------------------------

def main():
    out_dir = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
    out_dir = os.path.abspath(out_dir)
    os.makedirs(out_dir, exist_ok=True)

    # PNG(圆角版)
    render(32).save(os.path.join(out_dir, "32x32.png"))
    render(128).save(os.path.join(out_dir, "128x128.png"))
    render(256).save(os.path.join(out_dir, "128x128@2x.png"))
    print("PNG written")

    # ICO(圆角版,多尺寸)
    ico_sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    ico = render(256)
    ico.save(
        os.path.join(out_dir, "icon.ico"),
        format="ICO",
        sizes=ico_sizes,
    )
    print("ICO written")

    # ICNS(macOS:全方形,系统自动磨圆)
    icns = render(SIZE, rounded=False)
    icns.save(
        os.path.join(out_dir, "icon.icns"),
        format="ICNS",
        sizes=[(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (SIZE, SIZE)],
    )
    print("ICNS written")

    # ASCII 预览(便于人工检查形状)
    preview = make_master(rounded=False).resize((64, 64), Image.LANCZOS).convert("L")
    chars = " .:-=+*#%@"
    print("\n--- preview (64x64) ---")
    for y in range(64):
        row = ""
        for x in range(64):
            v = preview.getpixel((x, y))
            row += chars[min(9, v * 10 // 256)]
        print(row)


if __name__ == "__main__":
    sys.exit(main())
