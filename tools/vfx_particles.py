"""Read the Switch particle archive (VFXB, version 22) for extract-effects.py.

The file is a tree of sections: emitter sets contain emitters, emitters
contain child emitters and animation attachments; separate sections hold the
texture archive and the mesh archive. `read_emitter` decodes the fixed-size
emitter record field by field, `describe_emitter` turns it into the plain,
named structure the game reads at runtime.
"""
import math
import struct

NONE = 0xFFFFFFFF
NO_ID = 2 ** 64 - 1
VERSION = 22

VOLUMES = ('point', 'circle', 'circle_divided', 'circle_fill', 'sphere', 'sphere_divided', 'sphere_divided64',
           'sphere_fill', 'cylinder', 'cylinder_fill', 'box', 'box_fill', 'line', 'line_divided', 'rectangle',
           'primitive')
WRAPS = ('mirror', 'repeat', 'clamp', 'mirror_once')
PATTERNS = ('none', 'life_fit', 'clamp', 'loop', 'random')
FOLLOWS = ('all', 'world', 'translate')
# Magic letters of the animation attachments the particle simulation understands.
KEY_ANIMATIONS = ('EASL', 'EATR', 'EAER', 'EAA0', 'EAA1', 'EAC0', 'EAC1', 'EAOV', 'EADV', 'EAGV', 'EAPL',
                  'EAET', 'EAES')


class Cursor:
    """Sequential little-endian reader."""

    def __init__(self, data, position=0):
        self.data = data
        self.position = position

    def take(self, fmt):
        values = struct.unpack_from('<' + fmt, self.data, self.position)
        self.position += struct.calcsize('<' + fmt)
        return values[0] if len(values) == 1 else values

    def u8(self):
        return self.take('B')

    def u32(self):
        return self.take('I')

    def i32(self):
        return self.take('i')

    def u64(self):
        return self.take('Q')

    def f32(self):
        return self.take('f')

    def floats(self, count):
        return list(self.take(f'{count}f'))

    def skip(self, count):
        self.position += count

    def text(self, size):
        raw = self.data[self.position:self.position + size]
        self.position += size
        return raw.split(b'\0', 1)[0].decode('utf-8', 'replace')


class Section:
    """Header of a tree section: size, links to children, siblings and attachments."""

    def __init__(self, data, position):
        self.position = position
        self.magic = data[position:position + 4].decode('ascii', 'replace')
        (self.size, self.child_offset, self.next_offset, self.attribute_offset, self.body_offset,
         _padding, self.child_count) = struct.unpack_from('<7I', data, position + 4)

    @property
    def body(self):
        return self.position + self.body_offset

    def children(self, data):
        if self.child_offset == NONE or not self.child_count:
            return
        position = self.position + self.child_offset
        for _ in range(self.child_count):
            child = Section(data, position)
            yield child
            if child.next_offset == NONE:
                break
            position += child.next_offset


def top_sections(data):
    """Version and the top-level sections by magic (ESTA, GRTF, G3PR, ...)."""
    if data[:4] != b'VFXB':
        raise ValueError('Expected a VFXB particle file')
    version, block_offset = struct.unpack_from('<H', data, 10)[0], struct.unpack_from('<H', data, 22)[0]
    sections = {}
    position = block_offset
    while True:
        section = Section(data, position)
        sections[section.magic] = section
        if section.next_offset == NONE:
            return version, sections
        position += section.next_offset


def key_table(cursor):
    """Eight keys of (x, y, z, time); unused ones are filled with zeros."""
    return [cursor.floats(4) for _ in range(8)]


