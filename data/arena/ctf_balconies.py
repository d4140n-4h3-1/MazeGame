"""
Capture-the-flag arena on two floors: two mirrored bases, each with its flag in an open well
under a U-shaped balcony, catwalks along both side walls joining the balconies, a raised centre
hub, stairs between the floors, and random cover mirrored so neither side gets a better layout.

Builds data/arena/ctf_balconies.glb (and a .blend to look it over, wherever --out puts it):

    blender -b --python data/arena/ctf_balconies.py -- --seed 7 --out /tmp/ctf_balconies.blend --glb data/arena/ctf_balconies.glb

Or open in Blender > Scripting tab > Run Script.

Layout (top view, x runs along the length, red base at -x and blue at +x):

    +------------------------------------------------------------------+
    |BB  balcony |================ catwalk (upper) ===========| balcony BB|
    |BB    stairs|   stairs                         stairs   |stairs   BB|
    |BB=====+    |                                           |    +=====BB|
    |B| post| C  |            stairs +-----+ stairs          |  C |post |B|
    |B|  F  | P  |   P               | hub |              P  |  P |  F  |B|
    |BB=====+    |                   +-----+                 |    +=====BB|
    |BB    stairs|   stairs                         stairs   |stairs   BB|
    |BB  balcony |================ catwalk (upper) ===========| balcony BB|
    +------------------------------------------------------------------+

Made for Ruptura Systematis (MazeGame/maze), whose capture the flag reads the map's empties: the
game puts each side's flag in a firewall at `flag_red` / `flag_blue`, the computer that opens it
at `computer_red` / `computer_blue`, and its droids at `post_red_1` and on. Its droids walk only
the ground floor, so the flags and posts are all down there; the balconies, catwalks and hub are
the high ground over them. As the game needs, there is a ceiling over everything and the light
fixtures' glass is pure magenta. The lamps behind the glass reach 7.5 m, so the spaces under the
upper floors get strips of their own.
"""

import bpy
import math
import random
import sys
import os

LENGTH = 80.0        # hall length (x), base to base
WIDTH = 44.0         # hall width (y)
CEILING = 7.0        # hall height (lamps reach 7.5 m, so ceiling light still reaches the ground)
UPPER = 3.5          # height of the upper floor: balconies and catwalks
SLAB = 0.3           # upper floor thickness
BASE_DEPTH = 12.0    # how far each base reaches in from its end wall
BACK = 3.0           # depth of the balcony along the end wall
WELL = 10.0          # half-width of the open well the flag stands in
CATWALK = 3.5        # catwalk width along the side walls
HUB = 10.0           # centre hub side length
HUB_HEIGHT = 1.75
STEP_RISE = 0.25
STEP_RUN = 0.35
STAIR_WIDTH = 2.5
RAIL = 1.0           # railing height (low cover)
NUM_COVER = 14       # cover blocks per half (mirrored to the other half)
MIN_GAP = 2.0        # walkable space kept around cover
LIGHT_SPACING = 8.0


def get_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else random.randrange(1_000_000)
    glb = argv[argv.index("--glb") + 1] if "--glb" in argv else None
    out = argv[argv.index("--out") + 1] if "--out" in argv else None
    return seed, glb, out


def material(name, rgb):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*rgb, 1)
    m.diffuse_color = (*rgb, 1)
    return m


def collection(name):
    c = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(c)
    return c


def move_to(obj, col):
    for c in obj.users_collection:
        c.objects.unlink(obj)
    col.objects.link(obj)


def add_block(name, x, y, w, d, h, rot, col, mat, cover, z=0.0):
    """A box whose bottom sits at height z."""
    bpy.ops.mesh.primitive_cube_add(size=1, location=(x, y, z + h / 2))
    o = bpy.context.active_object
    o.name = name
    o.scale = (w, d, h)
    o.rotation_euler.z = rot
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    o.data.materials.append(mat)
    o["cover"] = cover
    move_to(o, col)
    return o


def add_box(name, x0, x1, y0, y1, z0, z1, col, mat, cover="none"):
    """An axis-aligned box given by its extents, in either order."""
    x0, x1 = sorted((x0, x1))
    y0, y1 = sorted((y0, y1))
    return add_block(name, (x0 + x1) / 2, (y0 + y1) / 2, x1 - x0, y1 - y0, z1 - z0, 0, col, mat, cover, z=z0)


