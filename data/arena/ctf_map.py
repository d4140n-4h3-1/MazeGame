# Builds data/arena/ctf_map.glb, a capture-the-flag arena, in Blender:
#
#     blender -b --python data/arena/ctf_map.py
#
# Two bases face each other down the long (x) axis, red at -x and cyan at +x, each a walled room
# with its flag in the middle - put there by the game, in a firewall, at the empty `flag_red` or
# `flag_blue`, with the computer that opens it at `computer_red` or `computer_blue` - and three
# ways in: the front door onto the yard, and a door in each
# side wall off a back corridor. Between them the field is split into three lanes by two long
# walls with doorways through them: an open middle lane round a central tower, and a narrower
# lane down each side. Everything is mirrored end to end, so neither side has the better of it.
#
# The floor is flat at 0, under a ceiling at 5 m; surfaces in pure magenta are light fixtures.

import math
import os

import bpy

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "ctf_map.glb")

HALF_X, HALF_Y = 42.0, 26.0  # inside the outer walls
CEILING = 5.0
WALL = 0.6  # inner walls' thickness
LOW, HIGH = 1.0, 1.8  # cover blocks' heights: crouch behind, stand behind

bpy.ops.wm.read_factory_settings(use_empty=True)


def material(name, rgb, metallic=0.0, roughness=0.5):
    m = bpy.data.materials.new(name)
    m.diffuse_color = (*rgb, 1.0)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    return m


MATS = {
    "Ground": material("Ground", (0.3, 0.32, 0.28)),
    "Wall": material("Wall", (0.25, 0.25, 0.28)),
    "Ceiling": material("Ceiling", (0.2, 0.2, 0.22)),
    "BlockLow": material("BlockLow", (0.6, 0.45, 0.25)),
    "BlockHigh": material("BlockHigh", (0.3, 0.4, 0.55)),
    "Pillar": material("Pillar", (0.55, 0.55, 0.58)),
    "TeamRed": material("TeamRed", (0.8, 0.07, 0.05)),
    "TeamCyan": material("TeamCyan", (0.0, 0.75, 0.85)),
    "Pole": material("Pole", (0.7, 0.7, 0.72), metallic=1.0, roughness=0.3),
    "LightGlass": material("LightGlass", (1.0, 0.0, 1.0)),
}
OTHER_TEAM = {"TeamRed": "TeamCyan", "TeamCyan": "TeamRed"}

# Each group becomes one object: name -> list of meshes to join.
groups = {}


def add(group, obj, mat):
    obj.data.materials.append(MATS[mat])
    groups.setdefault(group, []).append(obj)


def box(group, x0, x1, y0, y1, z0, z1, mat):
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2))
    obj = bpy.context.active_object
    obj.scale = (abs(x1 - x0), abs(y1 - y0), abs(z1 - z0))
    add(group, obj, mat)


def hexagon(group, x, y, radius, z0, z1, mat):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=6, radius=radius, depth=z1 - z0, location=(x, y, (z0 + z1) / 2), rotation=(0, 0, math.pi / 6)
    )
    add(group, bpy.context.active_object, mat)


def marker(name, x, y, sx):
    empty = bpy.data.objects.new(name, None)
    empty.location = (x, y, 0.0)
    empty.rotation_euler = (0.0, 0.0, 0.0 if sx == 1 else math.pi)
    bpy.context.scene.collection.objects.link(empty)


def mirrored(put):
    """Calls put(sx, sy, team) for each quarter: sx and sy flip the half, team is its colour."""
    for sx in (1, -1):
        for sy in (1, -1):
            put(sx, sy, "TeamRed" if sx == 1 else "TeamCyan")


def ends(put):
    """Calls put(sx, team) once for each end: red's at -x, cyan's at +x."""
    for sx in (1, -1):
        put(sx, "TeamRed" if sx == 1 else "TeamCyan")


