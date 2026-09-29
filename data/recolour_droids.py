#!/usr/bin/env python3
"""Make the maze's other droids from the player's: data/droid_full_deform.glb with the base
colours of its glowing materials changed, and nothing else - the same mesh, rig and animations.
Run it again whenever the player's droid is exported afresh, so every droid has its clips.

    python3 data/recolour_droids.py
"""
import json
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(HERE, "droid_full_deform.glb")
# Each droid's colour for each material it changes, by material name, as RGBA.
LIVERIES = {
    "droid_explorer.glb": {
        "Material.003": [0, 1, 0, 1],
        "Material.004": [0, 1, 0, 1],
    },
    "droid_hostile.glb": {
        "Material.003": [1, 0, 0, 1],
        "Material.004": [1, 0, 0, 1],
    },
    "droid_maintenance.glb": {
        "Material.003": [0.9734454154968262, 0.07036012411117554, 0.002428215928375721, 1],
        "Material.004": [0.9734454154968262, 0.07036012411117554, 0.002428215928375721, 1],
    },
    "droid_security.glb": {
        "Material.003": [0, 0, 1, 1],
    },
}


def read(path):
    data = open(path, "rb").read()
    magic, version, _ = struct.unpack("<4sII", data[:12])
    assert magic == b"glTF" and version == 2, path
    length, kind = struct.unpack("<I4s", data[12:20])
    assert kind == b"JSON", path
    gltf = json.loads(data[20 : 20 + length])
    # The rest - the binary chunk - is kept as it is.
    return gltf, data[20 + length :]


def write(path, gltf, rest):
    text = json.dumps(gltf, separators=(",", ":")).encode()
    text += b" " * (-len(text) % 4)
    chunk = struct.pack("<I4s", len(text), b"JSON") + text
    with open(path, "wb") as out:
        out.write(struct.pack("<4sII", b"glTF", 2, 12 + len(chunk) + len(rest)))
        out.write(chunk)
        out.write(rest)


def main():
    gltf, rest = read(SOURCE)
    for name, colours in LIVERIES.items():
        materials = json.loads(json.dumps(gltf["materials"]))
        for material in materials:
            if material.get("name") in colours:
                pbr = material.setdefault("pbrMetallicRoughness", {})
                pbr["baseColorFactor"] = colours[material["name"]]
        missing = set(colours) - {m.get("name") for m in materials}
        assert not missing, f"{name}: no {missing}"
        write(os.path.join(HERE, name), {**gltf, "materials": materials}, rest)
        print(f"{name}: {', '.join(sorted(colours))}")


if __name__ == "__main__":
    main()
