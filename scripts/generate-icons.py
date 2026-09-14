"""Generate real icon assets for the VECTOR desktop app.

The committed files were 70-byte PNG placeholders saved under .ico/.icns
extensions, which Tauri rejects ("not in 3.00 format") and which would ship a
broken installer icon. This produces genuine, correctly-encoded assets.

Usage: python3 scripts/generate-icons.py
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ICON_DIR = Path(__file__).resolve().parent.parent / "apps" / "desktop" / "src-tauri" / "icons"

# VECTOR brand: a dark navy field with a bright forward vector.
BACKGROUND = (14, 22, 40, 255)
ACCENT = (64, 196, 255, 255)
ACCENT_DIM = (32, 120, 180, 255)


def render(size: int) -> Image.Image:
    """Render the mark at the given square size."""
    # Render at 4x then downsample for clean antialiased edges.
    scale = 4
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Rounded-square background.
    radius = int(s * 0.22)
    draw.rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=BACKGROUND)

    # Chevron/vector mark pointing up-right, drawn as a thick polyline.
    thickness = max(2, int(s * 0.09))
    pad = int(s * 0.26)
    tip = (s - pad, pad)
    left = (pad, int(s * 0.52))
    bottom = (int(s * 0.52), s - pad)

    draw.line([left, tip], fill=ACCENT, width=thickness, joint="curve")
    draw.line([tip, bottom], fill=ACCENT, width=thickness, joint="curve")
    # Inner echo line for depth.
    draw.line(
        [(int(s * 0.36), int(s * 0.58)), (int(s * 0.58), int(s * 0.36))],
        fill=ACCENT_DIM,
        width=max(1, thickness // 2),
    )

    return img.resize((size, size), Image.LANCZOS)


def write_png(path: Path, size: int) -> None:
    render(size).save(path, format="PNG")
    print(f"wrote {path.name} ({size}x{size})")


def write_ico(path: Path, sizes: list[int]) -> None:
    """Write a real multi-resolution ICO (format 3.00)."""
    base = render(max(sizes))
    base.save(path, format="ICO", sizes=[(s, s) for s in sizes])
    print(f"wrote {path.name} (ICO, sizes={sizes})")


def write_icns(path: Path, sizes: list[int]) -> None:
    """Write an ICNS container with PNG-encoded entries.

    ICNS is a simple container: a magic header, total length, then typed
    chunks. PNG payloads are permitted for the modern types, so this avoids
    needing an external encoder.
    """
    # ICNS type codes mapped to the pixel size they carry.
    type_codes = {
        16: b"icp4",
        32: b"icp5",
        64: b"icp6",
        128: b"ic07",
        256: b"ic08",
        512: b"ic09",
        1024: b"ic10",
    }

    chunks = []
    for size in sizes:
        code = type_codes.get(size)
        if code is None:
            raise ValueError(f"no ICNS type code for size {size}")
        import io

        buf = io.BytesIO()
        render(size).save(buf, format="PNG")
        payload = buf.getvalue()
        chunks.append(code + struct.pack(">I", len(payload) + 8) + payload)

    body = b"".join(chunks)
    data = b"icns" + struct.pack(">I", len(body) + 8) + body
    path.write_bytes(data)
    print(f"wrote {path.name} (ICNS, sizes={sizes})")


def main() -> int:
    ICON_DIR.mkdir(parents=True, exist_ok=True)

    write_png(ICON_DIR / "32x32.png", 32)
    write_png(ICON_DIR / "128x128.png", 128)
    write_png(ICON_DIR / "128x128@2x.png", 256)
    # Tauri's Windows bundler also looks for these standard names.
    write_png(ICON_DIR / "icon.png", 512)
    write_png(ICON_DIR / "Square150x150Logo.png", 150)
    write_png(ICON_DIR / "Square44x44Logo.png", 44)
    write_png(ICON_DIR / "StoreLogo.png", 50)

    write_ico(ICON_DIR / "icon.ico", [16, 32, 48, 64, 128, 256])
    write_icns(ICON_DIR / "icon.icns", [16, 32, 64, 128, 256, 512, 1024])

    return 0


if __name__ == "__main__":
    sys.exit(main())
