"""Draw the 1024x1024 source icon: a rounded teal square with a waveform.
`cargo tauri icon icons/source.png` makes every size the bundles need.

    python icons/make_icon.py icons/source.png
"""
import math
import sys

from PIL import Image, ImageDraw

N = 1024
SS = 4  # supersample, then scale down for smooth edges


def main():
    size = N * SS
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    top, bottom = (20, 160, 150), (8, 90, 100)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([80 * SS, 80 * SS, 944 * SS, 944 * SS],
                                           radius=190 * SS, fill=255)
    grad = Image.new("RGBA", (size, size))
    gd = ImageDraw.Draw(grad)
    for y in range(size):
        t = y / size
        gd.line([(0, y), (size, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)))
    img.paste(grad, (0, 0), mask)

    # a speech-like waveform: bars under a raised-cosine envelope
    bars = 23
    x0, x1 = 210 * SS, 814 * SS
    step = (x1 - x0) / (bars - 1)
    width = int(step * 0.55)
    for i in range(bars):
        t = i / (bars - 1)
        env = 0.5 - 0.5 * math.cos(2 * math.pi * t)
        wobble = 0.55 + 0.45 * abs(math.sin(i * 2.3) * math.cos(i * 0.4))
        h = (40 + 300 * env * wobble) * SS
        cx = x0 + i * step
        d.rounded_rectangle([cx - width / 2, 512 * SS - h, cx + width / 2, 512 * SS + h],
                            radius=width / 2, fill=(255, 255, 255, 240))

    img.resize((N, N), Image.LANCZOS).save(sys.argv[1])


if __name__ == "__main__":
    main()