def read_static(c):
    """The emitter's animation-curve and texture-animation block."""
    s = {'flags': [c.u32() for _ in range(4)]}
    s['key_counts'] = dict(zip(('color0', 'alpha0', 'color1', 'alpha1', 'scale', 'param'), [c.u32() for _ in range(6)]))
    c.skip(8)
    c.skip(40)  # loop rate and loop randomness of the five curves
    c.skip(8)
    s['gravity_dir'] = c.floats(3)
    s['gravity_scale'] = c.f32()
    s['air_resistance'] = c.f32()
    c.skip(12 + 16)  # unknown scalars, centre and offset
    s['fluctuation'] = {'amplitude': c.floats(2), 'cycle': c.floats(2)}
    c.skip(16 + 8 + 8)  # fluctuation phases, coefficient, two unknown scalars
    s['patterns'] = []
    for _ in range(3):
        count, frequency, random_range, _pad = c.floats(4)
        s['patterns'].append({'count': count, 'frequency': frequency, 'random_range': random_range,
                              'table': list(c.take('32i'))})
    s['scrolls'] = []
    for _ in range(3):
        v = c.floats(20)
        s['scrolls'].append({'scroll_add': v[0:2], 'scroll': v[2:4], 'scale_add': v[6:8], 'scale': v[8:10],
                             'rotation_add': v[12], 'rotation': v[13], 'grid': v[18:20]})
    s['color_scale'] = c.f32()
    c.skip(12)
    for name in ('color0', 'alpha0', 'color1', 'alpha1'):
        s[name + '_keys'] = key_table(c)
    c.skip(40)  # soft edge, fresnel, distance fades, decal
    s['alpha_threshold'] = c.floats(2)[0]
    s['add_velocity_to_scale'] = c.f32()
    c.skip(12)
    s['scale_keys'] = key_table(c)
    s['param_keys'] = key_table(c)
    s['rotate_init'] = c.floats(4)[:3]
    s['rotate_init_random'] = c.floats(4)[:3]
    s['rotate_add'] = c.floats(3)
    s['rotate_resistance'] = c.f32()
    s['rotate_add_random'] = c.floats(3)
    c.skip(4 + 8 + 8)
    return s


def read_info(c):
    flags = [c.u8() for _ in range(13)]
    info = dict(zip(('draw', 'sort_type', 'calc_type', 'follow_type', 'fade_emit', 'alpha_fade', 'scale_fade',
                     'seed_type', 'update_matrix_on_emit', 'test_always', 'interpolate_emission',
                     'alpha_fade_in', 'scale_fade_in'), flags))
    c.skip(3 + 4)  # padding, random seed
    info['draw_path'] = c.u32()
    info['alpha_fade_time'] = c.i32()
    info['fade_in_time'] = c.i32()
    for name in ('trans', 'trans_random', 'rotate', 'rotate_random', 'scale'):
        info[name] = c.floats(3)
    info['color0'], info['color1'] = c.floats(4), c.floats(4)
    c.skip(12)  # emission range and far ratio
    return info


def read_inherit(c):
    names = ('velocity', 'scale', 'rotate', 'color_scale', 'color0', 'color1', 'alpha0', 'alpha1', 'draw_path',
             'pre_draw', 'alpha0_each_frame', 'alpha1_each_frame', 'enable_emitter_particle')
    inherit = {name: c.u8() for name in names}
    c.skip(3)
    inherit['velocity_rate'], inherit['scale_rate'] = c.f32(), c.f32()
    return inherit


def read_emission(c):
    e = {}
    e['one_time'], e['world_gravity'], _dist, _world_velocity = (c.u8() for _ in range(4))
    e['start'], e['timing'], e['duration'] = c.u32(), c.u32(), c.u32()
    e['rate'], e['rate_random'] = c.f32(), c.f32()
    e['interval'], e['interval_random'] = c.i32(), c.u32()
    e['position_random'], e['gravity_scale'] = c.f32(), c.f32()
    e['gravity_dir'] = c.floats(3)
    c.skip(20)  # emission by distance
    return e


def read_shape(c):
    s = {}
    names = ('volume', 'sweep_start_random', 'arc_type', 'latitude_enabled', 'table_index', 'table_index64',
             'latitude_dir', 'gpu')
    s.update({name: c.u8() for name in names})
    (s['sweep_longitude'], s['sweep_latitude'], s['sweep_start'], s['surface_random'],
     s['caliber_ratio']) = c.floats(5)
    s['line_center'], s['line_length'] = c.floats(2)
    s['radius'], s['form_scale'] = c.floats(3), c.floats(3)
    c.skip(4)
    s['mesh'] = c.u64()
    s['divide_circle'], s['divide_circle_random'] = c.i32(), c.i32()
    s['divide_line'], s['divide_line_random'] = c.i32(), c.i32()
    c.skip(8)  # trailing words of this version's record
    return s


