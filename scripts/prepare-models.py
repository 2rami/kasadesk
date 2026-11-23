"""Prepare manufacturer geometry in a local KasaDesk model library using Blender."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import urllib.request
import gzip


def convert(source, output, yaw):
    import bpy
    from mathutils import Matrix
    import numpy as np
    from math import ceil, radians, sqrt

    bpy.ops.wm.read_factory_settings(use_empty=True)
    if source.suffix.lower() == ".usdz":
        bpy.ops.wm.usd_import(filepath=str(source))
    else:
        bpy.ops.import_scene.gltf(filepath=str(source))
    objects = [o for o in bpy.context.scene.objects if o.type == 'MESH' and not o.hide_render and o.visible_get()]
    def color_image(socket):
        if not socket.links:
            return None
        node = socket.links[0].from_node
        if node.type == 'TEX_IMAGE' and node.image:
            return node
        # USD Preview Surface imports scaled emission textures through a Vector Math node.
        if node.type in ('VECT_MATH', 'MIX_RGB', 'MIX'):
            return next((img for inp in node.inputs if inp.is_linked and (img := color_image(inp))), None)
        return None

    textures = []
    materials = {}
    for obj in objects:
        for material in obj.data.materials:
            if not material or material in materials:
                continue
            nodes = material.node_tree.nodes if material.use_nodes else []
            bsdf = next((n for n in nodes if n.type == 'BSDF_PRINCIPLED'), None)
            color = material.diffuse_color[:3]
            texture = None
            emission = False
            uv_name = None
            if bsdf:
                base = bsdf.inputs['Base Color']
                color = base.default_value[:3]
                node = color_image(bsdf.inputs['Emission Color']) if bsdf.inputs['Emission Strength'].default_value > 0 else None
                emission = node is not None
                if node is None:
                    node = color_image(base)
                if node:
                    texture = node.image
                    color = (1, 1, 1)
                    vector = node.inputs['Vector']
                    if vector.is_linked and vector.links[0].from_node.type == 'UVMAP':
                        uv_name = vector.links[0].from_node.uv_map
            if texture and texture not in textures:
                textures.append(texture)
            materials[material] = (np.power(np.clip(color, 0, 1), 1 / 2.2), textures.index(texture) + 1 if texture else 0, emission, uv_name)

    # A white tile lets untextured surfaces share a single draw call with the product artwork.
    edge = 1024
    grid = ceil(sqrt(len(textures) + 1))
    if grid > 16:
        raise ValueError('too many texture images for one model')
    atlas = np.ones((edge, edge, 4), dtype=np.float32)
    tile = edge // grid
    for index, source_image in enumerate(textures, 1):
        img = source_image.copy()
        img.scale(tile - 4, tile - 4)
        pixels = np.empty((tile - 4, tile - 4, 4), dtype=np.float32)
        img.pixels.foreach_get(pixels.ravel())
        x, y = index % grid * tile, index // grid * tile
        atlas[y:y + tile, x:x + tile] = np.pad(pixels, ((2, 2), (2, 2), (0, 0)), mode='edge')
        bpy.data.images.remove(img)
    with tempfile.TemporaryDirectory() as tmp:
        pic = bpy.data.images.new('KasaDesk diffuse atlas', edge, edge, alpha=False, float_buffer=True)
        pic.colorspace_settings.name = 'Linear Rec.709'
        pic.pixels.foreach_set(atlas.ravel())
        pic.filepath_raw = str(Path(tmp) / 'atlas.png')
        pic.file_format = 'PNG'
        pic.save()
        tex = Path(pic.filepath_raw).read_bytes()

    parts = []
    turn = Matrix.Rotation(radians(yaw), 4, 'Z')
    axis = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]], dtype=np.float32)
    deps = bpy.context.evaluated_depsgraph_get()
    for original in objects:
        obj = original.evaluated_get(deps)
        mesh = obj.to_mesh()
        try:
            mesh.calc_loop_triangles()
            count = len(mesh.loop_triangles)
            if not count:
                continue
            ids = np.empty(count * 3, np.int32)
            mesh.loop_triangles.foreach_get('loops', ids)
            vertex_ids = np.empty(len(mesh.loops), np.int32)
            mesh.loops.foreach_get('vertex_index', vertex_ids)
            xyz = np.empty((len(mesh.vertices), 3), np.float32)
            mesh.vertices.foreach_get('co', xyz.ravel())
            transform = np.asarray(turn @ obj.matrix_world)
            linear = axis @ transform[:3, :3]
            if np.linalg.det(linear) < 0:
                ids = ids.reshape(-1, 3)[:, ::-1].reshape(-1)
            data = np.zeros((count * 3, 12), np.float32)
            data[:, :3] = xyz[vertex_ids[ids]] @ linear.T + axis @ transform[:3, 3]
            normals = np.empty((len(mesh.corner_normals), 3), np.float32)
            mesh.corner_normals.foreach_get('vector', normals.ravel())
            data[:, 3:6] = normals[ids] @ np.linalg.pinv(linear)
            data[:, 3:6] /= np.maximum(np.linalg.norm(data[:, 3:6], axis=1, keepdims=True), 1e-9)
            mat_ids = np.empty(count, np.int32)
            mesh.loop_triangles.foreach_get('material_index', mat_ids)
            mat_ids = np.repeat(mat_ids, 3)
            uv = np.zeros((len(mesh.loops), 2), np.float32)
            for m in np.unique(mat_ids):
                mask = mat_ids == m
                material = original.material_slots[m].material if m < len(original.material_slots) else None
                color, slot, emission, uv_name = materials.get(material, ((0.6, 0.6, 0.6), 0, False, None))
                layer = mesh.uv_layers.get(uv_name) if uv_name else mesh.uv_layers.active
                if layer:
                    layer.uv.foreach_get('vector', uv.ravel())
                data[mask, 9] = float(emission)
                data[mask, 6:9] = color
                origin = np.array([slot % grid, slot // grid]) * tile
                coords = np.nan_to_num(np.clip(uv[ids[mask]], 0, 1), nan=0.5) if slot else 0.5
                data[mask, 10:12] = (origin + 2.5 + coords * (tile - 5)) / edge
            data[:, 11] = 1 - data[:, 11]
            parts.append(data)
        finally:
            obj.to_mesh_clear()
    vertices = np.concatenate(parts)
    if len(vertices) > 3_000_000 or not np.isfinite(vertices).all():
        raise ValueError('invalid or oversized model')
    lo, hi = vertices[:, :3].min(axis=0), vertices[:, :3].max(axis=0)
    if hi[0] <= lo[0]:
        raise ValueError('model has no width')
    vertices[:, :3] -= [(lo[0] + hi[0]) / 2, lo[1], (lo[2] + hi[2]) / 2]
    vertices[:, :3] /= hi[0] - lo[0]
    with output.open('wb') as stream:
        stream.write(struct.pack('<4sII', b'KDM1', len(vertices), len(tex)))
        stream.write(vertices.astype('<f4', copy=False).tobytes())
        stream.write(tex)
    print(f'{source.name}: {len(vertices)//3} triangles, dimensions {hi-lo}', flush=True)


def prepare():
    parser = argparse.ArgumentParser(description=__doc__)
    if sys.platform == 'darwin':
        base = Path.home() / 'Library' / 'Application Support'
    elif os.name == 'nt':
        base = Path(os.environ['LOCALAPPDATA'])
    else:
        base = Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local/share'))
    parser.add_argument('catalog', type=Path, nargs='?', default=Path(__file__).with_name('official-models.json'))
    parser.add_argument('output', type=Path, nargs='?', default=Path(os.environ.get('KASADESK_MODEL_DIR', base / 'KasaDesk' / 'Models')))
    parser.add_argument('--blender', default=shutil.which('blender') or ('/Applications/Blender.app/Contents/MacOS/Blender' if sys.platform == 'darwin' else 'blender'))
    args = parser.parse_args()
    config = json.loads(args.catalog.read_text())
    args.output.mkdir(parents=True, exist_ok=True)
    if not shutil.which(args.blender):
        parser.error('Blender 4.4+ is required; specify its executable with --blender')
    with tempfile.TemporaryDirectory(dir=args.output) as tmp:
        work = Path(tmp)
        for name, entry in config['models'].items():
            if not name.isascii() or not name.replace('-', '').isalnum():
                raise ValueError(f'invalid asset key: {name}')
            url = entry.get('source')
            entry.pop('cache', None)
            if not url:
                continue
            suffix = Path(url).suffix.lower()
            if suffix not in ('.usdz', '.glb'):
                raise ValueError(f'unsupported source: {url}')
            source = work / (name + suffix)
            if url.startswith('https://'):
                with urllib.request.urlopen(url, timeout=60) as res, source.open('wb') as f:
                    data = gzip.GzipFile(fileobj=res) if res.headers.get('Content-Encoding') == 'gzip' else res
                    shutil.copyfileobj(data, f)
            else:
                shutil.copyfile(args.catalog.parent / url, source)
            if entry.get('sha256') and hashlib.sha256(source.read_bytes()).hexdigest() != entry['sha256']:
                raise ValueError(f'checksum mismatch: {name}')
            output = work / (name + '.kdm')
            subprocess.run([args.blender, '-b', '-t', '2', '--python-exit-code', '1', '-P', __file__, '--',
                            str(source.resolve()), str(output.resolve()), str(entry.get('yaw', 0))], check=True)
            with output.open('rb') as stream:
                hasher = hashlib.sha256()
                for block in iter(lambda: stream.read(1 << 20), b''):
                    hasher.update(block)
                digest = hasher.hexdigest()
            entry['cache'] = f'{name}-{digest[:16]}.kdm'
            output.rename(work / entry['cache'])
        # Publish the catalog after all referenced conversions have succeeded.
        for model in work.glob('*.kdm'):
            os.replace(model, args.output / model.name)
        manifest = work / 'catalog.json'
        manifest.write_text(json.dumps(config, ensure_ascii=False, indent=2))
        os.replace(manifest, args.output / manifest.name)


if __name__ == '__main__':
    if '--' in sys.argv:
        src, dst, yaw = sys.argv[sys.argv.index('--') + 1:]
        convert(Path(src), Path(dst), float(yaw))
    else:
        prepare()
