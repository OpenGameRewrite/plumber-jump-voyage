"""Local-only extraction wrapper. Supply your own ROM, keys and external tools."""
import argparse
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 512 * 1024 * 1024

def safe_name(name):
    p = PurePosixPath(name.replace('\\', '/'))
    if p.is_absolute() or '..' in p.parts or ':' in name or not p.parts:
        raise ValueError('Unsafe archive path')
    return p

def pfs_entries(path):
    with path.open('rb') as f:
        h = f.read(16)
        if len(h) != 16:
            raise ValueError('Truncated NSP')
        magic, count, names_size, _ = struct.unpack('<4sIII', h)
        if magic != b'PFS0' or count > 100000 or names_size > 16 * 1024 * 1024:
            raise ValueError('Expected an NSP/PFS0 container')
        entries = f.read(count * 24)
        names = f.read(names_size)
        base = 16 + count * 24 + names_size
        size = path.stat().st_size
        result = []
        for i in range(count):
            offset, length, name_offset, _ = struct.unpack_from('<QQII', entries, i * 24)
            end = names.find(b'\0', name_offset)
            if name_offset >= len(names) or end < 0 or base + offset + length > size:
                raise ValueError('Invalid NSP entry bounds')
            name = names[name_offset:end].decode('utf-8')
            safe_name(name)
            result.append((name, base + offset, length))
        return result

def program_nca(rom, output):
    entries = [e for e in pfs_entries(rom) if e[0].endswith('.nca')]
    if not entries:
        raise ValueError('NSP contains no NCA')
    name, offset, length = max(entries, key=lambda e: e[2])
    output.mkdir(parents=True, exist_ok=True)
    dst = output / name
    if dst.exists() and dst.stat().st_size == length:
        return dst
    partial = dst.with_suffix('.partial')
    with rom.open('rb') as src, partial.open('wb') as target:
        src.seek(offset)
        remaining = length
        while remaining:
            b = src.read(min(remaining, 8 * 1024 * 1024))
            if not b:
                raise ValueError('Truncated NSP payload')
            target.write(b)
            remaining -= len(b)
    partial.replace(dst)
    return dst

def title_keys(path):
    result = {}
    for line in path.read_text(encoding='utf-8-sig').splitlines():
        match = re.fullmatch(r'\s*([0-9a-fA-F]{32})\s*[,=]\s*([0-9a-fA-F]{32})\s*(?:#.*)?', line)
        if match:
            result[match[1].lower()] = match[2]
    return result

def ticket_keys(rom, titles):
    with rom.open('rb') as f:
        for name, offset, size in pfs_entries(rom):
            if not name.endswith('.tik') or size > 16384:
                continue
            f.seek(offset)
            data = f.read(size)
            # Common RSA-2048 Switch tickets: encrypted key block and rights ID.
            if len(data) >= 0x2b0 and data[:4] in (b'\x00\x01\x00\x04',b'\x04\x00\x01\x00') and data[0x281] == 0:
                titles[data[0x2a0:0x2b0].hex()] = data[0x180:0x190].hex()

def normalized_keys(path):
    # Ignore unrelated malformed/newer device-key lines unsupported by hactool.
    lines = []
    for line in path.read_text(encoding='utf-8-sig').splitlines():
        if '=' not in line:
            continue
        name, value = line.split('=', 1)
        value = value.strip()
        if len(value) in (32, 64) and re.fullmatch(r'[0-9a-fA-F]+', value):
            lines.append(name.strip() + ' = ' + value)
    target = ROOT / 'local/keys/prod.keys'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('\n'.join(lines), encoding='ascii')
    return target