def read_render_state(c):
    names = ('blend_enable', 'depth_test', 'depth_func', 'depth_write', 'alpha_test', 'alpha_func', 'blend_type',
             'display_side')
    r = {name: c.u8() for name in names}
    r['alpha_threshold'] = c.f32()
    c.skip(4)
    return r


def read_particle_data(c):
    names = ('infinite_life', 'trimming', 'billboard', 'rotation_type', 'offset_type', 'reverse_x', 'reverse_y',
             'reverse_z', 'rotate_x', 'rotate_y', 'rotate_z', 'primitive_scale_type', 'common_random',
             'connect_scale_z', 'avoid_z_fight', 'unknown')
    p = {name: c.u8() for name in names}
    p['life'], p['life_random'] = c.i32(), c.i32()
    p['momentum_random'] = c.f32()
    c.skip(4)
    p['mesh'] = c.u64()
    c.skip(8)
    loop_names = ('color0', 'alpha0', 'color1', 'alpha1', 'scale')
    p['loops'] = {name: c.u8() for name in loop_names}
    c.skip(7)
    p['loop_rates'] = list(c.take('5i'))
    return p


def read_shader_reference(c):
    shader = {'type': c.u8()}
    c.skip(3)
    shader['index'] = c.i32()
    c.skip(16 + 16 + 8)  # compute, user and custom shader indices, custom flags, extra indices
    shader['define1'] = c.text(16)
    shader['define2'] = c.text(16)
    return shader


def read_velocity(c):
    v = {'all_direction': c.f32(), 'designated_scale': c.f32(), 'designated_dir': c.floats(3),
         'diffusion_angle': c.f32(), 'xz_diffusion': c.f32(), 'diffusion': c.floats(3),
         'random': c.f32(), 'inherit': c.f32()}
    return v


def read_color(c):
    c.skip(8)
    color = {'type0': c.u8(), 'type1': c.u8(), 'alpha_type0': c.u8(), 'alpha_type1': c.u8()}
    color['color0'], color['alpha0'] = c.floats(3), c.f32()
    color['color1'], color['alpha1'] = c.floats(3), c.f32()
    return color


def read_scale(c):
    s = {'scale': c.floats(3), 'random': c.floats(3)}
    c.skip(4)
    s['min'], s['max'] = c.floats(2)
    return s


def read_sampler(c):
    s = {'texture': c.u64(), 'wrap_u': c.u8(), 'wrap_v': c.u8(), 'filter': c.u8(), 'sphere_map': c.u8()}
    c.skip(8 + 4 + 8)  # lod range, mip limit and density flags, unused words
    return s


def read_texture_animation(c):
    names = ('pattern', 'scroll', 'rotate', 'scale', 'repeat', 'inverse_random_u', 'inverse_random_v',
             'pattern_loop_random', 'uv_channel', 'crossfade')
    t = {name: c.u8() for name in names}
    c.skip(6)
    return t


COMBINER_FIELDS = ('color_process', 'alpha_process', 'tex1_color_blend', 'tex2_color_blend', 'prim_color_blend',
                   'tex1_alpha_blend', 'tex2_alpha_blend', 'prim_alpha_blend', 'tex_color0', 'tex_color1',
                   'tex_color2', 'tex_alpha0', 'tex_alpha1', 'tex_alpha2', 'prim_color', 'prim_alpha',
                   'shader_type', 'apply_alpha', 'distortion_by_camera', 'unused')


def read_emitter(data, position):
    """Decode the fixed-size emitter record starting at `position`."""
    c = Cursor(data, position)
    e = {'flag': c.u32(), 'seed': c.u32()}
    c.skip(8)
    e['name'] = c.text(64)
    e['static'] = read_static(c)
    e['info'] = read_info(c)
    e['inherit'] = read_inherit(c)
    e['emission'] = read_emission(c)
    e['shape'] = read_shape(c)
    e['render_state'] = read_render_state(c)
    e['particle'] = read_particle_data(c)
    e['combiner'] = {name: c.u8() for name in COMBINER_FIELDS}
    c.skip(4)
    e['shader'] = read_shader_reference(c)
    c.skip(4)
    e['velocity'] = read_velocity(c)
    e['color'] = read_color(c)
    e['scale'] = read_scale(c)
    c.skip(12)  # fluctuation flags
    e['samplers'] = [read_sampler(c) for _ in range(3)]
    e['texture_animations'] = [read_texture_animation(c) for _ in range(3)]
    return e


