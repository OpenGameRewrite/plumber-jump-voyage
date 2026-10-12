"""Add rendering metadata to existing local GLBs, preserving their binary chunks."""
import argparse
import json
from pathlib import Path
import struct
import sys
import tempfile
import asset_contract


def annotate(path):
    raw = path.read_bytes()
    if len(raw) < 20:
        raise ValueError(f'{path}: truncated GLB')
    magic, version, total, size, kind = struct.unpack_from('<4sIIII', raw)
    if magic != b'glTF' or version != 2 or total != len(raw) or kind != 0x4e4f534a or size > len(raw)-20:
        raise ValueError(f'{path}: invalid GLB')
    model = json.loads(raw[20:20+size])
    asset_contract.annotate(model, metal='Metal' in path.stem)
    encoded = json.dumps(model, separators=(',', ':')).encode()
    encoded += b' ' * (-len(encoded) % 4)
    chunks = raw[20+size:]
    output = struct.pack('<4sIIII', magic, version, 20+len(encoded)+len(chunks), len(encoded), kind)+encoded+chunks
    temporary = path.with_suffix('.partial')
    temporary.write_bytes(output)
    temporary.replace(path)


def main():
    if sys.argv[1:] == ['--self-check']:
        self_check()
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('paths', nargs='+', type=Path, help='Local GLB files or folders containing them')
    args = parser.parse_args()
    count = 0
    for path in args.paths:
        files = sorted(path.rglob('*.glb')) if path.is_dir() else [path]
        for file in files:
            annotate(file)
            count += 1
    print(f'Updated metadata in {count} local GLBs; geometry, textures and animations preserved.')


def self_check():
    model = {'meshes': [{'name': 'Cap__CapMT', 'primitives': [{}]}],
             'nodes': [{'name': 'Armature_Hip'}]}
    encoded = json.dumps(model).encode()
    encoded += b' ' * (-len(encoded) % 4)
    binary = struct.pack('<II', 12, 0x004e4942)+struct.pack('<3f', 1, 2, 3)
    raw = struct.pack('<4sIIII', b'glTF', 2, 20+len(encoded)+len(binary), len(encoded), 0x4e4f534a)+encoded+binary
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder)/'sample.glb'
        path.write_bytes(raw)
        annotate(path)
        result = path.read_bytes()
        size = struct.unpack_from('<I', result, 12)[0]
        assert result[20+size:] == binary
        document = json.loads(result[20:20+size])
        assert document['meshes'][0]['primitives'][0]['extras']['pjv_part']['role'] == 'worn_cap'
        first = path.read_bytes()
        annotate(path)
        assert path.read_bytes() == first
        path.write_bytes(b'broken')
        try:
            annotate(path)
        except ValueError:
            pass
        else:
            raise AssertionError('Truncated asset accepted')
        assert path.read_bytes() == b'broken'
    print('GLB annotation preserves binary chunks and is idempotent')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, struct.error) as error:
        raise SystemExit(f'Asset annotation stopped: {error}')