def run_tool(tool, keys, nca, titles, extra=()):
    cmd = [str(tool), '-k', str(keys), '-t', 'nca', '-i', str(nca)]
    info = subprocess.run(cmd, capture_output=True, text=True, errors='replace')
    output = info.stdout + info.stderr
    if not re.search(r'Magic:\s+NCA[23]', output):
        raise ValueError('NCA header could not be read; check prod.keys and tool compatibility')
    rights = re.search(r'Rights ID:\s*([0-9A-Fa-f]{32})', output)
    if rights and int(rights[1], 16):
        key = titles.get(rights[1].lower())
        if not key:
            raise ValueError('The program NCA rights ID has no matching entry in title.keys')
        cmd[1:1] = ['--titlekey', key]
    title = re.search(r'Title ID:\s*([0-9A-Fa-f]{16})', output)
    content = re.search(r'Content Type:\s*(\w+)', output)
    if not title or title[1].lower() not in ('0100000000010000', '0100000000010800') or not content or content[1].lower() != 'program':
        raise ValueError('The selected NCA is not an Odyssey base/update program')
    if extra:
        cmd.remove('-i')
        cmd[1:1] = ['-x', *map(str, extra)]
    # Never echo tool output: some versions print decrypted key material.
    result = subprocess.run(cmd, capture_output=True, text=True, errors='replace')
    if result.returncode or 'corrupted' in result.stdout + result.stderr:
        raise ValueError('External extraction failed; check your keys, base/update pair and tool version')
    return title[1].lower()

def yaz0(data):
    if data[:4] != b'Yaz0':
        return data
    size = struct.unpack_from('>I', data, 4)[0]
    if size > LIMIT:
        raise ValueError('Yaz0 output exceeds safety limit')
    output = bytearray()
    pos = 16
    while len(output) < size:
        code = data[pos]
        pos += 1
        for bit in range(7, -1, -1):
            if len(output) >= size:
                break
            if code & (1 << bit):
                output.append(data[pos]); pos += 1
            else:
                a, b = data[pos:pos + 2]; pos += 2
                distance = ((a & 15) << 8 | b) + 1
                length = a >> 4
                if not length:
                    length = data[pos] + 0x12; pos += 1
                else:
                    length += 2
                if distance > len(output) or len(output) + length > size:
                    raise ValueError('Invalid Yaz0 backreference')
                for _ in range(length):
                    output.append(output[-distance])
    return bytes(output)

def sarc(data):
    if data[:4] != b'SARC':
        raise ValueError('Expected a SARC archive')
    endian = '>' if data[6:8] == b'\xfe\xff' else '<'
    hsize = struct.unpack_from(endian + 'H', data, 4)[0]
    size, payload = struct.unpack_from(endian + 'II', data, 8)
    if size != len(data) or data[hsize:hsize + 4] != b'SFAT':
        raise ValueError('Invalid SARC header')
    fh, count = struct.unpack_from(endian + 'HH', data, hsize + 4)
    node_start = hsize + fh
    sfnt = node_start + count * 16
    if data[sfnt:sfnt + 4] != b'SFNT':
        raise ValueError('Invalid SARC string table')
    strings = sfnt + struct.unpack_from(endian + 'H', data, sfnt + 4)[0]
    for i in range(count):
        _, attr, lo, hi = struct.unpack_from(endian + 'IIII', data, node_start + i * 16)
        if not attr >> 24 or not 0 <= lo <= hi <= len(data) - payload:
            raise ValueError('Invalid SARC entry')
        off = strings + (attr & 0xffffff) * 4
        end = data.find(b'\0', off, payload)
        if end < 0:
            raise ValueError('Invalid SARC filename')
        name = data[off:end].decode('utf-8')
        safe_name(name)
        yield name, data[payload + lo:payload + hi]

# What the conversion and sound tools read from the RomFS; --clean keeps only these.
OBJECT_WORDS = ('mario', 'player', 'cap', 'costume')
SOUND_FILES = ('SeData.bfsar', 'SeDataBase.szs')

def needed(path, romfs):
    rel = path.relative_to(romfs).parts
    if rel[0] == 'ObjectData':
        return len(rel) == 2 and path.suffix == '.szs' and any(w in path.stem.lower() for w in OBJECT_WORDS)
    if rel[0] == 'SoundData':
        return len(rel) == 2 and (path.name in SOUND_FILES or path.name.startswith('SeResource'))
    return False