def read_attachments(data, section):
    """Animation attachments of an emitter: (magic, enable, loop, loop count, keys)."""
    found = []
    if section.attribute_offset == NONE:
        return found
    position = section.position + section.attribute_offset
    while True:
        attachment = Section(data, position)
        if attachment.magic in KEY_ANIMATIONS:
            c = Cursor(data, attachment.body)
            enable, loop, _random, _pad = c.u8(), c.u8(), c.u8(), c.u8()
            count, loop_count = c.u32(), c.u32()
            if count < 512:
                found.append({'kind': attachment.magic, 'enable': enable, 'loop': loop,
                              'loop_count': loop_count, 'keys': [c.floats(4) for _ in range(count)]})
        elif attachment.magic.startswith(('F', 'EP')):
            found.append({'kind': attachment.magic})  # field or plugin the simulation does not model
        if attachment.next_offset == NONE:
            return found
        position += attachment.next_offset


def read_emitter_tree(data, section):
    emitter = read_emitter(data, section.body)
    emitter['attachments'] = read_attachments(data, section)
    emitter['children'] = [read_emitter_tree(data, child) for child in section.children(data)]
    return emitter


def emitter_sets(data, esta):
    """Name and section of every emitter set."""
    for section in esta.children(data):
        c = Cursor(data, section.body)
        c.skip(16)
        yield c.text(64), section


def texture_names(data, grtf):
    """Texture id -> name from the descriptor child of the texture section."""
    names = {}
    descriptors = [s for s in grtf.children(data) if s.magic == 'GTNT']
    if not descriptors:
        raise ValueError('Texture section has no name table')
    position, end = descriptors[0].body, descriptors[0].body + descriptors[0].size
    while position < end:
        texture_id, step, length = struct.unpack_from('<QII', data, position)
        names[texture_id] = data[position + 16:position + 16 + length].split(b'\0')[0].decode('utf-8', 'replace')
        if not step:
            break
        position += step
    return names


def mesh_descriptors(data, g3pr):
    """(mesh id, attribute slots) of every mesh in the mesh section, in model order."""
    found = []
    for child in g3pr.children(data):
        if child.magic != 'G3NT':
            continue
        position, end = child.body, child.body + child.size
        while position < end:
            mesh_id, step, _eight = struct.unpack_from('<QII', data, position)
            found.append((mesh_id, struct.unpack_from('<6b', data, position + 16)))
            if not step:
                break
            position += step
    return found


def _round(values, digits=5):
    return [round(v, digits) for v in values]


def _channel(emitter, name, alpha=False):
    """Colour or alpha curve: constant, animated keys, or one random pick among keys."""
    static, color = emitter['static'], emitter['color']
    count = static['key_counts'][name]
    keys = [_round(k) for k in static[name + '_keys'][:count]]
    kind = color['alpha_type' + name[-1]] if alpha else color['type' + name[-1]]
    result = {'mode': 'random' if kind == 3 else ('keys' if count >= 2 else 'constant')}
    if alpha:
        result['constant'] = round(color[name], 5)
        if count:
            result['keys'] = [[k[0], k[3]] for k in keys]
    else:
        result['constant'] = _round(color[name])
        if count:
            result['keys'] = keys  # r, g, b, time
    return result


