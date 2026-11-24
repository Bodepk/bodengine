"""Genera los assets sintéticos de la demo sin dependencias externas."""

from __future__ import annotations

import json
import math
import struct
import zlib
from pathlib import Path

ASSETS = Path(__file__).resolve().parents[1] / "assets"


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def checker_png() -> bytes:
    width = height = 2
    pixels = bytearray()
    for y in range(height):
        pixels.append(0)  # filter: none
        for x in range(width):
            if (x + y) % 2 == 0:
                pixels.extend((64, 220, 180, 255))
            else:
                pixels.extend((20, 80, 120, 255))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(bytes(pixels)))
        + png_chunk(b"IEND", b"")
    )


def beep_wav() -> bytes:
    sample_rate = 8000
    samples = int(sample_rate * 0.12)
    pcm = bytearray()
    for i in range(samples):
        envelope = 1.0 - i / samples
        value = int(12000 * envelope * math.sin(2 * math.pi * 440 * i / sample_rate))
        pcm.extend(struct.pack("<h", value))
    header = b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVE"
    header += b"fmt " + struct.pack("<IHHIIHH", 16, 1, 1, sample_rate,
                                    sample_rate * 2, 2, 16)
    header += b"data" + struct.pack("<I", len(pcm))
    return header + bytes(pcm)


def triangle_glb() -> bytes:
    positions = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)]
    binary = bytearray()
    for x, y, z in positions:
        binary.extend(struct.pack("<fff", x, y, z))
    binary.extend(struct.pack("<HHH", 0, 1, 2))

    document = {
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"name": "Demo", "nodes": [0]}],
        "nodes": [{"name": "Triangle", "mesh": 0}],
        "meshes": [{
            "name": "DemoMesh",
            "primitives": [{
                "attributes": {"POSITION": 0},
                "indices": 1,
                "material": 0,
                "mode": 4,
            }],
        }],
        "materials": [{
            "name": "DemoMaterial",
            "alphaMode": "OPAQUE",
            "pbrMetallicRoughness": {"baseColorFactor": [0.25, 0.86, 0.7, 1.0]},
        }],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0],
            },
            {
                "bufferView": 1,
                "componentType": 5123,
                "count": 3,
                "type": "SCALAR",
            },
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 6},
        ],
        "buffers": [{"byteLength": len(binary)}],
    }

    json_chunk = json.dumps(document, separators=(",", ":")).encode()
    json_chunk += b" " * (-len(json_chunk) % 4)
    binary.extend(b"\0" * (-len(binary) % 4))
    length = 12 + 8 + len(json_chunk) + 8 + len(binary)
    return (
        struct.pack("<III", 0x46546C67, 2, length)
        + struct.pack("<II", len(json_chunk), 0x4E4F534A)
        + json_chunk
        + struct.pack("<II", len(binary), 0x004E4942)
        + bytes(binary)
    )


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    (ASSETS / "checker.png").write_bytes(checker_png())
    (ASSETS / "beep.wav").write_bytes(beep_wav())
    (ASSETS / "triangle.glb").write_bytes(triangle_glb())
    for name in ("checker.png", "beep.wav", "triangle.glb"):
        print((ASSETS / name).name)


if __name__ == "__main__":
    main()
