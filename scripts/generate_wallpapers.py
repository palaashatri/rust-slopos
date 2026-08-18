#!/usr/bin/env python3
"""Generate original, retro SLOPOS wallpapers and patterns."""
import os
from PIL import Image, ImageDraw

OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets", "wallpapers")
os.makedirs(OUTPUT_DIR, exist_ok=True)

def generate_classic_dither(width=1920, height=1080):
    """Classic Macintosh 50% 1-bit dither pattern (#808080 / #FFFFFF checkerboard)."""
    img = Image.new("RGB", (width, height), color=(128, 128, 128))
    pixels = img.load()
    for y in range(height):
        for x in range(width):
            if (x + y) % 2 == 0:
                pixels[x, y] = (140, 140, 140)
            else:
                pixels[x, y] = (116, 116, 116)
    path = os.path.join(OUTPUT_DIR, "01_classic_system_gray.png")
    img.save(path, format="PNG")
    print(f"Saved: {path}")

def generate_platinum_slate(width=1920, height=1080):
    """Platinum cool slate #758090 with subtle fine grid."""
    img = Image.new("RGB", (width, height), color=(117, 128, 144))
    pixels = img.load()
    for y in range(height):
        for x in range(width):
            if x % 8 == 0 or y % 8 == 0:
                pixels[x, y] = (110, 120, 136)
            elif (x + y) % 4 == 0:
                pixels[x, y] = (122, 134, 150)
    path = os.path.join(OUTPUT_DIR, "02_platinum_cool_slate.png")
    img.save(path, format="PNG")
    print(f"Saved: {path}")

def generate_vintage_mac_blue(width=1920, height=1080):
    """Classic Mac OS 8/9 vintage blue tweed #3A5F8B."""
    img = Image.new("RGB", (width, height), color=(58, 95, 139))
    pixels = img.load()
    for y in range(height):
        for x in range(width):
            if (x % 16 < 8 and y % 16 < 8) or (x % 16 >= 8 and y % 16 >= 8):
                if (x + y) % 2 == 0:
                    pixels[x, y] = (52, 86, 128)
                else:
                    pixels[x, y] = (64, 104, 150)
            else:
                if (x + y) % 2 == 0:
                    pixels[x, y] = (46, 76, 115)
                else:
                    pixels[x, y] = (58, 95, 139)
    path = os.path.join(OUTPUT_DIR, "03_vintage_mac_blue.png")
    img.save(path, format="PNG")
    print(f"Saved: {path}")

def generate_retro_teal_grid(width=1920, height=1080):
    """90s retro desktop teal matrix #008080."""
    img = Image.new("RGB", (width, height), color=(0, 128, 128))
    pixels = img.load()
    for y in range(height):
        for x in range(width):
            if x % 16 == 0 or y % 16 == 0:
                pixels[x, y] = (0, 112, 112)
            elif x % 4 == 0 and y % 4 == 0:
                pixels[x, y] = (0, 144, 144)
    path = os.path.join(OUTPUT_DIR, "04_retro_teal_grid.png")
    img.save(path, format="PNG")
    print(f"Saved: {path}")

def generate_oled_pure_dark(width=1920, height=1080):
    """OLED pure black #000000 with minimalist star constellation / subtle grid."""
    img = Image.new("RGB", (width, height), color=(0, 0, 0))
    pixels = img.load()
    for y in range(height):
        for x in range(width):
            if x % 32 == 0 and y % 32 == 0:
                pixels[x, y] = (50, 50, 55)
            elif (x % 32 == 1 or x % 32 == 31) and y % 32 == 0:
                pixels[x, y] = (25, 25, 28)
            elif x % 32 == 0 and (y % 32 == 1 or y % 32 == 31):
                pixels[x, y] = (25, 25, 28)
    path = os.path.join(OUTPUT_DIR, "05_oled_pure_dark.png")
    img.save(path, format="PNG")
    print(f"Saved: {path}")

if __name__ == "__main__":
    generate_classic_dither()
    generate_platinum_slate()
    generate_vintage_mac_blue()
    generate_retro_teal_grid()
    generate_oled_pure_dark()
    print("All wallpapers successfully generated!")