def place(source, target):
    """Install a file without a second copy on disk when the volume allows it."""
    if target.resolve() == source.resolve():
        return
    target.unlink(missing_ok=True)
    try:
        os.link(source, target)
    except OSError:
        shutil.copyfile(source, target)

def remove_intermediates(romfs=None):
    """Delete local extraction leftovers; with romfs, also its unused files."""
    local = (ROOT / 'local').resolve()
    freed = 0
    # Older manual extractions also left a decrypted base section and RomFS image.
    targets = [local / n for n in ('nca', 'base-section-raw.bin', 'base-section.bin', 'base.romfs')]
    if romfs is not None:
        romfs = romfs.resolve()
        if not romfs.is_relative_to(local):
            raise ValueError('Only a RomFS inside local/ is pruned')
        targets += [f for f in romfs.rglob('*') if f.is_file() and not needed(f, romfs)]
    for target in targets:
        if not target.resolve().is_relative_to(local) or not target.exists():
            continue
        files = [target] if target.is_file() else [f for f in target.rglob('*') if f.is_file()]
        freed += sum(f.stat().st_size for f in files)
        if target.is_file():
            target.unlink()
        else:
            shutil.rmtree(target)
    if romfs is not None:
        for folder in sorted((d for d in romfs.rglob('*') if d.is_dir()), key=lambda d: len(d.parts), reverse=True):
            if not any(folder.iterdir()):
                folder.rmdir()
    print(f'Removed {freed / 1e9:.1f} GB of local extraction leftovers.')

def extract_objects(romfs, output):
    obj = romfs / 'ObjectData'
    if not obj.is_dir():
        raise ValueError('RomFS extraction has no ObjectData folder')
    found = 0
    for archive in sorted(obj.glob('*.szs')):
        if not any(word in archive.stem.lower() for word in OBJECT_WORDS):
            continue
        raw = archive.read_bytes()
        for name, data in sarc(yaz0(raw)):
            if name.lower().endswith(('.bfres', '.bntx')):
                dst = output / archive.stem / safe_name(name)
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(data)
                found += 1
    if not found:
        raise ValueError('No player/cap BFRES archives were found')
    print(f'Extracted {found} local model/animation resources to {output}')

def glb_json(path):
    with path.open('rb') as f:
        header = f.read(20)
        if len(header) != 20:
            raise ValueError('Truncated GLB')
        magic, version, size, chunk_size, kind = struct.unpack('<4sIIII', header)
        if magic != b'glTF' or version != 2 or size != path.stat().st_size or kind != 0x4e4f534a or chunk_size > LIMIT:
            raise ValueError('Invalid GLB 2.0 header')
        model = json.loads(f.read(chunk_size))
    if not model.get('meshes') or not model.get('scenes'):
        raise ValueError('GLB must contain meshes and a scene')
    for item in model.get('buffers', []) + model.get('images', []):
        if 'uri' in item and not item['uri'].startswith('data:'):
            raise ValueError('Export a self-contained GLB with embedded textures/buffers')
    return model

