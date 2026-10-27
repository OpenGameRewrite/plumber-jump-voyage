"""Convert locally extracted Odyssey outfits using a separately installed Toolbox adapter."""
import argparse
import json
from pathlib import Path
import struct
import subprocess
import sys
import shutil
import extract
import importlib.util

spec=importlib.util.spec_from_file_location('pack',Path(__file__).with_name('pack-glb.py'))
packer=importlib.util.module_from_spec(spec);spec.loader.exec_module(packer)

def byml(data):
    """Read the small BYML dictionaries used by local costume metadata."""
    if data[:2] not in (b'YB',b'BY'):raise ValueError('Expected BYML')
    endian='<' if data[:2]==b'YB' else '>'
    def u32(offset):return struct.unpack_from(endian+'I',data,offset)[0]
    def count(offset):return int.from_bytes(data[offset+1:offset+4],'little' if endian=='<' else 'big')
    def strings(offset):
        if not offset:return []
        if data[offset]!=0xc2:raise ValueError('Expected BYML string table')
        result=[]
        for i in range(count(offset)):
            start=offset+u32(offset+4+i*4);end=data.index(0,start)
            result.append(data[start:end].decode('utf-8'))
        return result
    keys=strings(u32(4));values=strings(u32(8))
    def value(kind,offset,depth):
        if depth>32:raise ValueError('BYML nesting too deep')
        v=u32(offset)
        if kind in (0xc0,0xc1):return node(v,depth+1)
        if kind==0xa0:return values[v]
        if kind==0xd0:return bool(v)
        if kind==0xd1:return struct.unpack_from(endian+'i',data,offset)[0]
        if kind==0xd2:return struct.unpack_from(endian+'f',data,offset)[0]
        if kind==0xd3:return v
        if kind==0xff:return None
        raise ValueError(f'Unsupported BYML value type {kind:x}')
    def node(offset,depth):
        n=count(offset);kind=data[offset]
        if kind==0xc1:
            result={}
            for i in range(n):
                off=offset+4+i*8
                key=int.from_bytes(data[off:off+3],'little' if endian=='<' else 'big')
                result[keys[key]]=value(data[off+3],off+4,depth)
            return result
        if kind==0xc0:
            start=(offset+4+n+3)&~3
            return [value(data[offset+4+i],start+i*4,depth) for i in range(n)]
        raise ValueError('Expected BYML container')
    return node(u32(12),0)

def archive(path):return dict(extract.sarc(extract.yaz0(path.read_bytes())))