def add_stairs(name, x, y, direction, rise, col, mat):
    """Solid stairs whose top step meets height `rise` at (x, y) and that descend toward
    `direction` ('+x', '-x', '+y' or '-y'). Returns the footprint (x0, x1, y0, y1)."""
    n = round(rise / STEP_RISE)
    sign = 1 if direction[0] == "+" else -1
    along_x = direction[1] == "x"
    w = STAIR_WIDTH / 2
    for i in range(n):
        top = rise - i * STEP_RISE                      # step i is i steps down from the top
        a, b = i * STEP_RUN * sign, (i + 1) * STEP_RUN * sign
        if along_x:
            add_box(f"{name}_{i}", x + a, x + b, y - w, y + w, 0, top, col, mat)
        else:
            add_box(f"{name}_{i}", x - w, x + w, y + a, y + b, 0, top, col, mat)
    run = n * STEP_RUN * sign
    if along_x:
        return (*sorted((x, x + run)), y - w, y + w)
    return (x - w, x + w, *sorted((y, y + run)))


def add_rail(name, a0, a1, at, along_x, gaps, col, mat):
    """A railing on the upper floor from a0 to a1 along x (at y = `at`) or along y (at x = `at`),
    broken at each (gap_start, gap_end)."""
    edges = [a0] + [g for gap in sorted(gaps) for g in gap] + [a1]
    for i in range(0, len(edges), 2):
        if edges[i + 1] - edges[i] > 0.1:
            span = (edges[i], edges[i + 1])
            x0, x1, y0, y1 = (*span, at - 0.1, at + 0.1) if along_x else (at - 0.1, at + 0.1, *span)
            add_box(f"{name}_{i // 2}", x0, x1, y0, y1, UPPER, UPPER + RAIL, col, mat, "low")


def marker(name, x, y, facing_x):
    """An empty where the game puts something, its +x turned to point along `facing_x` (+1 or -1)."""
    empty = bpy.data.objects.new(name, None)
    empty.location = (x, y, 0.0)
    empty.rotation_euler.z = 0.0 if facing_x > 0 else math.pi
    bpy.context.scene.collection.objects.link(empty)