def install_exports(exports, output, scale, yaw):
    if not math.isfinite(scale) or not 0 < scale < 100 or not math.isfinite(yaw):
        raise ValueError('Invalid scale/yaw')
    files = sorted(exports.glob('*.glb'))
    if not files:
        raise ValueError('No exported GLB outfits found')
    parsed = [(p, glb_json(p)) for p in files]
    shared=next((m.get('extras',{}).get('expressions',{}) for p,m in parsed if p.stem=='Mario'),{})
    if not shared:shared=next((m['extras']['expressions'] for _,m in parsed if m.get('extras',{}).get('expressions')), {})
    output.mkdir(parents=True, exist_ok=True)
    aliases = {'idle':('Wait','Idle'),'walk':('Walk',),'run':('Run','Dash','Walk'),'run_start':('RunStart',),'brake':('Brake',),
        'land':('Land',),'land_hard':('LandStiffen','Land'),'jump':('Jump','Fall'),'fall':('Fall',),
        'double_jump':('Jump2',),'triple_jump':('Jump3',),'long_jump':('JumpBroad',),'backflip':('JumpBack',),'sideflip':('JumpReverse',),
        'spin_jump':('SpinJump',),'pound_start':('HipDropStart',),'pound':('HipDrop','GroundPound'),'pound_land':('HipDropLand',),'pound_jump':('JumpHipDrop',),
        'roll_start':('RollingStart',),'roll':('Rolling','Roll'),'roll_end':('RollingEnd',),'roll_jump':('RollingJump',),
        'dive':('Dive','HipDrop'),'slide':('HeadSliding','Slide'),'wall_slide':('WallSlideL','WallKeep'),'wall_jump':('WallJump',),
        'ledge':('WallCatch',),'ledge_start':('WallCatchStart',),'pole':('PoleWait','PoleClimb'),'pole_climb':('PoleClimb','ClimbTree'),
        'cap_vault':('JumpCapLeapFrog',),'cap_bounce':('JumpNpcTrample','JumpCapLeapFrog'),
        'throw':('ThrowCap',),'throw_upper':('ThrowCapParts',),'throw_upper_air':('ThrowCapJumpParts',),'throw_air':('ThrowCapJump','ThrowCap'),'spin_throw':('SpinCapStart','SpinCap'),
        'swim':('SwimWait','Swim'),'swim_move':('SwimWalk','SwimWait'),'swim_dash':('SwimDive','SwimWait'),
        'crouch':('SquatWait','Squat','Crouch'),'crouch_start':('SquatStart',),'crawl':('SquatWalk','SquatWait')}
    outfits = []
    sources = {p.name: m for p, m in parsed}
    for file, model in parsed:
        # Outfits that share another file's clips (by bone path) name it.
        source = model.get('extras', {}).get('animation_source')
        if source is not None and (source not in sources or not re.fullmatch(r'[A-Za-z0-9_.-]+\.glb', source)):
            raise ValueError(f'{file.name} shares clips with a missing file {source}')
        names = [a.get('name', '') for a in (sources[source] if source else model).get('animations', [])]
        mapping = {state: next((n for prefix in prefixes for n in names if n.lower()==prefix.lower()),next((n for prefix in prefixes for n in names if n.lower().startswith(prefix.lower())), '')) for state, prefixes in aliases.items()}
        mapping = {k:v for k,v in mapping.items() if v}
        target = output / file.name
        place(file, target)
        name=file.stem.removeprefix('Mario') if file.stem.startswith('Mario') else file.stem
        name=re.sub(r'([a-z0-9])([A-Z])',r'\1 \2',name).replace('_',' ') or 'Classic plumber'
        name={'Dot3d':'8-bit'}.get(name,name)
        outfits.append({'name':name, 'file':'local/' + file.name, 'scale':scale,'yaw':yaw,'animations':mapping})
        if source:outfits[-1]['animation_file']='local/'+source
        expressions=model.get('extras',{}).get('expressions')
        if expressions and expressions!=shared:outfits[-1]['expressions']=expressions
        cap=file.stem+'Cap.glb'
        if not (output/'caps'/cap).is_file():cap='MarioCap.glb'
        if (output/'caps'/cap).is_file():outfits[-1]['cap_file']='local/caps/'+cap
    manifest = output / 'manifest.json'
    partial = manifest.with_suffix('.partial')
    partial.write_text(json.dumps({'outfits':outfits, 'expressions':shared}, separators=(',',':')), encoding='utf-8')
    partial.replace(manifest)
    print(f'Installed {len(outfits)} local outfits. Launch with cargo run --features local-assets; choose Outfit in the menu.')

