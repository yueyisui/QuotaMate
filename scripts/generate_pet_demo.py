"""Generate the README animation for QuotaMate's 5-hour-quota desktop pet."""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs" / "images" / "pet-energy-demo.gif"
WIDTH, HEIGHT = 800, 360


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "seguisb.ttf" if bold else "segoeui.ttf"
    return ImageFont.truetype(str(Path("C:/Windows/Fonts") / name), size)


TITLE = font(25, True)
LARGE = font(42, True)
LABEL = font(18, True)
SMALL = font(14)


def lerp(a: float, b: float, amount: float) -> float:
    return a + (b - a) * amount


def mix_color(first: tuple[int, int, int], second: tuple[int, int, int], amount: float):
    return tuple(round(lerp(a, b, amount)) for a, b in zip(first, second))


def state_for(energy: float):
    if energy >= 75:
        return "FULLY CHARGED", (79, 124, 255)
    if energy >= 45:
        return "DOING WELL", (38, 185, 135)
    if energy >= 20:
        return "LOW ENERGY", (239, 166, 58)
    return "NEEDS CHARGING", (237, 91, 104)


def draw_fox(draw: ImageDraw.ImageDraw, cx: int, cy: int, energy: float, phase: float):
    bob = round(math.sin(phase) * 5)
    cy += bob
    purple = (121, 104, 238)
    pale = (244, 241, 255)
    ink = (37, 33, 58)
    outline = (228, 222, 255)

    # Tail keeps moving even when energy is low, but with a smaller swing.
    swing = math.sin(phase * 1.35) * (13 if energy >= 45 else 6)
    tail_x = cx - 81 + round(swing)
    draw.ellipse((tail_x - 23, cy + 37, tail_x + 53, cy + 105), fill=purple, outline=outline, width=5)
    draw.arc((tail_x - 16, cy + 45, tail_x + 49, cy + 98), 18, 145, fill=pale, width=13)

    # Ears and head.
    draw.polygon([(cx - 61, cy - 43), (cx - 52, cy - 105), (cx - 9, cy - 70)], fill=purple, outline=outline)
    draw.line([(cx - 61, cy - 43), (cx - 52, cy - 105), (cx - 9, cy - 70)], fill=outline, width=5, joint="curve")
    draw.polygon([(cx + 61, cy - 43), (cx + 52, cy - 105), (cx + 9, cy - 70)], fill=purple, outline=outline)
    draw.line([(cx + 61, cy - 43), (cx + 52, cy - 105), (cx + 9, cy - 70)], fill=outline, width=5, joint="curve")
    draw.polygon([(cx - 49, cy - 55), (cx - 47, cy - 88), (cx - 24, cy - 66)], fill=(184, 173, 255))
    draw.polygon([(cx + 49, cy - 55), (cx + 47, cy - 88), (cx + 24, cy - 66)], fill=(184, 173, 255))
    draw.ellipse((cx - 66, cy - 73, cx + 66, cy + 62), fill=purple, outline=outline, width=5)
    draw.polygon([(cx - 53, cy + 3), (cx, cy + 67), (cx + 53, cy + 3), (cx + 40, cy + 50), (cx, cy + 72), (cx - 40, cy + 50)], fill=pale)

    blink = energy >= 20 and math.sin(phase * 2.3) > 0.94
    if energy < 20:
        for offset in (-28, 28):
            draw.line((cx + offset - 7, cy - 19, cx + offset + 7, cy - 5), fill=ink, width=5)
            draw.line((cx + offset + 7, cy - 19, cx + offset - 7, cy - 5), fill=ink, width=5)
    elif blink:
        draw.line((cx - 36, cy - 10, cx - 20, cy - 10), fill=ink, width=5)
        draw.line((cx + 20, cy - 10, cx + 36, cy - 10), fill=ink, width=5)
    elif energy < 45:
        draw.ellipse((cx - 34, cy - 20, cx - 24, cy - 10), fill=ink)
        draw.ellipse((cx + 24, cy - 20, cx + 34, cy - 10), fill=ink)
    else:
        draw.arc((cx - 40, cy - 24, cx - 18, cy - 2), 195, 345, fill=ink, width=5)
        draw.arc((cx + 18, cy - 24, cx + 40, cy - 2), 195, 345, fill=ink, width=5)

    draw.polygon([(cx - 7, cy + 10), (cx + 7, cy + 10), (cx, cy + 19)], fill=ink)
    if energy >= 45:
        draw.arc((cx - 22, cy + 9, cx + 22, cy + 43), 15, 165, fill=ink, width=5)
    elif energy >= 20:
        draw.line((cx - 12, cy + 32, cx + 12, cy + 32), fill=ink, width=5)
    else:
        draw.arc((cx - 20, cy + 25, cx + 20, cy + 51), 195, 345, fill=ink, width=5)

    # Collar meter.
    _, color = state_for(energy)
    meter = (cx - 37, cy + 57, cx + 37, cy + 77)
    draw.rounded_rectangle(meter, radius=9, fill=(236, 233, 249), outline=(255, 255, 255), width=2)
    fill_width = max(5, round(66 * energy / 100))
    draw.rounded_rectangle((cx - 33, cy + 61, cx - 33 + fill_width, cy + 73), radius=6, fill=color)

    if energy >= 75:
        sparkle = 3 + round((math.sin(phase * 2) + 1) * 2)
        for x, y in [(cx - 84, cy - 44), (cx + 84, cy - 25), (cx + 69, cy + 50)]:
            draw.ellipse((x - sparkle, y - sparkle, x + sparkle, y + sparkle), fill=(208, 198, 255))


