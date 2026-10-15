"""Install the player's particle effects from your extracted RomFS.

Reads the game's particle archive and the tables that say when each effect
starts (animation names, hit reactions, joints, floor materials), and writes
only what Mario and Cappy use to assets/local/effects: effects.json (the
emitters, triggers and small meshes) and the particle textures as PNG files.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import struct
import tempfile

import extract
import reaction_names
import switch_meshes
import switch_textures
import vfx_particles

spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
outfits = importlib.util.module_from_spec(spec)
spec.loader.exec_module(outfits)

FORMAT_VERSION = 1
# The effect tables of the player, Cappy and the player's hit reactions.
TABLES = {'Mario.byml': 'Mario', 'MarioCap.byml': 'Cap', 'PlayerHitReactionHakoniwa.byml': 'Reaction'}
# Meshes drawn in centimetres are stored one centimetre per unit like the rest.
MESH_UNIT_LIMIT = 20.
# Keys of an effect entry that are switches ("o" when set) rather than parameters.
NOT_FLAGS = ('ActionList', 'ResourceList', 'EffectName', 'JointName', 'PosOffset', 'HitReactionList',
             'DrawCategory', 'Rotate', 'Scale')


def vector(value):
    """[x, y, z] from the tables' {"X":..,"Y":..,"Z":..} dictionaries."""
    return None if value is None else [value.get('X', 0.), value.get('Y', 0.), value.get('Z', 0.)]


def effect_trigger(table, entry):
    """One table entry in the form the game reads."""
    return {
        'table': table,
        'actions': [{'name': a['ActionName'], 'start': a.get('StartFrame', 0), 'keep': bool(a.get('KeepEmitter'))}
                    for a in entry.get('ActionList', [])],
        'reactions': [reaction_names.english(r['ReactionName']) for r in entry.get('HitReactionList', [])],
        'joint': entry.get('JointName'),
        'offset': vector(entry.get('PosOffset')),
        'rotate': vector(entry.get('Rotate')),
        'scale': entry.get('Scale'),
        'flags': sorted(k for k, v in entry.items() if v == 'o' and k not in NOT_FLAGS),
        'resources': [{'name': r['ResourceName'], 'material': r.get('MaterialType'),
                       'joint': r.get('JointName'), 'offset': vector(r.get('PosOffset'))}
                      for r in entry.get('ResourceList', [])],
    }


def read_effect_tables(archive):
    """Effect name -> triggers from the player, cap and hit-reaction tables."""
    effects = {}
    for file, table in TABLES.items():
        if file not in archive:
            raise ValueError(f'The effect database has no {file}')
        for entry in outfits.byml(archive[file]).get('EffectList', []):
            effects.setdefault(entry['EffectName'], []).append(effect_trigger(table, entry))
    return effects


def referenced_resources(effects):
    return sorted({r['name'] for triggers in effects.values() for t in triggers for r in t['resources']})


def used_meshes(resources):
    return {e['render']['mesh'] for tree in resources.values() for e in vfx_particles.walk(tree) if e['render']['mesh']}


def used_textures(resources):
    return {t['name'] for tree in resources.values() for e in vfx_particles.walk(tree) for t in e['textures'] if t}


def normalized_mesh(mesh):
    """Positions and texture coordinates as plain lists, large meshes scaled to unit size."""
    peak = max((abs(c) for p in mesh['position'] for c in p[:3]), default=1.)
    divisor = 100. if peak >= MESH_UNIT_LIMIT else 1.
    uv = mesh['uv'] or [(0., 0.)] * len(mesh['position'])
    return {'position': [[round(c / divisor, 4) for c in p[:3]] for p in mesh['position']],
            'uv': [[round(u, 4), round(v, 4)] for u, v in (x[:2] for x in uv)],
            'triangles': [list(t) for t in mesh['triangles']]}


def finite(value):
    """The same structure with non-finite numbers (unused fields can hold them) replaced by zero."""
    if isinstance(value, float):
        return value if math.isfinite(value) else 0.
    if isinstance(value, list):
        return [finite(v) for v in value]
    if isinstance(value, dict):
        return {k: finite(v) for k, v in value.items()}
    return value


def install_textures(data, section, names, wanted, folder):
    """Write the wanted textures as PNG files; returns the names that were written."""
    archive = switch_textures.read_archive(data[section.body:section.body + section.size])
    by_name = {t.name: t for t in archive}
    folder.mkdir(parents=True, exist_ok=True)
    written, skipped = [], []
    for name in sorted(wanted):
        texture = by_name.get(name)
        image = texture.decode() if texture else None
        if image is None:
            skipped.append(name)
            continue
        switch_textures.apply_channel_select(image, texture.channels).save(folder / f'{name}.png', optimize=True)
        written.append(name)
    return written, skipped