def _texture(emitter, slot, names):
    sampler = emitter['samplers'][slot]
    if sampler['texture'] == NO_ID or sampler['texture'] not in names:
        return None
    static = emitter['static']
    pattern, scroll = static['patterns'][slot], static['scrolls'][slot]
    animation = emitter['texture_animations'][slot]
    count = int(pattern['count'])
    table = pattern['table'][:count] if 0 < count <= 32 and any(pattern['table']) else None
    return {
        'name': names[sampler['texture']],
        'wrap': [WRAPS[min(sampler['wrap_u'], 3)], WRAPS[min(sampler['wrap_v'], 3)]],
        'grid': [max(int(scroll['grid'][0]), 1), max(int(scroll['grid'][1]), 1)],
        'pattern': {'type': PATTERNS[min(animation['pattern'], 4)], 'count': count,
                    'frames_per_tile': pattern['frequency'], 'random_range': pattern['random_range'],
                    'table': table, 'loop_random': animation['pattern_loop_random']},
        'scroll': _round(scroll['scroll_add']), 'scroll_start': _round(scroll['scroll']),
        'scale_add': _round(scroll['scale_add']), 'scale_start': _round(scroll['scale']),
        'rotation_add': round(scroll['rotation_add'], 5),
        'animated': [animation['scroll'], animation['rotate'], animation['scale']]}


def blend_mode(render_state):
    if not render_state['blend_enable'] and render_state['alpha_test']:
        return 'cutout'
    return 'add' if render_state['blend_type'] == 1 else 'alpha'


def describe_emitter(emitter, names):
    """Plain named structure of one emitter (and its children) for the game."""
    static, particle, emission = emitter['static'], emitter['particle'], emitter['emission']
    info, velocity, shape = emitter['info'], emitter['velocity'], emitter['shape']
    render_state, shader = emitter['render_state'], emitter['shader']
    sections = [dict(a, keys=[_round(k) for k in a['keys']]) if 'keys' in a else a for a in emitter['attachments']]
    mesh = None if particle['mesh'] == NO_ID else '0x%x' % particle['mesh']
    return {
        'name': emitter['name'],
        'emission': {
            'one_time': bool(emission['one_time']), 'start': emission['start'], 'duration': emission['duration'],
            'interval': emission['interval'] + 1, 'interval_random': emission['interval_random'],
            'rate': emission['rate'], 'rate_random': emission['rate_random'],
            'child_timing': emission['timing'] if emitter['inherit']['enable_emitter_particle'] else None,
            # Percent of the parent particle's life at which an ordinary child emits (the field is
            # read for every emitter; only the ones inside a parent use it).
            'timing': emission['timing'],
            'position_random': emission['position_random']},
        'life': {'frames': particle['life'], 'random': particle['life_random'], 'infinite': bool(particle['infinite_life'])},
        'shape': {
            'volume': VOLUMES[min(shape['volume'], 15)], 'radius': _round(shape['radius']),
            'form_scale': _round(shape['form_scale']),
            'sweep_longitude': round(shape['sweep_longitude'], 5), 'sweep_latitude': round(shape['sweep_latitude'], 5),
            'sweep_start': round(shape['sweep_start'], 5), 'arc_type': shape['arc_type'],
            'caliber_ratio': shape['caliber_ratio'], 'surface_random': round(shape['surface_random'], 5),
            'line_center': shape['line_center'], 'line_length': shape['line_length'],
            'divide_circle': shape['divide_circle'], 'divide_circle_random': shape['divide_circle_random'],
            'divide_line': shape['divide_line'], 'divide_line_random': shape['divide_line_random']},
        'velocity': {
            'all_direction': velocity['all_direction'], 'direction': _round(velocity['designated_dir']),
            'direction_speed': velocity['designated_scale'], 'cone_degrees': velocity['diffusion_angle'],
            'xz_diffusion': velocity['xz_diffusion'], 'diffusion': _round(velocity['diffusion']),
            'random': velocity['random'], 'inherit': velocity['inherit']},
        'forces': {
            'gravity_dir': _round(emission['gravity_dir']), 'gravity': emission['gravity_scale'],
            'world_gravity': bool(emission['world_gravity']), 'air_resistance': static['air_resistance'],
            'momentum_random': particle['momentum_random']},
        'scale': {
            'base': _round(emitter['scale']['scale']), 'random': _round(emitter['scale']['random']),
            'curve': [_round(k) for k in static['scale_keys'][:static['key_counts']['scale']]],
            'loop': bool(particle['loops']['scale']), 'add_velocity': static['add_velocity_to_scale']},
        'rotation': {
            'start': _round(static['rotate_init']), 'start_random': _round(static['rotate_init_random']),
            'add': _round(static['rotate_add']), 'add_random': _round(static['rotate_add_random']),
            'resistance': static['rotate_resistance'], 'type': particle['rotation_type'],
            'axes': [particle['rotate_x'], particle['rotate_y'], particle['rotate_z']],
            'reverse': [particle['reverse_x'], particle['reverse_y'], particle['reverse_z']]},
        'color': {
            'color0': _channel(emitter, 'color0'), 'alpha0': _channel(emitter, 'alpha0', True),
            'color1': _channel(emitter, 'color1'), 'alpha1': _channel(emitter, 'alpha1', True),
            'scale': static['color_scale'], 'tint0': _round(info['color0']), 'tint1': _round(info['color1']),
            'loop_color0': bool(particle['loops']['color0']), 'loop_alpha0': bool(particle['loops']['alpha0']),
            'loop_frames': particle['loop_rates'][:5],
            'alpha_fade_time': info['alpha_fade_time'], 'fade_in_time': info['fade_in_time'],
            'alpha_fade': info['alpha_fade'], 'alpha_fade_in': info['alpha_fade_in'],
            'scale_fade': info['scale_fade']},
        'textures': [_texture(emitter, slot, names) for slot in range(3)],
        'render': {
            'blend': blend_mode(render_state), 'depth_test': bool(render_state['depth_test']),
            'depth_write': bool(render_state['depth_write']), 'cull': render_state['display_side'],
            'alpha_threshold': round(render_state['alpha_threshold'], 4), 'billboard': particle['billboard'],
            'sort_type': info['sort_type'], 'mesh': mesh,
            'combiner': {k: v for k, v in emitter['combiner'].items() if v},
            'shader_type': {0: 'default', 1: 'distortion', 2: 'lit'}.get(shader['type'], 'default')},
        'follow': FOLLOWS[min(info['follow_type'], 2)],
        'transform': {'translate': _round(info['trans']), 'translate_random': _round(info['trans_random']),
                      'rotate': _round(info['rotate']), 'rotate_random': _round(info['rotate_random']),
                      'scale': _round(info['scale'])},
        'attachments': sections,
        'children': [describe_emitter(child, names) for child in emitter['children']],
        '_shader_index': shader['index'],
    }