# The shell: floor, ceiling and outer walls. Everything in red's half is written at -x; mirroring
# with sx = -1 makes cyan's.
T = 0.5
box("Ground", -HALF_X - T, HALF_X + T, -HALF_Y - T, HALF_Y + T, -0.2, 0.0, "Ground")
box("Ground", -HALF_X - T, HALF_X + T, -HALF_Y - T, HALF_Y + T, CEILING, CEILING + 0.2, "Ceiling")
for s in (1, -1):
    box("Ground", -HALF_X - T, HALF_X + T, s * HALF_Y, s * (HALF_Y + T), 0.0, CEILING, "Wall")
    box("Ground", s * HALF_X, s * (HALF_X + T), -HALF_Y, HALF_Y, 0.0, CEILING, "Wall")


def wall_x(sx, x0, x1, y, mat="Wall"):
    """A full-height wall along x, at y, from x0 to x1 (red's side; flipped by sx)."""
    box("Ground", sx * x0, sx * x1, y - WALL / 2, y + WALL / 2, 0.0, CEILING, mat)


def wall_y(sx, x, y0, y1, mat="Wall"):
    box("Ground", sx * (x - WALL / 2), sx * (x + WALL / 2), y0, y1, 0.0, CEILING, mat)


# The lane walls, at y = +-10, with a doorway in the middle and one in each half.
LANE = 10.0
for sy in (1, -1):
    for x0, x1 in ((-24.0, -15.0), (-12.0, -1.5), (1.5, 12.0), (15.0, 24.0)):
        wall_x(1, x0, x1, sy * LANE)

# The bases: a room from the end wall to x = -30, 28 m across, with a 4 m door in front and a
# 3 m door in each side wall.
FRONT, SIDE = -30.0, 14.0


def base(sx, team):
    wall_y(sx, FRONT, -SIDE - WALL / 2, -2.0)
    wall_y(sx, FRONT, 2.0, SIDE + WALL / 2)
    for sy in (1, -1):
        wall_x(sx, -HALF_X, -37.0, sy * SIDE)
        wall_x(sx, -34.0, FRONT, sy * SIDE)
    # A band of the team's colour round the room at eye height, inside and out, and round each
    # doorway, so there is no mistaking whose base it is.
    band = (2.4, 2.8)
    for side in (-1, 1):
        f = FRONT + side * (WALL / 2 + 0.02)
        for y0, y1 in ((-SIDE, -2.0), (2.0, SIDE)):
            box("Ground", sx * (f - 0.02), sx * (f + 0.02), y0, y1, *band, team)
        for sy in (1, -1):
            y = sy * (SIDE + side * (WALL / 2 + 0.02))
            for x0, x1 in ((-HALF_X, -37.0), (-34.0, FRONT)):
                box("Ground", sx * x0, sx * x1, y - 0.02, y + 0.02, *band, team)
    box("Ground", sx * (-HALF_X + 0.02), sx * (-HALF_X - 0.02), -HALF_Y, HALF_Y, *band, team)
    for y in (-2.0, 2.0):  # the front door's jambs
        box("Ground", sx * (FRONT - WALL / 2 - 0.02), sx * (FRONT + WALL / 2 + 0.02), y - 0.1, y + 0.1, 0.0, CEILING, team)
    for sy in (1, -1):  # the side doors' jambs
        for x in (-37.0, -34.0):
            box("Ground", sx * (x - 0.1), sx * (x + 0.1), sy * (SIDE - WALL / 2 - 0.02), sy * (SIDE + WALL / 2 + 0.02), 0.0, CEILING, team)
    # The flag's pad on the floor: the capture point.
    bpy.ops.mesh.primitive_cylinder_add(vertices=24, radius=2.0, depth=0.02, location=(sx * -37.0, 0.0, 0.01))
    add("Ground", bpy.context.active_object, team)

    # Cover inside: low blocks between the front door and the flag, tall ones at the back to
    # spawn behind.
    for sy in (1, -1):
        box("CoverBlocks", sx * -33.5, sx * -32.5, sy * 4.0, sy * 7.0, 0.0, LOW, "BlockLow")
        box("CoverBlocks", sx * -41.0, sx * -39.0, sy * 8.0, sy * 10.0, 0.0, HIGH, "BlockHigh")
        hexagon("Pillars", sx * -37.0, sy * 9.0, 0.8, 0.0, CEILING, "Pillar")

    # Where the game puts the flag, in its firewall, and the computer that firewall answers to:
    # empties, each facing along its own +x - the flag's cloth towards the middle of the arena,
    # the computer's screen out from the back wall.
    side = "red" if team == "TeamRed" else "blue"
    marker(f"flag_{side}", sx * -37.0, 0.0, sx)
    marker(f"computer_{side}", sx * -(HALF_X - 0.8), -5.0, sx)


