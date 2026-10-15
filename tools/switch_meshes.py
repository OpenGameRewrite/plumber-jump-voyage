"""Read the small meshes particles use (an embedded Switch BFRES) for extract-effects.py.

Only positions, first texture coordinates and triangles are decoded; the
particle renderer applies its own colour and lighting.
"""
import struct

# Vertex attribute format -> (struct code, components, normalisation divisor)
ATTRIBUTE_FORMATS = {
    0x0516: ('f', 1, 1.), 0x0517: ('f', 2, 1.), 0x0518: ('f', 3, 1.), 0x0519: ('f', 4, 1.),
    0x050a: ('e', 1, 1.), 0x0512: ('e', 2, 1.), 0x0515: ('e', 4, 1.),
    0x0109: ('B', 2, 255.), 0x010b: ('B', 4, 255.), 0x0209: ('b', 2, 127.), 0x020b: ('b', 4, 127.),
    0x0112: ('H', 2, 65535.), 0x0115: ('H', 4, 65535.), 0x0212: ('h', 2, 32767.), 0x0215: ('h', 4, 32767.),
}
# Roles of the six attribute slots listed per mesh in the name table.
ROLES = ('position', 'normal', 'tangent', 'color', 'uv0', 'uv1')
TRIANGLES, STRIP = 3, 4
INDEX_CODES = {0: 'B', 1: 'H', 2: 'I'}


class Reader:
    """Random access over the mesh section, offsets as stored in the file."""

    def __init__(self, data):
        self.data = data

    def u16(self, offset):
        return struct.unpack_from('<H', self.data, offset)[0]

    def u32(self, offset):
        return struct.unpack_from('<I', self.data, offset)[0]

    def u64(self, offset):
        return struct.unpack_from('<Q', self.data, offset)[0]

    def text(self, offset):
        length = self.u16(offset)
        return self.data[offset + 2:offset + 2 + length].decode('utf-8', 'replace')


def read_vertex_buffer(file, offset, buffer_base):
    """One FVTX: vertex count, attribute table and the byte ranges holding them."""
    attributes = []
    attribute_table = file.u64(offset + 0x10)
    sizes, strides = file.u64(offset + 0x38), file.u64(offset + 0x40)
    buffer_offset = struct.unpack_from('<i', file.data, offset + 0x50)[0]
    attribute_count, buffer_count = file.data[offset + 0x54], file.data[offset + 0x55]
    count = file.u32(offset + 0x58)
    for index in range(attribute_count):
        item = attribute_table + index * 0x10
        attributes.append({
            'format': struct.unpack_from('>H', file.data, item + 8)[0],
            'offset': file.u16(item + 12), 'buffer': file.u16(item + 14)})
    buffers = []
    cursor = buffer_base + buffer_offset
    for index in range(buffer_count):
        size, stride = file.u32(sizes + index * 0x10), file.u32(strides + index * 0x10)
        cursor = (cursor + 7) // 8 * 8
        buffers.append((cursor, size, stride))
        cursor += size
    return {'count': count, 'attributes': attributes, 'buffers': buffers}


def read_attribute(file, vertex_buffer, index):
    """Decode one attribute of every vertex to tuples of floats."""
    attribute = vertex_buffer['attributes'][index]
    spec = ATTRIBUTE_FORMATS.get(attribute['format'])
    if spec is None:
        return None
    code, components, divisor = spec
    start, _size, stride = vertex_buffer['buffers'][attribute['buffer']]
    layout = struct.Struct('<' + code * components)
    values = []
    for vertex in range(vertex_buffer['count']):
        raw = layout.unpack_from(file.data, start + vertex * stride + attribute['offset'])
        values.append(tuple(v / divisor for v in raw) if divisor != 1. else tuple(raw))
    return values


def read_triangles(file, shape, buffer_base):
    """Triangle list of all mesh levels of a shape; strips become triangles."""
    triangles = []
    mesh_array, mesh_count = file.u64(shape + 0x20), file.data[shape + 0x67]
    for index in range(mesh_count):
        mesh = mesh_array + index * 0x38
        face_offset = file.u32(mesh + 0x20)
        primitive, index_format, count, first_vertex = struct.unpack_from('<IIII', file.data, mesh + 0x24)
        code = INDEX_CODES.get(index_format)
        if code is None or primitive not in (TRIANGLES, STRIP):
            continue
        start = buffer_base + face_offset
        indices = [i + first_vertex for i in struct.unpack_from(f'<{count}{code}', file.data, start)]
        if primitive == TRIANGLES:
            triangles += [tuple(indices[i:i + 3]) for i in range(0, count - 2, 3)]
        else:
            for i in range(count - 2):
                a, b, c = indices[i:i + 3]
                if a != b and b != c and a != c:
                    triangles.append((a, b, c) if i % 2 == 0 else (b, a, c))
        break  # the first mesh is the full-detail one
    return triangles


def read_meshes(data, descriptors):
    """Decode meshes of a BFRES; `descriptors` lists (mesh id, attribute slot per role)."""
    file = Reader(data)
    model_array = file.u64(0x28)
    buffer_info = file.u64(0x90)
    buffer_base = struct.unpack_from('<q', data, buffer_info + 8)[0]
    meshes = {}
    for index, (mesh_id, slots) in enumerate(descriptors):
        model = model_array + index * 0x78
        if data[model:model + 4] != b'FMDL':
            raise ValueError('Unexpected model table layout')
        name = file.text(file.u64(model + 0x10))
        buffer_count, shape_count = file.u16(model + 0x68), file.u16(model + 0x6a)
        if not buffer_count or not shape_count:
            continue
        vertex_buffers = file.u64(model + 0x28)
        shape = file.u64(model + 0x30)
        if data[shape:shape + 4] != b'FSHP':
            continue
        vertex_buffer = read_vertex_buffer(file, vertex_buffers + file.u16(shape + 0x62) * 0x60, buffer_base)
        decoded = {}
        for role, slot in zip(ROLES, slots):
            if role in ('position', 'uv0') and 0 <= slot < len(vertex_buffer['attributes']):
                decoded[role] = read_attribute(file, vertex_buffer, slot)
        if not decoded.get('position'):
            continue
        meshes[mesh_id] = {'name': name, 'position': decoded['position'], 'uv': decoded.get('uv0'),
                           'triangles': read_triangles(file, shape, buffer_base)}
    return meshes


def self_check():
    attribute = {'format': 0x0518, 'offset': 0, 'buffer': 0}
    packed = struct.pack('<6f', 1, 2, 3, 4, 5, 6)
    buffer = {'count': 2, 'attributes': [attribute], 'buffers': [(0, len(packed), 12)]}
    assert read_attribute(Reader(packed), buffer, 0) == [(1., 2., 3.), (4., 5., 6.)]
    half = {'count': 1, 'attributes': [{'format': 0x0109, 'offset': 0, 'buffer': 0}], 'buffers': [(0, 2, 2)]}
    assert read_attribute(Reader(bytes([255, 0])), half, 0) == [(1., 0.)]
    print('Mesh decoding checks passed')