def self_check():
    r = Path('romfs')
    assert needed(r/'ObjectData'/'MarioAloha.szs', r) and needed(r/'ObjectData'/'PlayerAnimation.szs', r)
    assert needed(r/'SoundData'/'SeData.bfsar', r) and needed(r/'SoundData'/'SeResource0.aras', r)
    assert not needed(r/'ObjectData'/'Kuribo.szs', r) and not needed(r/'StageData'/'Stage.szs', r)
    assert not needed(r/'SoundData'/'Bgm.bfstm', r)
    assert yaz0(b'Yaz0' + struct.pack('>I',3) + bytes(8) + b'\xe0abc') == b'abc'
    for name in ('../bad','/absolute','C:/bad','x/../../bad'):
        try: safe_name(name)
        except ValueError: pass
        else: raise AssertionError('Path traversal accepted')
    with tempfile.TemporaryDirectory() as temp:
        path = Path(temp)/'check.nsp'
        names = b'test.nca\0'
        path.write_bytes(struct.pack('<4sIII',b'PFS0',1,len(names),0)+struct.pack('<QQII',0,3,0,0)+names+b'abc')
        assert pfs_entries(path) == [('test.nca',49,3)]
    print('Archive/path checks passed')

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--rom',type=Path);p.add_argument('--update',type=Path)
    p.add_argument('--keys',type=Path);p.add_argument('--title-keys',type=Path)
    p.add_argument('--hactool',type=Path);p.add_argument('--romfs',type=Path)
    p.add_argument('--exports',type=Path);p.add_argument('--scale',type=float,default=1.)
    p.add_argument('--yaw',type=float,default=0.);p.add_argument('--probe',action='store_true')
    p.add_argument('--self-check',action='store_true')
    p.add_argument('--clean',action='store_true',help='Delete extraction leftovers and RomFS files the tools do not read')
    p.add_argument('--keep-intermediates',action='store_true',help='Keep the temporary NCA copies after extracting')
    a=p.parse_args()
    if a.self_check: self_check();return
    if a.clean: remove_intermediates(a.romfs or ROOT/'local/romfs');return
    if a.exports: install_exports(a.exports,ROOT/'assets/local',a.scale,a.yaw);return
    romfs=a.romfs
    if a.rom:
        if not a.keys or not a.title_keys or not a.hactool:
            p.error('--rom requires --keys, --title-keys and --hactool (all supplied locally)')
        for file in (a.rom,a.keys,a.title_keys,a.hactool):
            if not file.is_file():p.error(f'Local input does not exist: {file}')
        titles=title_keys(a.title_keys)
        ticket_keys(a.rom, titles)
        if a.update:ticket_keys(a.update, titles)
        keys=normalized_keys(a.keys)
        base=program_nca(a.rom,ROOT/'local/nca/base')
        title=run_tool(a.hactool.resolve(),keys,base,titles)
        print(f'Base NCA header and title-key lookup passed ({title}); keys were not printed.')
        patch=None
        if a.update:
            patch=program_nca(a.update,ROOT/'local/nca/update')
            title=run_tool(a.hactool.resolve(),keys,patch,titles)
            print(f'Update NCA header and title-key lookup passed ({title}).')
        if a.probe:return
        romfs=ROOT/'local/romfs'
        romfs.mkdir(parents=True,exist_ok=True)
        if patch:
            basefs=ROOT/'local/base-section-raw.bin'
            # BKTR base offsets include the IVFC region; --romfs strips that region.
            if not basefs.exists():run_tool(a.hactool.resolve(),keys,base,titles,['-r','--section1',basefs])
            run_tool(a.hactool.resolve(),keys,patch,titles,['--romfsdir',romfs,'--baseromfs',basefs])
        else:
            run_tool(a.hactool.resolve(),keys,base,titles,['--romfsdir',romfs])
        # The NCA copies and base section only produce the RomFS, and the tools
        # read a small part of that; the rest is removed unless kept on request.
        if not a.keep_intermediates and (romfs/'ObjectData').is_dir():remove_intermediates(romfs)
    if not romfs:p.error('Choose --rom, --romfs or --exports')
    extract_objects(romfs,ROOT/'local/objects')

if __name__=='__main__':
    try:main()
    except (ValueError,OSError,struct.error,IndexError,json.JSONDecodeError) as e:
        raise SystemExit(f'Extraction stopped: {e}')