def walk(emitters):
    """Every emitter in a tree, parents before children."""
    for emitter in emitters:
        yield emitter
        yield from walk(emitter['children'])


def self_check():
    section = struct.pack('<4s7I', b'EMTR', 64, NONE, NONE, NONE, 32, 0, 0)
    parsed = Section(section, 0)
    assert (parsed.magic, parsed.body, list(parsed.children(section))) == ('EMTR', 32, [])
    cursor = Cursor(struct.pack('<BIf', 7, 9, 1.5) + b'abc\0xyz')
    assert (cursor.u8(), cursor.u32(), cursor.f32(), cursor.text(8)) == (7, 9, 1.5, 'abc')
    names = {5: 'smoke'}
    stub = {'samplers': [{'texture': 5, 'wrap_u': 1, 'wrap_v': 0}, {'texture': NO_ID}, {'texture': NO_ID}],
            'static': {'patterns': [{'count': 2., 'frequency': 3., 'random_range': 0., 'table': [0, 1] + [0] * 30}] * 3,
                       'scrolls': [{'scroll_add': [0, 0], 'scroll': [0, 0], 'scale_add': [0, 0], 'scale': [1, 1],
                                    'rotation_add': 0., 'grid': [8., 4.]}] * 3},
            'texture_animations': [{'pattern': 3, 'scroll': 0, 'rotate': 0, 'scale': 0, 'pattern_loop_random': 1}] * 3}
    texture = _texture(stub, 0, names)
    assert texture['grid'] == [8, 4] and texture['pattern']['type'] == 'loop' and texture['wrap'] == ['repeat', 'mirror']
    assert _texture(stub, 1, names) is None
    assert blend_mode({'blend_enable': 0, 'alpha_test': 1, 'blend_type': 0}) == 'cutout'
    assert blend_mode({'blend_enable': 1, 'alpha_test': 0, 'blend_type': 1}) == 'add'
    assert math.isclose(_round([0.123456789])[0], 0.12346)
    print('Particle file checks passed')

