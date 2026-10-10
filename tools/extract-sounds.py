"""Install local movement sounds from your extracted RomFS using vgmstream-cli."""
import argparse
import json
from pathlib import Path
import struct
import subprocess
import wave
import extract
import importlib.util

spec=importlib.util.spec_from_file_location('outfits',Path(__file__).with_name('convert-outfits.py'))
outfits=importlib.util.module_from_spec(spec);spec.loader.exec_module(outfits)

class NW:
    def __init__(self,data):
        self.data=data
        if len(data)<20 or data[4:6] not in (b'\xff\xfe',b'\xfe\xff'):raise ValueError('Invalid local sound container')
        self.endian='<' if data[4:6]==b'\xff\xfe' else '>'
        self.blocks={}
        for i in range(self.u16(16)):
            entry=20+12*i;off=self.u32(entry+4);size=self.u32(entry+8)
            if off+size>len(data):raise ValueError('Sound block outside container')
            self.blocks[data[off:off+4]]=off
    def u16(self,off):return struct.unpack_from(self.endian+'H',self.data,off)[0]
    def u32(self,off):return struct.unpack_from(self.endian+'I',self.data,off)[0]
    def ref(self,off,base):
        relative=self.u32(off+4)
        if relative==0xffffffff:raise ValueError('Missing sound reference')
        target=base+relative
        if target>=len(self.data):raise ValueError('Sound reference outside container')
        return target
    def table(self,off):
        count=self.u32(off)
        if count>100000 or off+4+count*8>len(self.data):raise ValueError('Invalid sound reference table')
        return [self.ref(off+4+8*i,off) for i in range(count)]
    def part(self,off,size):
        if off+size>len(self.data):raise ValueError('Sound file outside container')
        return self.data[off:off+size]

def sound_index(data):
    n=NW(data);s=n.blocks[b'STRG']+8;table=n.ref(s,s)
    strings=[]
    for i in range(n.u32(table)):
        entry=table+4+i*12;start=n.ref(entry,table);size=n.u32(entry+8)
        strings.append(n.part(start,size).rstrip(b'\0').decode('utf-8'))
    base=n.blocks[b'INFO']+8
    tables=[n.table(n.ref(base+i*8,base)) for i in (0,3)]
    sounds={}
    for off in tables[0]:
        if n.u16(off+12)!=0x2202 or not n.u32(off+20)&1:continue
        sounds[strings[n.u32(off+24)]]=(n.u32(off),n.u32(n.ref(off+12,off)))
    wave_files=[n.u32(off) for off in tables[1]]
    return sounds,wave_files

def group_files(data):
    n=NW(data);table=n.blocks[b'INFO']+8;base=n.blocks[b'FILE']+8
    return {n.u32(off):n.part(n.ref(off+4,base),n.u32(off+12)) for off in n.table(table) if n.u32(off+8)!=0xffffffff}

def wave_for(files,wave_files,file_id,index):
    n=NW(files[file_id]);base=n.blocks[b'INFO']+8
    ids=n.ref(base,base);entries=n.table(n.ref(base+8,base));sound=entries[index]
    notes=n.table(n.ref(sound+16,sound));wave_id=n.u32(notes[0]);off=ids+4+wave_id*8
    archive=n.u32(off)&0xffffff;sample=n.u32(off+4)
    n=NW(files[wave_files[archive]]);table=n.blocks[b'INFO']+8;base=n.blocks[b'FILE']+8
    if sample>=n.u32(table):raise ValueError('Wave index outside archive')
    entry=table+4+sample*12
    return n.part(n.ref(entry,base),n.u32(entry+8))

