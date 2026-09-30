# Exports the capture-the-flag models from their .blend files to data/ctf/:
#
#     blender -b ~/Documents/blender/unlockables/capture_the_flag/red_flag.blend \
#         --python data/ctf/export_models.py -- flag_red.glb
#     blender -b .../blue_flag.blend --python data/ctf/export_models.py -- flag_blue.glb
#     blender -b .../firewall.blend --python data/ctf/export_models.py -- firewall.glb
#
# The flags' files hold rig widgets (WGT-*) and a camera besides the flag, and the flag's
# `Circle` is only an outline, without faces: only the pole and the cloth are exported, as
# `pole` and `cloth`. The pole stands from 0.5 m, on the firewall's plinth.
#
# The firewall is split in two: `firewall_barrier`, the glowing orange shell, which goes when
# its computer is hacked, and is see-through - blended, with an alpha of BARRIER_ALPHA - and
# `firewall_frame`, the grey plinth and rim that stay.

import os
import sys

import bpy
import bmesh

# How opaque the firewall's shell is. The game draws it as its own glass, tinted and glowing
# orange, and flickers its glow (see src/firewall.rs); glTF has no way to carry a flicker.
BARRIER_ALPHA = 0.35

HERE = os.path.dirname(os.path.abspath(__file__))
out = os.path.join(HERE, sys.argv[sys.argv.index("--") + 1])

for obj in list(bpy.data.objects):
    if obj.name not in ("Cylinder", "Plane", "firewall.001"):
        bpy.data.objects.remove(obj)

if "firewall.001" in bpy.data.objects:
    frame = bpy.data.objects["firewall.001"]
    barrier = frame.copy()
    barrier.data = frame.data.copy()
    bpy.context.scene.collection.objects.link(barrier)
    # The barrier keeps the orange faces (material slot 0), the frame the rest.
    for obj, keep in ((barrier, 0), (frame, 1)):
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index != keep], context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(obj.data)
        bm.free()
        slot = obj.material_slots[keep].material
        obj.data.materials.clear()
        obj.data.materials.append(slot)
    shell = barrier.material_slots[0].material.copy()
    shell.name = "firewall_shell"
    shell.surface_render_method = "BLENDED"
    shell.node_tree.nodes["Principled BSDF"].inputs["Alpha"].default_value = BARRIER_ALPHA
    barrier.material_slots[0].material = shell
    frame.name, barrier.name = "firewall_frame", "firewall_barrier"
    frame.data.name, barrier.data.name = "firewall_frame", "firewall_barrier"
else:
    bpy.data.objects["Cylinder"].name = "pole"
    bpy.data.objects["Plane"].name = "cloth"

bpy.ops.export_scene.gltf(
    filepath=out, export_format="GLB", export_apply=True, export_animations=False, export_yup=True
)
print("Wrote", out)