def main():
    seed, glb, out = get_args()
    rng = random.Random(seed)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    hx, hy = LENGTH / 2, WIDTH / 2
    edge = hx - BASE_DEPTH                  # |x| of each base's front edge
    back = hx - BACK                        # |x| of the back balcony's inner edge
    cw = hy - CATWALK                       # |y| of each catwalk's inner edge
    under = UPPER - SLAB                    # underside of the upper floor

    col_hall = collection("Hall")
    col_struct = collection("Structures")
    col_cover = collection("Cover")
    col_lights = collection("Lights")

    mat_ground = material("Ground", (0.30, 0.32, 0.28))
    mat_wall = material("Wall", (0.25, 0.25, 0.28))
    mat_ceiling = material("Ceiling", (0.20, 0.20, 0.22))
    mat_floor = material("UpperFloor", (0.45, 0.45, 0.48))
    mat_stairs = material("Stairs", (0.50, 0.48, 0.42))
    mat_rail = material("Railing", (0.35, 0.35, 0.38))
    mat_low = material("CoverLow", (0.60, 0.45, 0.25))
    mat_high = material("CoverHigh", (0.40, 0.42, 0.45))
    mat_glass = material("LightGlass", (1.0, 0.0, 1.0))  # the game's marker for fixture glass
    # the game's own team colours: blue's droids are cyan
    mat_team = {"red": material("TeamRed", (0.80, 0.07, 0.05)),
                "blue": material("TeamCyan", (0.00, 0.75, 0.85))}

    # hall: floor top at exactly 0 (the game keeps a safety floor of its own just below)
    add_box("Ground", -hx, hx, -hy, hy, -0.2, 0, col_hall, mat_ground)
    for i, (x0, x1, y0, y1) in enumerate([(-hx - 1, hx + 1, hy, hy + 1), (-hx - 1, hx + 1, -hy - 1, -hy),
                                          (hx, hx + 1, -hy, hy), (-hx - 1, -hx, -hy, hy)]):
        add_box(f"Wall_{i}", x0, x1, y0, y1, 0, CEILING, col_hall, mat_wall, "high")
    add_box("Ceiling", -hx - 1, hx + 1, -hy - 1, hy + 1, CEILING, CEILING + 0.3, col_hall, mat_ceiling)

    # footprints on the ground that random cover must stay clear of: (x0, x1, y0, y1)
    keep_clear = []

    # catwalks along both side walls, between the two bases' balconies
    for side, sy in (("N", 1), ("S", -1)):
        add_box(f"Catwalk_{side}", -edge, edge, sy * cw, sy * hy, under, UPPER, col_struct, mat_floor)
        add_box(f"CatwalkGlass_{side}", -edge, edge, sy * (cw + 1.0), sy * (hy - 1.0), under - 0.1, under,
                col_lights, mat_glass)
        gaps = []
        for sx in (-1, 1):
            x = sx * edge / 2
            keep_clear.append(add_stairs(f"CatwalkStairs_{side}{'W' if sx < 0 else 'E'}", x, sy * cw,
                                         "-y" if sy > 0 else "+y", UPPER, col_struct, mat_stairs))
            gaps.append((x - STAIR_WIDTH / 2, x + STAIR_WIDTH / 2))
        add_rail(f"CatwalkRail_{side}", -edge, edge, sy * (cw + 0.1), True, gaps, col_struct, mat_rail)
        keep_clear.append((-edge, edge, *sorted((sy * cw, sy * hy))))

    # bases: the flag on the ground in an open well, under a balcony round three sides of it
    for team, sx in (("red", -1), ("blue", 1)):
        mat = mat_team[team]
        facing = -sx                                        # toward the middle of the hall
        keep_clear.append((*sorted((sx * edge, sx * hx)), -hy, hy))
        add_box(f"{team}BackBalcony", sx * back, sx * hx, -hy, hy, under, UPPER, col_struct, mat_floor)
        add_rail(f"{team}BackRail", -WELL, WELL, sx * (back - 0.1), False, [(-1.5, 1.5)], col_struct, mat)
        for side, sy in (("N", 1), ("S", -1)):
            add_box(f"{team}Balcony{side}", sx * edge, sx * back, sy * WELL, sy * hy, under, UPPER,
                    col_struct, mat_floor)
            # stairs down into the well, and off the front toward the middle of the hall
            well_x = sx * (edge + 3)
            add_stairs(f"{team}WellStairs{side}", well_x, sy * WELL, "-y" if sy > 0 else "+y", UPPER,
                       col_struct, mat_stairs)
            front_y = sy * (WELL + cw) / 2
            add_stairs(f"{team}FrontStairs{side}", sx * edge, front_y, "+x" if sx < 0 else "-x", UPPER,
                       col_struct, mat_stairs)
            keep_clear.append((*sorted((sx * edge, sx * (edge - 5))), front_y - STAIR_WIDTH, front_y + STAIR_WIDTH))
            w = STAIR_WIDTH / 2
            add_rail(f"{team}WellRail{side}", *sorted((sx * edge, sx * back)), sy * (WELL + 0.1), True,
                     [(well_x - w, well_x + w)], col_struct, mat)
            add_rail(f"{team}FrontRail{side}", *sorted((sy * WELL, sy * cw)), sx * (edge + 0.1), False,
                     [(front_y - w, front_y + w)], col_struct, mat)
            # pillars under the balcony, and lights on its underside for the room beneath
            for px in (edge + 3, back - 2.5):
                py = sy * (WELL + cw) / 2
                add_box(f"{team}Pillar{side}_{px:.0f}", sx * px - 0.5, sx * px + 0.5, py - 0.5, py + 0.5, 0, under,
                        col_struct, mat, "high")
            for lx in (edge + 4.5, back - 4.0):
                add_box(f"{team}BalconyGlass{side}_{lx:.0f}", sx * (lx - 2), sx * (lx + 2), sy * (WELL + 1.5),
                        sy * (WELL + 2.0), under - 0.1, under, col_lights, mat_glass)
                add_box(f"{team}BalconyGlass{side}_{lx:.0f}b", sx * (lx - 2), sx * (lx + 2), sy * (hy - 2.0),
                        sy * (hy - 1.5), under - 0.1, under, col_lights, mat_glass)
        for ly in (-14, -4, 4, 14):
            add_box(f"{team}BackGlass_{ly:+d}", sx * (hx - 1.75), sx * (hx - 1.25), ly - 2, ly + 2,
                    under - 0.1, under, col_lights, mat_glass)
        # a band of the side's colour along its end wall, and the pad the flag stands on
        add_box(f"{team}Band", sx * (hx - 0.02), sx * hx, -hy, hy, 2.4, 2.8, col_struct, mat)
        flag_x = sx * (edge + back) / 2
        bpy.ops.mesh.primitive_cylinder_add(vertices=24, radius=2.0, depth=0.02, location=(flag_x, 0, 0.01))
        pad = bpy.context.active_object
        pad.name = f"{team}FlagPad"
        pad.data.materials.append(mat)
        move_to(pad, col_struct)

        # what the game puts here itself: the flag in its firewall, the computer that opens it
        # (under the back balcony, its screen out from the wall), and each droid's post
        marker(f"flag_{team}", flag_x, 0.0, facing)
        marker(f"computer_{team}", sx * (hx - 0.8), -6.0, facing)
        for n, (x, y) in enumerate(((edge + 1.5, -3.0), (edge - 6.0, 0.0), (15.0, 0.0)), start=1):
            marker(f"post_{team}_{n}", sx * x, y, facing)
            keep_clear.append((sx * x - 1.0, sx * x + 1.0, y - 1.0, y + 1.0))

    # centre hub: a raised platform with stairs up from both sides' ends
    h = HUB / 2
    add_box("Hub", -h, h, -h, h, 0, HUB_HEIGHT, col_struct, mat_floor)
    keep_clear.append((-h, h, -h, h))
    for i, (x, d) in enumerate(((-h, "-x"), (h, "+x"))):
        keep_clear.append(add_stairs(f"HubStairs_{i}", x, 0, d, HUB_HEIGHT, col_struct, mat_stairs))
    for i, (x0, x1, y0, y1) in enumerate([(-h, -h + 1, 2.0, h), (h - 1, h, -h, -2.0)]):
        add_box(f"HubCover_{i}", x0, x1, y0, y1, HUB_HEIGHT, HUB_HEIGHT + RAIL, col_struct, mat_low, "low")

    # ceiling light strips
    rows_x, rows_y = int(LENGTH // LIGHT_SPACING), int(WIDTH // LIGHT_SPACING)
    for ix in range(rows_x):
        for iy in range(rows_y):
            x = -(rows_x - 1) * LIGHT_SPACING / 2 + ix * LIGHT_SPACING
            y = -(rows_y - 1) * LIGHT_SPACING / 2 + iy * LIGHT_SPACING
            add_block(f"LightStrip_{ix}_{iy}", x, y, 4.0, 0.5, 0.1, 0, col_lights, mat_glass, "none",
                      z=CEILING - 0.1)

    # random cover on the blue half, mirrored through the centre onto the red half
    placed = []

    def clear(x, y, r):
        for x0, x1, y0, y1 in keep_clear:
            dx, dy = max(x0 - x, 0, x - x1), max(y0 - y, 0, y - y1)
            if math.hypot(dx, dy) < r + MIN_GAP:
                return False
        return all(math.hypot(x - px, y - py) >= r + pr + MIN_GAP for px, py, pr in placed)

    n_cover = 0
    for i in range(NUM_COVER):
        low = rng.random() < 0.6
        w = rng.uniform(1.5, 4.0)
        d = rng.uniform(0.6, 1.2) if low else rng.uniform(0.8, 2.0)
        ht = rng.uniform(0.9, 1.2) if low else rng.uniform(2.0, 2.8)
        r = math.hypot(w, d) / 2
        rot = rng.choice([0, math.pi / 2, rng.uniform(0, math.pi)])
        for _ in range(200):
            x, y = rng.uniform(h + r, edge - r), rng.uniform(-cw + r, cw - r)
            if clear(x, y, r) and clear(-x, -y, r):
                placed += [(x, y, r), (-x, -y, r)]
                kind, mat = ("Low", mat_low) if low else ("High", mat_high)
                add_block(f"{kind}Cover_{i}_B", x, y, w, d, ht, rot, col_cover, mat, kind.lower())
                add_block(f"{kind}Cover_{i}_R", -x, -y, w, d, ht, rot, col_cover, mat, kind.lower())
                n_cover += 2
                break

    # light + camera
    bpy.ops.object.light_add(type="SUN", location=(0, 0, 30), rotation=(math.radians(45), 0, math.radians(30)))
    bpy.context.active_object.data.energy = 3
    bpy.ops.object.camera_add(location=(0, -WIDTH * 1.6, LENGTH * 0.75), rotation=(math.radians(40), 0, 0))
    bpy.context.scene.camera = bpy.context.active_object
    world = bpy.data.worlds.new("World")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.55, 0.65, 0.8, 1)
    bpy.context.scene.world = world
    bpy.context.scene["map_seed"] = seed

    out = os.path.abspath(out) if out else os.path.join(os.path.dirname(os.path.abspath(__file__)), "ctf_map.blend")
    bpy.ops.wm.save_as_mainfile(filepath=out)
    print(f"[map] seed={seed} cover={n_cover} -> {out}")
    if glb:
        export_glb(os.path.abspath(glb), [col_hall, col_struct, col_cover])


def join(col):
    """Joins a collection's meshes into one object (keeping their materials)."""
    objs = [o for o in col.objects if o.type == "MESH"]
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    objs[0].name = col.name


def export_glb(path, cols):
    """Exports for the game with the hall, structures and cover each joined into one mesh, and
    the markers as empties. Runs after the .blend is saved, so the saved file keeps every object
    separate."""
    for col in cols:
        join(col)
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", export_extras=True,
                              export_cameras=False, export_lights=False)
    print(f"[map] exported -> {path}")


if __name__ == "__main__":
    main()
