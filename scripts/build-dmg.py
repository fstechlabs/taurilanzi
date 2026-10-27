# /// script
# requires-python = ">=3.12"
# dependencies = ["dmgbuild==1.6.7", "pillow==13.0.0"]
# ///
"""Packs the .app into a .dmg that opens on the app, an arrow and Applications, ready to drag.

The bundler's own .dmg lays its window out through Finder, which CI and a locked screen cannot
do; dmgbuild writes the layout itself. The background is drawn here, so the repository carries
no image for it.

Usage: uv run scripts/build-dmg.py <Taurilanzi.app> <output.dmg> <volume icon.icns>
"""

import sys
import tempfile
from pathlib import Path

import dmgbuild
from PIL import Image, ImageDraw

WIDTH, HEIGHT = 640, 400
ICON_SIZE = 128
# window_rect is the whole window: its content, where the background goes, sits below the title bar.
TITLE_BAR = 31
APP_AT = (170, 185)
APPLICATIONS_AT = (470, 185)
# Finder draws the labels in black, dark mode included: a mid-grey keeps them readable and still
# looks like the app.
TOP, BOTTOM = (104, 104, 108), (82, 82, 86)
ARROW = (255, 255, 255, 150)
# Drawn this many times larger and scaled down, for smooth edges.
SUPERSAMPLE = 4


def draw_background(path: Path, scale: int) -> None:
    size = (WIDTH * scale * SUPERSAMPLE, HEIGHT * scale * SUPERSAMPLE)
    unit = scale * SUPERSAMPLE
    image = Image.new("RGBA", size)
    draw = ImageDraw.Draw(image)
    for y in range(size[1]):
        t = y / (size[1] - 1)
        color = tuple(round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM))
        draw.line([(0, y), (size[0], y)], fill=(*color, 255))

    arrow = Image.new("RGBA", size)
    pen = ImageDraw.Draw(arrow)
    y = APP_AT[1] * unit
    start = (APP_AT[0] + ICON_SIZE // 2 + 30) * unit
    tip = (APPLICATIONS_AT[0] - ICON_SIZE // 2 - 30) * unit
    head = 22 * unit
    pen.rounded_rectangle(
        [start, y - 3 * unit, tip - head // 2, y + 3 * unit], radius=3 * unit, fill=ARROW
    )
    pen.polygon([(tip - head, y - 14 * unit), (tip, y), (tip - head, y + 14 * unit)], fill=ARROW)
    image.alpha_composite(arrow)

    final = image.resize((WIDTH * scale, HEIGHT * scale), Image.Resampling.LANCZOS)
    final.convert("RGB").save(path, dpi=(72 * scale, 72 * scale))


def main() -> None:
    app, dmg, icon = (Path(arg) for arg in sys.argv[1:4])
    with tempfile.TemporaryDirectory() as tmp:
        background = Path(tmp) / "background.png"
        draw_background(background, 1)
        draw_background(Path(tmp) / "background@2x.png", 2)
        dmgbuild.build_dmg(
            str(dmg),
            "Taurilanzi",
            settings={
                "format": "UDZO",
                "files": [str(app)],
                "symlinks": {"Applications": "/Applications"},
                "icon": str(icon),
                "background": str(background),
                "window_rect": ((200, 160), (WIDTH, HEIGHT + TITLE_BAR)),
                "default_view": "icon-view",
                "show_status_bar": False,
                "show_tab_view": False,
                "show_toolbar": False,
                "show_pathbar": False,
                "show_sidebar": False,
                "icon_size": ICON_SIZE,
                "text_size": 13,
                "icon_locations": {app.name: APP_AT, "Applications": APPLICATIONS_AT},
                "hide_extensions": [app.name],
            },
        )


if __name__ == "__main__":
    main()