def self_check():
    outfits.self_check()
    def container(blocks):
        offset=20+12*len(blocks);entries=b'';body=b''
        for name,data in blocks:
            block=name+struct.pack('<I',8+len(data))+data
            entries+=struct.pack('<HHII',0,0,offset,len(block));body+=block;offset+=len(block)
        return b'FWAR\xff\xfe'+struct.pack('<HIIHH',20,0,offset,len(blocks),0)+entries+body
    reference=lambda offset:struct.pack('<HHI',0,0,offset)
    sample=b'FWAV'+bytes(20)
    table=struct.pack('<I',1)+reference(0)+struct.pack('<I',len(sample))
    archive=container([(b'INFO',table),(b'FILE',sample)])
    # FWSD references one wave ID and one note in one sound entry.
    info=reference(16)+reference(28)+struct.pack('<III',1,0,0)
    info+=struct.pack('<I',1)+reference(12)
    info+=bytes(16)+reference(24)+struct.pack('<I',1)+reference(12)+struct.pack('<I',0)
    files={1:container([(b'INFO',info)]),2:archive}
    assert wave_for(files,[2],1,0)==sample
    try:NW(archive[:-1])
    except ValueError:pass
    else:raise AssertionError('Truncated sound container accepted')
    print('Sound archive checks passed')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--romfs',type=Path,default=extract.ROOT/'local/romfs')
    p.add_argument('--vgmstream',type=Path)
    p.add_argument('--list',action='store_true')
    p.add_argument('--self-check',action='store_true')
    a=p.parse_args()
    if a.self_check:self_check();return
    if not a.vgmstream and not a.list:p.error('Supply your installed --vgmstream executable')
    sound=a.romfs/'SoundData'
    sounds,wave_files=sound_index((sound/'SeData.bfsar').read_bytes())
    if a.list:
        print('\n'.join(name for name in sounds if name.startswith(('WsP','SeP'))));return
    files={}
    for archive in sound.glob('SeResource*.aras'):
        for name,data in extract.sarc(archive.read_bytes()):
            if data[:4]==b'FGRP':files.update(group_files(data))
    databases=outfits.archive(sound/'SeDataBase.szs');plays={}
    for name in ('Mario.byml','MarioVoice.byml','MarioFoot.byml','PlayerHakoniwaFoot.byml','MarioHead.byml'):
        for item in outfits.byml(databases[name]).get('PlayInfoList',[]):
            plays[item['Name']]=[r['Name'] for r in item.get('ResourceInfoList',[]) if r.get('Name') in sounds]
    actions={'jump':['vJump'],'double_jump':['vJump2nd'],'triple_jump':['vJump3rd'],
        'long_jump':['vJumpBroad'],'backflip':['vJumpTurn'],'sideflip':['vJumpTurn2'],
        'spin_jump':['SpinJump','vJump3rd'],'roll':['Roll'],'roll_jump':['vJump'],
        'dive':['HeadSlidingStart','vHeadSlide'],'pound':['HipDrop'], 'pound_jump':['JumpHipDrop','vJump3rd'],
        'land':['DamageLand','vLandStiffen'],'wall_jump':['WallJump','vJumpTurn'],'ledge':['WallCatchAccent'], 'pole':['WallCatchHand'],
        'swim':['SwimWalk'],'swim_dash':['SwimHeadSlidingStart'],'throw':[],
        'catch':[],'cap_bounce':['vJump3rd'], 'step':[]}
    direct={'throw':['WsPmCapFlyMat_loop','WsPvThrow'],'catch':['WsPmCapCatch'],
        'step':['WsPfFootSandal00','WsPfFootSandal01'],'cap_bounce':['WsPmSpring00']}
    temp=extract.ROOT/'local/audio';temp.mkdir(parents=True,exist_ok=True)
    output=extract.ROOT/'assets/local/sounds';output.mkdir(parents=True,exist_ok=True)
    manifest={};missing=[]
    (temp/'events.json').write_text(json.dumps(plays,indent=2),encoding='utf-8')
    for action,aliases in actions.items():
        names=list(dict.fromkeys([resource for alias in aliases for resource in plays.get(alias,[])]+[n for n in direct.get(action,[]) if n in sounds]))[:4]
        paths=[]
        for i,name in enumerate(names):
            file_id,index=sounds[name]
            raw=temp/(name+'.bfwav');raw.write_bytes(wave_for(files,wave_files,file_id,index))
            dst=output/(action+'_'+str(i)+'.wav')
            result=subprocess.run([str(a.vgmstream.resolve()),'-i','-o',str(dst),str(raw)],capture_output=True)
            if result.returncode:raise ValueError(f'vgmstream could not decode {name}')
            with wave.open(str(dst),'rb') as audio:
                if audio.getnframes()==0:raise ValueError('Decoded audio is empty')
            paths.append('local/sounds/'+dst.name)
        if paths:manifest[action]=paths
        else:missing.append(action)
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2),encoding='utf-8')
    print(f'Installed {sum(map(len,manifest.values()))} sound files for {len(manifest)} movement events.')
    if missing:print('No matching local event for: '+', '.join(missing)+'. See local/audio/events.json.')

if __name__=='__main__':
    try:main()
    except (OSError,ValueError,KeyError,IndexError,struct.error) as e:raise SystemExit(f'Sound extraction stopped: {e}')