def install(romfs, output):
    effect_dir = romfs / 'EffectData'
    for file in extract.EFFECT_FILES:
        if not (effect_dir / file).is_file():
            raise ValueError(f'{effect_dir / file} is missing; run tools/extract.py again to keep the effect files')
    database = outfits.archive(effect_dir / 'EffectDataBase.szs')
    effects = read_effect_tables(database)
    particles = dict(extract.sarc(extract.yaz0((effect_dir / 'EffectPtcl.szs').read_bytes())))
    data = particles.get('EffectData.ptcl')
    if data is None:
        raise ValueError('EffectPtcl.szs has no EffectData.ptcl')
    version, sections = vfx_particles.top_sections(data)
    if version != vfx_particles.VERSION:
        raise ValueError(f'Unsupported particle file version {version}')
    names = vfx_particles.texture_names(data, sections['GRTF'])
    wanted = referenced_resources(effects)
    resources, missing = {}, []
    sets = dict(vfx_particles.emitter_sets(data, sections['ESTA']))
    for name in wanted:
        if name not in sets:
            missing.append(name)
            continue
        resources[name] = [vfx_particles.describe_emitter(vfx_particles.read_emitter_tree(data, e), names)
                           for e in sets[name].children(data)]
    mesh_ids = used_meshes(resources)
    decoded = switch_meshes.read_meshes(
        data[sections['G3PR'].body:sections['G3PR'].body + sections['G3PR'].size],
        vfx_particles.mesh_descriptors(data, sections['G3PR']))
    meshes = {key: normalized_mesh(decoded[int(key, 16)]) for key in sorted(mesh_ids) if int(key, 16) in decoded}
    textures_dir = output / 'textures'
    for old in textures_dir.glob('*.png') if textures_dir.is_dir() else []:
        old.unlink()
    written, skipped = install_textures(data, sections['GRTF'], names, used_textures(resources), textures_dir)
    # Emitters never need the shader index; it only helped while decoding.
    for tree in resources.values():
        for emitter in vfx_particles.walk(tree):
            emitter.pop('_shader_index', None)
    table = {'version': FORMAT_VERSION, 'effects': effects,
             'resources': {k: {'emitters': v} for k, v in resources.items()},
             'meshes': meshes, 'textures': written}
    partial = output / 'effects.json.partial'
    partial.write_text(json.dumps(finite(table), allow_nan=False, separators=(',', ':'), ensure_ascii=False),
                       encoding='utf-8')
    partial.replace(output / 'effects.json')
    print(f'Installed {len(resources)} particle effects, {len(written)} textures and {len(meshes)} meshes '
          f'for {len(effects)} named effects.')
    if missing or skipped:
        print('Left out: ' + ', '.join([*missing, *(f'texture {s}' for s in skipped)]))


def self_check():
    switch_textures.self_check()
    switch_meshes.self_check()
    vfx_particles.self_check()
    reaction_names.self_check()
    assert vector({'X': 1., 'Y': 2.}) == [1., 2., 0.] and vector(None) is None
    entry = {'EffectName': 'Land', 'FollowTrans': 'o', 'DrawCategory': 'x', 'JointName': 'Hip',
             'ActionList': [{'ActionName': 'Jump', 'StartFrame': -1, 'KeepEmitter': True}],
             'HitReactionList': [{'ReactionName': '着地'}, {'ReactionName': 'unlisted'}],
             'ResourceList': [{'ResourceName': 'A'}, {'ResourceName': 'B', 'MaterialType': 'Snow',
                                                        'PosOffset': {'Y': 30.}}]}
    trigger = effect_trigger('Mario', entry)
    assert trigger['flags'] == ['FollowTrans'] and trigger['reactions'] == ['Land', 'unlisted']
    assert trigger['actions'] == [{'name': 'Jump', 'start': -1, 'keep': True}]
    assert trigger['resources'][1] == {'name': 'B', 'material': 'Snow', 'joint': None, 'offset': [0., 30., 0.]}
    assert referenced_resources({'Land': [trigger]}) == ['A', 'B']
    assert finite({'a': [float('nan'), 1.5, float('inf')]}) == {'a': [0., 1.5, 0.]}
    small = normalized_mesh({'position': [(1., 0., 0.)], 'uv': None, 'triangles': []})
    large = normalized_mesh({'position': [(50., 0., 0.)], 'uv': [(0.5, 1.)], 'triangles': [(0, 0, 0)]})
    assert small['position'] == [[1., 0., 0.]] and large['position'] == [[0.5, 0., 0.]]
    with tempfile.TemporaryDirectory() as temp:
        try:
            install(Path(temp), Path(temp) / 'out')
        except ValueError:
            pass
        else:
            raise AssertionError('Missing effect files accepted')
    print('Effect extraction checks passed')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--romfs', type=Path, default=extract.ROOT / 'local/romfs')
    p.add_argument('--output', type=Path, default=extract.ROOT / 'assets/local/effects')
    p.add_argument('--self-check', action='store_true')
    a = p.parse_args()
    if a.self_check:
        self_check()
        return
    a.output.mkdir(parents=True, exist_ok=True)
    install(a.romfs, a.output)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, IndexError, struct.error) as e:
        raise SystemExit(f'Effect extraction stopped: {e}')
