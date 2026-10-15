"""Read Switch BNTX texture archives and decode them to Pillow images.

Used by extract-effects.py. Handles the formats particle textures use: BCn
blocks, 8-bit grey, two-channel, 565 and 8888, stored in the Tegra X1
block-linear layout. Written from the public BRTI header layout and the public
description of the block-linear (GOB) addressing.
"""
import io
import struct

from PIL import Image

# BNTX format code -> (label, bytes per block, block width, DXGI code for BCn)
FORMATS = {
    0x0201: ('R8', 1, 1, 0),
    0x0701: ('R5G6B5', 2, 1, 0),
    0x0901: ('R8G8', 2, 1, 0),
    0x0b01: ('R8G8B8A8', 4, 1, 0),
    0x0b06: ('R8G8B8A8', 4, 1, 0),
    0x0c01: ('B8G8R8A8', 4, 1, 0),
    0x0c06: ('B8G8R8A8', 4, 1, 0),
    0x1a01: ('BC1', 8, 4, 71),
    0x1a06: ('BC1', 8, 4, 71),
    0x1b01: ('BC2', 16, 4, 74),
    0x1b06: ('BC2', 16, 4, 74),
    0x1c01: ('BC3', 16, 4, 77),
    0x1c06: ('BC3', 16, 4, 77),
    0x1d01: ('BC4', 8, 4, 80),
    0x1d02: ('BC4', 8, 4, 81),
    0x1e01: ('BC5', 16, 4, 83),
    0x1e02: ('BC5', 16, 4, 84),
    0x2001: ('BC7', 16, 4, 98),
    0x2006: ('BC7', 16, 4, 98),
}
GOB_WIDTH = 64  # bytes
GOB_HEIGHT = 8  # rows


