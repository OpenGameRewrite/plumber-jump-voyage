"""Versioned rendering metadata for local glTF exports.

Source names are interpreted here once. The game consumes explicit roles;
renaming exported meshes and bones leaves their behavior unchanged.
"""
VERSION = 1


def validate(spec, kind):
    if not isinstance(spec, dict):
        raise ValueError(f'{kind} metadata must be an object')
    if type(spec.get('version')) is not int or spec['version'] != VERSION:
        raise ValueError(f'Unsupported {kind} metadata version')
    if kind == 'part':
        if set(spec) != {'version', 'role', 'track', 'pupil', 'surface', 'stain', 'exported', 'metal'}:
            raise ValueError('Invalid part metadata fields')
        if not isinstance(spec['role'], str) or spec['role'] not in {'unchanged', 'hidden', 'visible', 'closed_left', 'closed_right', 'pupil_left', 'pupil_right', 'track', 'worn_cap', 'uncapped_hair'}:
            raise ValueError('Invalid visibility role')
        if not isinstance(spec['surface'], str) or not isinstance(spec['stain'], str) or spec['surface'] not in {'ordinary', 'glass', 'hair'} or spec['stain'] not in {'none', 'body', 'cap'}:
            raise ValueError('Invalid material role')
        if any(spec[key] is not None and not isinstance(spec[key], str) for key in ('track', 'pupil')):
            raise ValueError('Track identities must be strings or null')
        if spec['role'] in {'track', 'worn_cap'} and not spec['track']:
            raise ValueError('Visibility role requires a track identity')
        flags = ('exported', 'metal')
    else:
        if set(spec) != {'version', 'key', 'leg', 'hip'}:
            raise ValueError('Invalid bone metadata fields')
        key = spec['key']
        if not isinstance(key, list) or len(key) != 2 or not all(isinstance(x, str) for x in key) or not key[0]:
            raise ValueError('Invalid bone identity')
        flags = ('leg', 'hip')
    if any(type(spec[key]) is not bool for key in flags):
        raise ValueError('Metadata flags must be booleans')


def part(name, metal=False):
    shape, _, material = name.partition('__')
    role, track = 'unchanged', None
    if name.startswith(('EyeClose', 'EyeHalf')):
        role = 'hidden'
    elif name.startswith('EyeOpen'):
        role = 'visible'
    elif name.startswith('EyelidCloseLeft'):
        role = 'closed_left'
    elif name.startswith('EyelidCloseRight'):
        role = 'closed_right'
    elif name.startswith('Eyeball__EyePupilLeft'):
        role = 'pupil_left'
    elif name.startswith('Eyeball__EyePupilRight'):
        role = 'pupil_right'
    elif name.startswith('Eyelid'):
        role, track = 'track', shape
    elif name.startswith('Eyeball__EyeMT'):
        role = 'hidden'
    elif name.startswith(('Cap__', 'CapHair__')):
        role, track = 'worn_cap', shape
    elif name.startswith('Hair__'):
        role = 'uncapped_hair'
    stain = 'none' if '__' not in name or 'Eye' in name or 'Glass' in name else 'cap' if name.startswith(('Cap__', 'CapHair__')) else 'body'
    surface = 'glass' if 'Glass' in name else 'hair' if name.endswith('__HairMT') else 'ordinary'
    return dict(version=VERSION, role=role, track=track,
                pupil=material if material.startswith('EyePupil') else None,
                surface=surface, stain=stain, exported='__' in name, metal=metal)


def parent_key(name):
    return '' if name in ('Armature', 'Armature_nw4f_root') else name


def bone(name, parent):
    short = name.removeprefix('Armature_')
    leg = not short.startswith(('Eye_', 'Face_', 'Head_')) and any(
        word in short for word in ('Hip', 'Leg', 'Foot', 'Toe', 'JointRoot', 'AllRoot', 'nw4f_root'))
    return dict(version=VERSION, key=[name, parent_key(parent)], leg=leg, hip=name.endswith('Hip'))


def annotate(model, metal=False):
    """Attach roles to primitive/node extras without changing geometry or clips."""
    if not isinstance(model, dict):
        raise ValueError('glTF JSON must be an object')
    for mesh in model.get('meshes', []):
        for i, primitive in enumerate(mesh['primitives']):
            name = mesh.get('name', '')
            if len(mesh['primitives']) > 1:
                name += f'.{i}'
            spec = part(name, metal)
            spec = primitive.setdefault('extras', {}).setdefault('pjv_part', spec)
            validate(spec, 'part')
    nodes = model.get('nodes', [])
    parents = {}
    identities = set()
    for i, node in enumerate(nodes):
        for child in node.get('children', []):
            if not 0 <= child < len(nodes) or child in parents:
                raise ValueError('Invalid or multiply parented glTF node')
            parents[child] = i
    completed = set()
    for node in parents:
        trail = set()
        while node in parents and node not in completed:
            if node in trail:
                raise ValueError('Cyclic glTF node hierarchy')
            trail.add(node)
            node = parents[node]
        completed.update(trail)
    for i, node in enumerate(nodes):
        name = node.get('name', '')
        if not name:
            continue
        parent = nodes[parents[i]].get('name', '') if i in parents else ''
        spec = node.setdefault('extras', {}).setdefault('pjv_bone', bone(name, parent))
        validate(spec, 'bone')
        key = tuple(spec['key'])
        if key in identities:
            raise ValueError(f'Ambiguous animation bone identity {key}')
        identities.add(key)


def self_check():
    assert part('MarioEye__EyeMT')['role'] == 'unchanged'
    assert part('Eyeball__EyeMT')['role'] == 'hidden'
    assert part('CapHair__HairMT', True)['stain'] == 'cap'
    assert bone('Armature_Eye_JointRoot', '')['leg'] is False
    model = {'meshes': [{'name': 'Cap__CapMT', 'primitives': [{}]}],
             'nodes': [{'name': 'Armature', 'children': [1]}, {'name': 'Armature_Hip'}]}
    annotate(model)
    original = model['meshes'][0]['primitives'][0]['extras']['pjv_part'].copy()
    identity = model['nodes'][1]['extras']['pjv_bone'].copy()
    model['meshes'][0]['name'] = 'renamed';model['nodes'][1]['name'] = 'renamed'
    annotate(model)
    assert model['meshes'][0]['primitives'][0]['extras']['pjv_part'] == original
    assert model['nodes'][1]['extras']['pjv_bone'] == identity
    bad = part('Cap__CapMT')
    bad['track'] = None
    try:
        validate(bad, 'part')
    except ValueError:
        pass
    else:
        raise AssertionError('Missing visibility track accepted')
    for nodes in ([{'name':'a','children':[1]},{'name':'b','children':[0]}],
                  [{'name':'a'},{'name':'a'}], [{'children':[2]}]):
        try:
            annotate({'nodes': nodes})
        except ValueError:
            pass
        else:
            raise AssertionError('Invalid or ambiguous hierarchy accepted')
    print('Asset metadata checks passed')


if __name__ == '__main__':
    self_check()
