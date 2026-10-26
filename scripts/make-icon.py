"""生成 1024x1024 应用图标：蓝色渐变圆角方块 + 白色菱形（对应界面里的 logo）。"""
from PIL import Image, ImageDraw

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

# 渐变背景
grad = Image.new("RGBA", (S, S))
px = grad.load()
c1 = (86, 141, 251)
c2 = (47, 111, 237)
for y in range(S):
    for x in range(S):
        t = (x + y) / (2 * S - 2)
        px[x, y] = (
            int(c1[0] + (c2[0] - c1[0]) * t),
            int(c1[1] + (c2[1] - c1[1]) * t),
            int(c1[2] + (c2[2] - c1[2]) * t),
            255,
        )

# 圆角遮罩
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=230, fill=255)
img.paste(grad, (0, 0), mask)

d = ImageDraw.Draw(img)
cx, cy = S // 2, S // 2
outer = 300
inner = 176


def diamond(r):
    return [(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)]


# 白色菱形外框
d.polygon(diamond(outer), fill=(255, 255, 255, 255))
# 挖空内部形成描边
d.polygon(diamond(inner), fill=None, outline=None)
hole = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(hole).polygon(diamond(inner), fill=(47, 111, 237, 255))
img.alpha_composite(hole)

# 中间小实心菱形，形成「◇ + ◆」的层次
d = ImageDraw.Draw(img)
d.polygon(diamond(88), fill=(255, 255, 255, 255))

img.save("icon-src.png")
print("icon-src.png 已生成", img.size)