def ceil_div(a, b):
    return -(-a // b)


def deswizzle(data, blocks_wide, blocks_high, block_bytes, gob_stack):
    """Convert block-linear storage to rows of blocks.

    The image is cut into 64x8-byte GOBs; `gob_stack` GOBs are stacked
    vertically before the next column starts. Inside a GOB, bytes interleave
    in a fixed 512-byte pattern.
    """
    pitch = ceil_div(blocks_wide * block_bytes, GOB_WIDTH) * GOB_WIDTH
    gobs_per_row = pitch // GOB_WIDTH
    stack_bytes = 512 * gob_stack
    out = bytearray(blocks_wide * blocks_high * block_bytes)
    size = len(data)
    for y in range(blocks_high):
        stack_row = y // (GOB_HEIGHT * gob_stack)
        row_base = stack_row * gobs_per_row * stack_bytes
        in_stack = y % (GOB_HEIGHT * gob_stack)
        row_offset = (in_stack // 8) * 512 + ((y % 8) // 2) * 64 + (y % 2) * 16
        for x in range(blocks_wide):
            byte_x = x * block_bytes
            address = (row_base + (byte_x // GOB_WIDTH) * stack_bytes + row_offset
                       + ((byte_x % 64) // 32) * 256 + ((byte_x % 32) // 16) * 32 + byte_x % 16)
            target = (y * blocks_wide + x) * block_bytes
            chunk = data[address:address + block_bytes]
            if len(chunk) == block_bytes and address < size:
                out[target:target + block_bytes] = chunk
    return bytes(out)


def dds_container(blocks, width, height, dxgi):
    """Wrap raw BCn blocks in a DX10 DDS file so Pillow can decode them."""
    pixel_format = struct.pack('<8I', 32, 4, int.from_bytes(b'DX10', 'little'), 0, 0, 0, 0, 0)
    header = struct.pack('<4s7I', b'DDS ', 124, 0x1007 | 0x80000, height, width, len(blocks), 0, 1)
    header += bytes(44) + pixel_format + struct.pack('<5I', 0x1000, 0, 0, 0, 0)
    return header + struct.pack('<5I', dxgi, 3, 0, 1, 0) + blocks


class Texture:
    """One BRTI texture of a BNTX archive."""

    def __init__(self, data, offset):
        if data[offset:offset + 4] != b'BRTI':
            raise ValueError('Expected a BRTI texture header')
        (_flags, _dim, _tile, _swizzle, _mips, _samples, self.format, _access,
         self.width, self.height) = struct.unpack_from('<BBHHHIIIII', data, offset + 0x10)
        _depth, _arrays, layout = struct.unpack_from('<III', data, offset + 0x2c)
        self.gob_stack = 1 << (layout & 7)
        self.image_size, _align, self.channels = struct.unpack_from('<III', data, offset + 0x50)
        name_offset, _parent, pointers = struct.unpack_from('<QQQ', data, offset + 0x60)
        length = struct.unpack_from('<H', data, name_offset)[0]
        self.name = data[name_offset + 2:name_offset + 2 + length].decode('utf-8', 'replace')
        self.first_mip = struct.unpack_from('<Q', data, pointers)[0]
        self.archive = data

    @property
    def label(self):
        info = FORMATS.get(self.format)
        return info[0] if info else None

    def decode(self):
        """RGBA image of the first mip level, or None for unsupported formats."""
        info = FORMATS.get(self.format)
        if info is None or self.width <= 0 or self.height <= 0:
            return None
        label, block_bytes, block_size, dxgi = info
        blocks_wide = ceil_div(self.width, block_size)
        blocks_high = ceil_div(self.height, block_size)
        raw = self.archive[self.first_mip:self.first_mip + self.image_size]
        linear = deswizzle(raw, blocks_wide, blocks_high, block_bytes, self.gob_stack)
        if block_size == 4:
            image = Image.open(io.BytesIO(dds_container(linear, blocks_wide * 4, blocks_high * 4, dxgi)))
            return image.convert('RGBA').crop((0, 0, self.width, self.height))
        size = (self.width, self.height)
        if label == 'R8':
            grey = Image.frombytes('L', size, linear)
            return Image.merge('RGBA', (grey, grey, grey, Image.new('L', size, 255)))
        if label == 'R8G8':
            pairs = Image.frombytes('LA', size, linear)  # two interleaved bytes
            red, green = pairs.split()
            return Image.merge('RGBA', (red, green, Image.new('L', size, 0), Image.new('L', size, 255)))
        if label == 'R5G6B5':
            pixels = bytearray()
            for (value,) in struct.iter_unpack('<H', linear):
                pixels += bytes(((value >> 11) * 255 // 31, ((value >> 5) & 63) * 255 // 63, (value & 31) * 255 // 31, 255))
            return Image.frombytes('RGBA', size, bytes(pixels))
        if label == 'B8G8R8A8':
            blue, green, red, alpha = Image.frombytes('RGBA', size, linear).split()
            return Image.merge('RGBA', (red, green, blue, alpha))
        return Image.frombytes('RGBA', size, linear)


def apply_channel_select(image, select):
    """Route source channels to R, G, B, A as the texture header asks.

    `select` holds one source per byte, red first: 0 zero, 1 one, 2..5 the
    red, green, blue or alpha channel of the decoded image.
    """
    bands = image.convert('RGBA').split()
    constant = {0: Image.new('L', image.size, 0), 1: Image.new('L', image.size, 255)}
    routed = []
    for index in range(4):
        source = (select >> (8 * index)) & 0xff
        routed.append(constant[source] if source in constant else bands[min(source - 2, 3)])
    if (select >> 24) & 0xff == 1:
        # A constant-one alpha: the texture's shape is in its colour (soft
        # ripples, flames), which particles use as alpha (inferred from how
        # the emitters use these textures).
        routed[3] = routed[0]
    return Image.merge('RGBA', routed)


def read_archive(data):
    """All textures of a BNTX archive."""
    if data[:4] != b'BNTX':
        raise ValueError('Expected a BNTX texture archive')
    count = struct.unpack_from('<I', data, 0x24)[0]
    table = struct.unpack_from('<Q', data, 0x28)[0]
    if count > 100000 or table + count * 8 > len(data):
        raise ValueError('Invalid BNTX texture table')
    return [Texture(data, offset) for offset in struct.unpack_from(f'<{count}Q', data, table)]


def self_check():
    # Linear data of one GOB-sized image survives the swizzle round trip layout.
    width = height = 8
    swizzled = bytearray(512)
    for y in range(height):
        for x in range(width):
            row = ((y // 2) * 64) + (y % 2) * 16
            swizzled[row + ((x * 4) // 16) * 32 + (x * 4) % 16] = y * width + x
    plain = deswizzle(bytes(swizzled), width, height, 4, 1)
    assert all(plain[(y * width + x) * 4] == y * width + x for y in range(height) for x in range(width))
    sample = Image.new('RGBA', (2, 1), (10, 20, 30, 40))
    # Red byte first: red <- green, green <- red, blue <- one, alpha <- zero.
    routed = apply_channel_select(sample, 3 | 2 << 8 | 1 << 16 | 0 << 24)
    assert routed.getpixel((0, 0)) == (20, 10, 255, 0)
    shaped = apply_channel_select(sample, 2 | 2 << 8 | 2 << 16 | 1 << 24)
    assert shaped.getpixel((0, 0)) == (10, 10, 10, 10), 'a constant-one alpha takes the brightness'
    # A constant-one alpha takes the brightness instead.
    shaped = apply_channel_select(sample, 2 | 2 << 8 | 2 << 16 | 1 << 24)
    assert shaped.getpixel((0, 0)) == (10, 10, 10, 10)
    try:
        read_archive(b'nope')
    except ValueError:
        pass
    else:
        raise AssertionError('Invalid texture archive accepted')
    print('Texture decoding checks passed')