def self_check():
    # One real dictionary node with a string and signed integer, in both byte orders.
    for endian,magic in (('<',b'YB'),('>',b'BY')):
        table=lambda names: b'\xc2'+len(names).to_bytes(3,'little' if endian=='<' else 'big')+struct.pack(endian+'I'*len(names),*(4+4*len(names)+sum(len(n)+1 for n in names[:i]) for i in range(len(names))))+b''.join(n+b'\0' for n in names)
        keys=table([b'name',b'value']);values=table([b'explorer'])
        root=16+len(keys)+len(values)
        raw=magic+struct.pack(endian+'HIII',3,16,16+len(keys),root)+keys+values
        raw+=b'\xc1'+(2).to_bytes(3,'little' if endian=='<' else 'big')
        raw+=(0).to_bytes(3,'little' if endian=='<' else 'big')+b'\xa0'+struct.pack(endian+'I',0)
        raw+=(1).to_bytes(3,'little' if endian=='<' else 'big')+b'\xd1'+struct.pack(endian+'i',-7)
        assert byml(raw)=={'name':'explorer','value':-7}
    try:byml(b'bad')
    except ValueError:pass
    else:raise AssertionError('Invalid metadata accepted')
    print('Outfit metadata checks passed')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--adapter',type=Path,default=extract.ROOT/'local/converter/convert.exe')
    p.add_argument('--romfs',type=Path,default=extract.ROOT/'local/romfs')
    p.add_argument('--outfit',action='append',help='Archive name, e.g. Mario or MarioAloha; repeat to select several')
    p.add_argument('--list',action='store_true')
    p.add_argument('--inspect',action='store_true')
    p.add_argument('--force',action='store_true',help='Regenerate existing local GLBs')
    p.add_argument('--repack',action='store_true',help='Repack existing conversions (needs --keep-intermediates on the earlier run)')
    p.add_argument('--add-clip',action='append',help='Convert this Mario clip (repeatable) and append it to the installed Mario file; nothing else changes')
    p.add_argument('--curves',action='store_true',help='Write assets/local/Mario.curves.bin: the original curves of every clip the installed Mario.glb holds (nothing else changes)')
    p.add_argument('--keep-intermediates',action='store_true',help='Keep the converter working files (DAE, glTF, PNG)')
    p.add_argument('--self-check',action='store_true')
    a=p.parse_args()
    if a.self_check:self_check();return
    candidates={}
    paths=[a.romfs/'ObjectData'/f'{n}.szs' for n in a.outfit] if a.outfit else sorted((a.romfs/'ObjectData').glob('Mario*.szs'))
    for file in paths:
        files=archive(file)
        if 'InitPartsFixInfoHandL.byml' in files and 'InitSubActor.byml' in files:candidates[file.stem]=files
    if a.list:print('\n'.join(candidates));return
    names=a.outfit or list(candidates)
    if a.inspect:
        for name in names:
            print(name)
            for key in ('InitCostumeInfoBody.byml','InitSubActor.byml'):
                if key in candidates[name]:print(key,json.dumps(byml(candidates[name][key]),indent=2))
        return
    if not a.adapter.is_file():raise ValueError('Build the adapter with tools/build-converter.ps1 first')
    objects=extract.ROOT/'local/objects';exports=extract.ROOT/'local/outfits'
    exports.mkdir(parents=True,exist_ok=True)
    def model(name):
        file=objects/name/(name+'.bfres')
        if not file.is_file():raise ValueError(f'Missing extracted model {name}; run extract.py first')
        return file
    base_files=candidates.get('Mario') or archive(a.romfs/'ObjectData'/'Mario.szs')
    def placement(files,suffix):
        """The costume's attachment joint and local rotation/offset for one part."""
        raw=files.get(f'InitPartsFixInfo{suffix}.byml') or base_files.get(f'InitPartsFixInfo{suffix}.byml')
        if not raw:return ''
        info=byml(raw)
        if not info.get('JointName'):return ''
        rot=info.get('LocalRotate') or {};trans=info.get('LocalTrans') or {}
        values=[rot.get(k,0.) for k in 'XYZ']+[trans.get(k,0.) for k in 'XYZ']
        return '|'+info['JointName']+''.join(f'|{float(v):g}' for v in values)
    def body_parts(name,files):
        """The separate meshes (head, hands, hat) the body outfit is assembled from."""
        parts={}
        head=name+'Head' if (objects/(name+'Head')).is_dir() else 'MarioHead'
        parts[head]=placement(files,'Head')
        for item in byml(files['InitSubActor.byml']).get('CreatorList',[]):
            if item.get('ClassName')=='PartsModel' and item.get('ModelName'):
                parts.setdefault(item['ModelName'],placement(files,item.get('FixFileSuffixName','')))
        # Texture-only archives are not independent meshes.
        for part in [name+'BodyTexture',head+'Texture']+[p.replace('HandL','HandTexture').replace('HandR','HandTexture') for p in parts if 'Hand' in p]:
            if (objects/part).is_dir():parts.setdefault(part,'')
        return parts
    def mario_animation():
        """The converter arguments that bring in Mario's body, finger and facial clips."""
        return [str(model('PlayerAnimation')),'--hand-l='+str(model('PlayerAnimationHandL')),'--hand-r='+str(model('PlayerAnimationHandR'))]+[
            '--'+part.lower()+'='+str(model('PlayerAnimation'+part)) for part in ('Eye','Face','Head')]
    if a.curves:
        # The game evaluates the clips' curves between frames; the converter writes them
        # exactly as stored, for the clips the installed Mario.glb already has.
        installed=extract.ROOT/'assets/local/Mario.glb'
        if not installed.is_file():raise ValueError('Install Mario.glb first (run this script without --curves)')
        clips=[clip['name'] for clip in extract.glb_json(installed).get('animations',[])]
        scratch=exports/'_curves';scratch.mkdir(exist_ok=True)
        output=scratch/'Mario.curves.bin'
        command=[str(a.adapter.resolve()),str(model('Mario')),str(output)]+mario_animation()+['--curves-only']
        command+=['--clip='+clip for clip in clips]
        command+=['--part='+str(model(part))+fix for part,fix in body_parts('Mario',candidates['Mario']).items()]
        with output.with_suffix('.log').open('w',encoding='utf-8') as f:result=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT)
        if result.returncode or not output.is_file():raise ValueError(f'Curve export failed; details in {output.with_suffix(".log")}')
        if output.read_bytes()[:8] != b'PJVCURV1':
            raise ValueError('Adapter did not write skeletal curves; rebuild it with tools/build-converter.ps1 or select a curves-capable --adapter. Installed curves were preserved.')
        shutil.copyfile(output,extract.ROOT/'assets/local/Mario.curves.bin')
        print(f'Wrote assets/local/Mario.curves.bin for {len(clips)} clips')
        if not a.keep_intermediates:shutil.rmtree(scratch,ignore_errors=True)
        return
    if a.add_clip:
        # Bake only the named clips and append them to the installed Mario file,
        # leaving everything it holds as it is (older builds read the same file).
        scratch=exports/'_clips';scratch.mkdir(exist_ok=True)
        output=scratch/'Mario.glb'
        files=candidates['Mario']
        command=[str(a.adapter.resolve()),str(model('Mario')),str(output)]+mario_animation()
        command+=['--clip='+clip for clip in a.add_clip]
        command+=['--part='+str(model(part))+fix for part,fix in body_parts('Mario',files).items()]
        with output.with_suffix('.log').open('w',encoding='utf-8') as f:result=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT)
        if result.returncode:raise ValueError(f'Clip conversion failed; details in {output.with_suffix(".log")}')
        packed=packer.pack(output.with_suffix('.gltf'))
        added=[]
        for target in (exports/'Mario.glb',extract.ROOT/'assets/local/Mario.glb'):
            if target.is_file():added=packer.add_animations(target,packed,set(a.add_clip))
        print('Added clips: '+(', '.join(added) or 'none (all were already installed)'))
        if not a.keep_intermediates:shutil.rmtree(scratch,ignore_errors=True)
        return
    for name in names:
        if name not in candidates:raise ValueError(f'No body outfit named {name}; use --list')
        files=candidates[name]
        parts=body_parts(name,files)
        output=exports/(name+'.glb')
        base=exports/'Mario.glb'
        if output.is_file() and not a.force:
            # Outfits share Mario's clips; older packs carried a full copy each.
            shares=extract.glb_json(output).get('extras',{}).get('animation_source')
            if a.repack or name!='Mario' and base.is_file() and not shares:
                if not output.with_suffix('.gltf').is_file():
                    raise ValueError(f'{name}: the conversion files were cleaned up; run with --force instead')
                packer.pack(output.with_suffix('.gltf'),None if name=='Mario' else base)
            continue
        # Mario carries the body, fingers and facial clips;
        # other outfits share them by bone name.
        animation=['--geometry-only'] if name!='Mario' and base.is_file() else mario_animation()
        if animation[0]=='--geometry-only':
            animation+=['--'+part.lower()+'='+str(model('PlayerAnimation'+part)) for part in ('Eye','Face','Head')]
        command=[str(a.adapter.resolve()),str(model(name)),str(output)]+animation+['--part='+str(model(part))+fix for part,fix in parts.items()]
        log=output.with_suffix('.log')
        with log.open('w',encoding='utf-8') as f:result=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT)
        if result.returncode:raise ValueError(f'{name} conversion failed; details in {log}')
        packer.pack(output.with_suffix('.gltf'),base if animation[0]=='--geometry-only' else None)
    caps=exports/'caps';caps.mkdir(exist_ok=True)
    cap_output=extract.ROOT/'assets/local/caps';cap_output.mkdir(parents=True,exist_ok=True)
    for name in names:
        cap=name+'Cap' if (objects/(name+'Cap')).is_dir() else 'MarioCap'
        output=caps/(cap+'.glb')
        if not output.is_file() or a.force:
            with output.with_suffix('.log').open('w',encoding='utf-8') as log:
                texture=cap.removesuffix('Cap')+'HeadTexture'
                if not (objects/texture).is_dir():texture='MarioHeadTexture'
                result=subprocess.run([str(a.adapter.resolve()),str(model(cap)),str(output),'--geometry-only','--part='+str(model(texture))],stdout=log,stderr=subprocess.STDOUT)
            if result.returncode:raise ValueError(f'{cap} conversion failed; see {output.with_suffix(".log")}')
            packer.pack(output.with_suffix('.gltf'))
        extract.place(output,cap_output/output.name)
    eyes=caps/'CapManHeroEyes.glb'
    if not eyes.is_file() or a.force:
        with eyes.with_suffix('.log').open('w',encoding='utf-8') as log:
            result=subprocess.run([str(a.adapter.resolve()),str(model('CapManHeroEyes')),str(eyes),'--geometry-only'],stdout=log,stderr=subprocess.STDOUT)
        if result.returncode:raise ValueError('Cap eyes conversion failed; see '+str(eyes.with_suffix('.log')))
        packer.pack(eyes.with_suffix('.gltf'))
    extract.place(eyes,cap_output/eyes.name)
    extract.install_exports(exports,extract.ROOT/'assets/local',0.01,3.141592653589793)
    if not a.keep_intermediates:
        # The packed GLBs hold everything; the converter's working files are large.
        freed=0
        for folder in (exports,caps):
            for f in folder.iterdir():
                if f.is_file() and (f.suffix in ('.dae','.gltf','.bin','.png') or f.name.endswith('.expressions.json')):
                    freed+=f.stat().st_size;f.unlink()
        print(f'Removed {freed/1e9:.1f} GB of conversion working files.')

if __name__=='__main__':
    try:main()
    except (ValueError,OSError,KeyError,IndexError,struct.error) as e:raise SystemExit(f'Outfit conversion stopped: {e}')