ends(base)


# The back corridors round each base's sides, and the yard in front of it.
def yards(sx, sy, team):
    box("CoverBlocks", sx * -28.0, sx * -26.0, sy * 5.5, sy * 6.5, 0.0, LOW, "BlockLow")
    hexagon("Pillars", sx * -27.0, sy * 19.0, 1.0, 0.0, CEILING, "Pillar")
    box("CoverBlocks", sx * -40.0, sx * -38.0, sy * 19.0, sy * 21.0, 0.0, HIGH, "BlockHigh")
    box("CoverBlocks", sx * -33.0, sx * -32.0, sy * 22.0, sy * 24.5, 0.0, LOW, "BlockLow")


mirrored(yards)


# The middle lane: a tower in the middle, with cover round it and along the lane.
hexagon("Pillars", 0.0, 0.0, 2.5, 0.0, CEILING, "Pillar")
for s in (1, -1):
    box("CoverBlocks", s * 4.5, s * 5.5, -1.5, 1.5, 0.0, LOW, "BlockLow")
    box("CoverBlocks", -1.5, 1.5, s * 6.5, s * 7.5, 0.0, HIGH, "BlockHigh")


def middle(sx, sy, team):
    box("CoverBlocks", sx * -10.5, sx * -9.5, sy * 3.0, sy * 5.5, 0.0, HIGH, "BlockHigh")
    hexagon("Pillars", sx * -20.0, sy * 6.0, 1.0, 0.0, CEILING, "Pillar")
    box("CoverBlocks", sx * -15.0, sx * -13.0, sy * 0.5, sy * 1.5, 0.0, LOW, "BlockLow")


mirrored(middle)


# The side lanes: narrower, with a low wall across the middle and pillars and blocks to dodge
# between.
for s in (1, -1):
    box("CoverBlocks", -3.0, 3.0, s * 17.6, s * 18.4, 0.0, LOW, "BlockLow")


def sides(sx, sy, team):
    hexagon("Pillars", sx * -18.0, sy * 18.0, 1.0, 0.0, CEILING, "Pillar")
    box("CoverBlocks", sx * -13.0, sx * -11.0, sy * 14.5, sy * 15.5, 0.0, LOW, "BlockLow")
    box("CoverBlocks", sx * -7.5, sx * -6.5, sy * 20.5, sy * 23.5, 0.0, HIGH, "BlockHigh")
    box("CoverBlocks", sx * -22.5, sx * -21.5, sy * 12.5, sy * 14.5, 0.0, HIGH, "BlockHigh")


mirrored(sides)

# Light strips under the ceiling, clear of every wall.
strips = []
for i, x in enumerate((-39.0, -34.0, -26.0, -18.0, -9.0, 0.0, 9.0, 18.0, 26.0, 34.0, 39.0)):
    for j, y in enumerate((-21.0, -5.0, 5.0, 21.0)):
        box(f"LightStrip_{i}_{j}", x - 2.0, x + 2.0, y - 0.25, y + 0.25, CEILING - 0.1, CEILING, "LightGlass")

# Join each group into one object, with its transforms applied.
for name, objs in groups.items():
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    if len(objs) > 1:
        bpy.ops.object.join()
    joined = bpy.context.active_object
    joined.name = name
    joined.data.name = name

bpy.ops.export_scene.gltf(filepath=OUT, export_format="GLB", export_apply=True, export_yup=True)
print("Wrote", OUT)