def frame(energy: float, phase: float) -> Image.Image:
    image = Image.new("RGB", (WIDTH, HEIGHT), (19, 18, 29))
    draw = ImageDraw.Draw(image)

    # Soft purple backdrop.
    for radius in range(260, 20, -12):
        alpha = (260 - radius) / 260
        color = mix_color((34, 28, 62), (69, 49, 122), alpha * 0.22)
        draw.ellipse((35 - radius, 170 - radius, 35 + radius, 170 + radius), fill=color)
    draw.rounded_rectangle((24, 22, WIDTH - 24, HEIGHT - 22), radius=27, fill=(29, 28, 42), outline=(69, 65, 86), width=2)

    draw.text((55, 45), "THE PET FEELS YOUR 5H QUOTA", font=TITLE, fill=(244, 241, 251))
    draw.text((55, 82), "It moves, reacts, and changes expression as your quota runs down.", font=SMALL, fill=(167, 162, 180))

    draw_fox(draw, 245, 205, energy, phase)

    label, color = state_for(energy)
    draw.text((435, 126), "5 HOUR QUOTA", font=SMALL, fill=(168, 162, 181))
    draw.text((432, 148), f"{round(energy)}%", font=LARGE, fill=(246, 243, 251))
    draw.rounded_rectangle((435, 211, 720, 226), radius=8, fill=(72, 69, 86))
    draw.rounded_rectangle((435, 211, 435 + max(5, round(285 * energy / 100)), 226), radius=8, fill=color)
    draw.rounded_rectangle((435, 251, 435 + draw.textlength(label, font=LABEL) + 30, 286), radius=12, fill=mix_color((39, 37, 52), color, 0.2))
    draw.text((450, 258), label, font=LABEL, fill=color)
    draw.text((435, 307), "Animated fox · custom PNG / WebP / GIF", font=SMALL, fill=(157, 151, 171))
    return image


def main() -> None:
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    keyframes = [98, 62, 28, 8, 28, 62, 98]
    frames: list[Image.Image] = []
    steps = 7
    for segment, (start, end) in enumerate(zip(keyframes, keyframes[1:])):
        for step in range(steps):
            progress = step / steps
            eased = (1 - math.cos(progress * math.pi)) / 2
            energy = lerp(start, end, eased)
            frames.append(frame(energy, (segment * steps + step) * 0.42))
    frames.extend(frame(98, (len(frames) + step) * 0.42) for step in range(8))
    frames[0].save(
        OUTPUT,
        save_all=True,
        append_images=frames[1:],
        duration=95,
        loop=0,
        optimize=True,
    )
    print(f"Generated {OUTPUT} ({OUTPUT.stat().st_size} bytes, {len(frames)} frames)")


if __name__ == "__main__":
    main()
