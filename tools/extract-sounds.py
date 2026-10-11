"""Install local movement sounds from your extracted RomFS using vgmstream-cli.

The game's own sound tables decide what plays: which sounds each animation
starts on which frame, what landing and cap events play, per-sound volume,
pitch and speed curves. Sequence-driven sounds (default footsteps, jumps and
landings) are rendered from their instrument bank into plain waves.
"""
import argparse
import array
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
    def u8(self,off):return self.data[off]
    def u16(self,off):return struct.unpack_from(self.endian+'H',self.data,off)[0]
    def u32(self,off):return struct.unpack_from(self.endian+'I',self.data,off)[0]
    def f32(self,off):return struct.unpack_from(self.endian+'f',self.data,off)[0]
    def kind(self,off):return self.u16(off)
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

WAVE_SOUND,SEQUENCE_SOUND=0x2202,0x2203

def sound_index(data):
    """Named sounds: wave sounds (file, index) and sequence sounds (file, start, banks)."""
    n=NW(data);s=n.blocks[b'STRG']+8;table=n.ref(s,s)
    strings=[]
    for i in range(n.u32(table)):
        entry=table+4+i*12;start=n.ref(entry,table);size=n.u32(entry+8)
        strings.append(n.part(start,size).rstrip(b'\0').decode('utf-8'))
    base=n.blocks[b'INFO']+8
    sound_table,bank_table,war_table=(n.table(n.ref(base+i*8,base)) for i in (0,2,3))
    sounds={}
    for off in sound_table:
        if not n.u32(off+20)&1:continue
        name=strings[n.u32(off+24)];kind=n.kind(off+12);detail=n.ref(off+12,off)
        # Volume 127 is unity in the authoring tool.
        info={'file':n.u32(off),'volume':n.u8(off+8)/127}
        if kind==WAVE_SOUND:
            info.update(kind='wave',index=n.u32(detail))
        elif kind==SEQUENCE_SOUND:
            ids=n.ref(detail,detail);flags=n.u32(detail+12)
            banks=[n.u32(ids+4+4*i) for i in range(n.u32(ids))]
            info.update(kind='seq',start=n.u32(detail+16) if flags&1 else 0,
                banks=[n.u32(bank_table[b&0xffffff]) for b in banks if b!=0xffffffff])
        else:continue
        sounds[name]=info
    return sounds,[n.u32(off) for off in war_table]

def group_files(data):
    n=NW(data);table=n.blocks[b'INFO']+8;base=n.blocks[b'FILE']+8
    return {n.u32(off):n.part(n.ref(off+4,base),n.u32(off+12)) for off in n.table(table) if n.u32(off+8)!=0xffffffff}

def archived_wave(files,wave_files,archive,sample):
    n=NW(files[wave_files[archive]]);table=n.blocks[b'INFO']+8;base=n.blocks[b'FILE']+8
    if sample>=n.u32(table):raise ValueError('Wave index outside archive')
    entry=table+4+sample*12
    return n.part(n.ref(entry,base),n.u32(entry+8))

def wave_for(files,wave_files,file_id,index):
    n=NW(files[file_id]);base=n.blocks[b'INFO']+8
    ids=n.ref(base,base);entries=n.table(n.ref(base+8,base));sound=entries[index]
    notes=n.table(n.ref(sound+16,sound));wave_id=n.u32(notes[0]);off=ids+4+wave_id*8
    return archived_wave(files,wave_files,n.u32(off)&0xffffff,n.u32(off+4))

def varlen(data,pos):
    value=0
    while True:
        b=data[pos];pos+=1;value=(value<<7)|(b&0x7f)
        if not b&0x80:return value,pos

# Argument sizes of sequence commands that do not affect a one-shot effect.
U8_COMMANDS={0xc0,0xc1,0xc2,0xc5,0xc6,0xc8,0xc9,0xca,0xcb,0xcc,0xcd,0xce,0xcf,0xd0,0xd1,0xd2,0xd3,0xd4,0xd5,0xd6,0xd7,0xd8,0xd9,0xda,0xdb,0xdc,0xdd,0xde,0xdf,0xb0}
S16_COMMANDS={0xe0,0xe1,0xe2,0xe3}

def sequence_notes(seq,start):
    """Notes of a one-shot effect sequence: (tick, program, key, velocity, transpose, bend, random)."""
    n=NW(seq);data=seq;origin=n.blocks[b'DATA']+8;notes=[]
    def track(pos,tick,budget=[4096]):
        program=0;transpose=0;bend=0;note_wait=True;random={};stack=[]
        while budget[0]>0:
            budget[0]-=1
            cmd=data[pos];pos+=1
            if cmd<0x80:
                key=cmd;velocity=data[pos];pos+=1;length,pos=varlen(data,pos)
                notes.append({'tick':tick,'program':program,'key':key+transpose,'velocity':velocity,'bend':bend,'random':dict(random)})
                if note_wait:tick+=length
            elif cmd==0x80:
                wait,pos=varlen(data,pos);tick+=wait
            elif cmd==0x81:
                program,pos=varlen(data,pos)
            elif cmd==0x88:
                # Open another track that starts at this moment.
                target=int.from_bytes(data[pos+1:pos+4],'little');pos+=4
                track(origin+target,tick)
            elif cmd==0x89:
                pos=origin+int.from_bytes(data[pos:pos+3],'little')
            elif cmd==0x8a:
                stack.append(pos+3);pos=origin+int.from_bytes(data[pos:pos+3],'little')
            elif cmd==0xfd:
                if not stack:return
                pos=stack.pop()
            elif cmd==0xa0:
                # Randomised argument: the next command takes a value between two bounds.
                sub=data[pos];pos+=1;low,high=struct.unpack_from('<hh',data,pos);pos+=4
                if sub==0x81:random['program']=(low,high);program=low
                elif sub==0xc3:random['transpose']=(low,high);transpose=low
                elif sub==0xc4:random['bend']=(low,high);bend=low
                elif sub in S16_COMMANDS or sub in U8_COMMANDS:pass
                else:raise ValueError(f'Unsupported random command {sub:#x}')
            elif cmd==0xa1:pos+=2  # argument read from a sequence variable; keep the default
            elif cmd==0xc3:transpose=struct.unpack_from('b',data,pos)[0];pos+=1
            elif cmd==0xc4:bend=struct.unpack_from('b',data,pos)[0];pos+=1
            elif cmd==0xc7:note_wait=bool(data[pos]);pos+=1
            elif cmd in U8_COMMANDS:pos+=1
            elif cmd in S16_COMMANDS:pos+=2
            elif cmd==0xfe:pos+=2
            elif cmd==0xf0 and 0x80<=data[pos]<=0x9f:pos+=4  # variable arithmetic
            elif cmd==0xff:return
            else:raise ValueError(f'Unsupported sequence command {cmd:#x}')
        raise ValueError('Sequence did not end')
    track(origin+start,0)
    return sorted(notes,key=lambda x:x['tick'])

def bank_region(bank,program,key,velocity):
    """(wave archive item, wave index, root key, volume, pitch) for one note."""
    n=NW(bank);info=n.blocks[b'INFO']+8
    waves=n.ref(info,info);instruments=n.ref(info+8,info)
    if program>=n.u32(instruments):raise ValueError('Program outside bank')
    entry=instruments+4+program*8;instrument=n.ref(entry,instruments)
    def pick(ref_off,owner,value):
        kind=n.kind(ref_off);chunk=n.ref(ref_off,owner)
        if kind==0x6000:return n.ref(chunk,chunk)
        if kind==0x6001:
            count=n.u32(chunk);keys=[n.u8(chunk+4+i) for i in range(count)]
            refs=chunk+4+(count+3)//4*4
            for i,top in enumerate(keys):
                if value<=top:return n.ref(refs+8*i,chunk)
            raise ValueError('Note outside bank ranges')
        if kind==0x6002:
            low,high=n.u8(chunk),n.u8(chunk+1)
            return n.ref(chunk+4+8*(min(max(value,low),high)-low),chunk)
        raise ValueError(f'Unknown bank region {kind:#x}')
    region=pick(instrument,instrument,key)
    velocity_region=pick(region,region,velocity)
    wave_index=n.u32(velocity_region);flags=n.u32(velocity_region+4);opt=velocity_region+8
    root,volume,pitch=60,127,1.
    if flags&1:root=n.u8(opt);opt+=4
    if flags&2:volume=n.u8(opt);opt+=4
    if flags&4:opt+=4
    if flags&8:pitch=n.f32(opt)
    item=waves+4+wave_index*8
    return n.u32(item)&0xffffff,n.u32(item+4),root,volume/127,pitch

def read_wav(path):
    with wave.open(str(path),'rb') as w:
        channels=w.getnchannels();rate=w.getframerate();frames=w.readframes(w.getnframes())
    samples=array.array('h');samples.frombytes(frames)
    mono=[sum(samples[i:i+channels])/channels for i in range(0,len(samples),channels)]
    return mono,rate

def render_sequence(layers,dst):
    """Mix decoded note layers: (samples, rate, start seconds, speed, gain)."""
    rate=max(r for _,r,_,_,_ in layers)
    length=max(int(start*rate+len(s)/speed*rate/r)+1 for s,r,start,speed,_ in layers)
    out=[0.]*length
    for samples,source_rate,start,speed,gain in layers:
        step=speed*source_rate/rate;offset=int(start*rate);pos=0.
        while pos<len(samples)-1 and offset<length:
            i=int(pos);f=pos-i
            out[offset]+=(samples[i]*(1-f)+samples[i+1]*f)*gain
            pos+=step;offset+=1
    pcm=array.array('h',(max(-32768,min(32767,int(v))) for v in out))
    with wave.open(str(dst),'wb') as w:
        w.setnchannels(1);w.setsampwidth(2);w.setframerate(rate);w.writeframes(pcm.tobytes())

# Animations the controller plays (their sound triggers are installed).
CLIPS='''Wait RunStart Brake TurnPoint Turn Land LandStiffen LandJump3 LandTurn LandDownFall LandRolling
Jump JumpReverse Jump2 Jump3 JumpBack JumpTurn JumpBroad JumpBroad2 JumpBroad3 Fall
SpinJumpStart SpinJumpLoop SpinJumpDownFall SpinGroundL SpinGroundR
HipDropStart HipDrop HipDropLand JumpHipDrop RollingStart Rolling RollingEnd RollingJump RollingStandUp
HeadSlidingStart HeadSliding SquatStart SquatWait SquatWalk SquatEnd
SpinCapStart SpinCapStartLeft SpinCapStartRight SpinCapStart2Left SpinCapStart2Right SpinCapStart3Left SpinCapStart3Right
SpinCapStartDoubleUp SpinCapStartDoubleDown SpinCapStartDoubleLeft SpinCapStartDoubleRight
SpinCapAirStart SpinCapAirStartLeft SpinCapAirStartRight SpinCapAirStartDoubleUp SpinCapAirStartDoubleDown
SpinCapAirStartDoubleLeft SpinCapAirStartDoubleRight
JumpCapLeapFrog JumpCapCatchCommon JumpCapCatch JumpCapCatchAir WallJump WallKeep WallSlideL WallCatchStart WallCatch
WallCatchEnd WallCatchEndJump PoleCatch PoleClimb PoleFall PoleWait SwimFallIn SwimWait SwimWalk SwimDive'''.split()
# Event tables keyed by the original's reaction names.
MARIO_REACTIONS={'着地':'Land','着地[走り]':'LandRun','転がり再加速[1回目]':'RollBoost',
    '転がり再加速[2回目以降]':'RollBoostAgain','転がり着地':'RollLand'}
CAP_ACTIONS=['FlyingWait','FlyingWaitSpiral','FlyingWaitRolling','Trample','SpinCapStart3Left','SpinCapStart3Right']
CAP_REACTIONS={'帽子投げ':'Throw','帽子戻し':'Return','帽子キャッチ':'Catch'}
# Footstep playback the controller triggers directly on its foot-print frames.
FOOT=['RunL','RunR','SquatRunL','SquatRunR','FootBrake','SwimRunL','SwimRunR']
# Footwear sets (the original chooses these in code; this mapping is inferred).
FOOTWEAR={'MarioAloha':'Sandal','MarioSwimwear':'Bare','MarioUnderwear':'Bare','MarioZombie':'Zombie','Mario64Metal':'Metal'}

def self_check():
    outfits.self_check()
    def container(blocks,magic=b'FWAR'):
        offset=20+12*len(blocks);entries=b'';body=b''
        for name,data in blocks:
            block=name+struct.pack('<I',8+len(data))+data
            entries+=struct.pack('<HHII',0,0,offset,len(block));body+=block;offset+=len(block)
        return magic+b'\xff\xfe'+struct.pack('<HIIHH',20,0,offset,len(blocks),0)+entries+body
    reference=lambda offset,kind=0:struct.pack('<HHI',kind,0,offset)
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
    # A footstep-like sequence: program 0, random transpose, one note, end.
    data=bytes([0x81,0,0xa0,0xc3,0,0,3,0,0x3c,0x7f,0,0x80,5,0x81,1,0x3f,0x64,0,0xff])
    notes=sequence_notes(container([(b'DATA',data)],b'FSEQ'),0)
    assert [(x['tick'],x['program'],x['key']) for x in notes]==[(0,0,60),(5,1,63)]
    assert notes[0]['random']=={'transpose':(0,3)}
    # A bank with one direct instrument region pointing at wave 0, root key 62.
    velocity=struct.pack('<II',0,3)+struct.pack('<BxxxBxxx',62,100)
    key_region=reference(8,0x6000)+reference(8,0x5902)+velocity
    instrument=reference(8,0x6000)+reference(8,0x5901)+key_region
    waves=struct.pack('<III',1,0x05000004,7)
    instruments=struct.pack('<I',1)+reference(12,0x5900)
    body=reference(16+len(instruments)+len(instrument),0x0100)+reference(16,0x0101)+instruments+instrument+waves
    assert bank_region(container([(b'INFO',body)],b'FBNK'),0,60,127)==(4,7,62,100/127,1.)
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
        print('\n'.join(name for name in sounds if name.startswith(('WsP','SeP','RcSeP'))));return
    files={}
    for archive in sound.glob('SeResource*.aras'):
        for name,data in extract.sarc(archive.read_bytes()):
            if data[:4]==b'FGRP':files.update(group_files(data))
    db=outfits.archive(sound/'SeDataBase.szs')
    load=lambda name:outfits.byml(db[name]) if name in db else {}
    def plays(*names):
        found={}
        for name in names:
            for item in load(name).get('PlayInfoList',[]):found.setdefault(item['Name'],item)
        return found
    mario=load('Mario.byml');cap=load('PossessCap.byml')
    mario_plays=plays('Mario.byml','MarioFoot.byml','MarioVoice.byml','MarioShakeNoise.byml','PlayerHakoniwaShakeNoise.byml')
    cap_plays=plays('PossessCap.byml','PossessCapVoice.byml')
    def actions(table,wanted):
        return {item['Name']:[{'play':x['Name'],'frame':x.get('StartFrame',0.),'once':x.get('IsOneTime',False)}
            for x in item.get('PlayInfoInActionList',[])] for item in table.get('ActionInfoList',[]) if item['Name'] in wanted}
    def reactions(table,wanted):
        return {wanted[item['Name']]:[x['Name'] for x in item.get('PlayInfoList',[])]
            for item in table.get('HitReactionInfoList',[]) if item['Name'] in wanted}
    out={'mario':{'actions':actions(mario,CLIPS),'reactions':reactions(mario,MARIO_REACTIONS),'plays':{}},
        'cap':{'actions':actions(cap,CAP_ACTIONS),'reactions':reactions(cap,CAP_REACTIONS),'plays':{}},
        'footwear':FOOTWEAR,'resources':{}}
    wanted_foot=FOOT+[prefix+n for prefix in set(FOOTWEAR.values()) for n in ('RunL','RunR','Jump','Land','FootBrake')]
    wanted_foot+=['RunMetalL','RunMetalR','JumpMetalHeavy','LandMetalHeavy']
    for group,table in (('mario',mario_plays),('cap',cap_plays)):
        g=out[group];names={x['play'] for v in g['actions'].values() for x in v}|{x for v in g['reactions'].values() for x in v}
        if group=='mario':names|=set(wanted_foot)
        for name in sorted(names):
            item=table.get(name)
            if not item:continue
            layers=[]
            for r in item.get('ResourceInfoList',[]):
                if r.get('Name') not in sounds:continue
                layer={'resource':r['Name']}
                for key,field in (('DirectVolume','volume'),('DirectPitch','pitch'),('RandPitch','random_pitch'),
                        ('RandVolume','random_volume'),('DelayFrame','delay')):
                    if key in r:layer[field]=r[key]
                for key in ('Volume','Pitch'):
                    curve=r.get(key)
                    if curve and not curve.get('RtpcSource'):
                        layer[key.lower()+'_curve']=[curve['InMin'],curve['InMax'],curve['OutMin'],curve['OutMax']]
                layers.append(layer)
            if layers:
                g['plays'][name]={'random':item.get('IsModeRandom',False),'loop':item.get('IsLevel',False),'layers':layers}
    temp=extract.ROOT/'local/audio';temp.mkdir(parents=True,exist_ok=True)
    output=extract.ROOT/'assets/local/sounds';output.mkdir(parents=True,exist_ok=True)
    for old in output.glob('*.wav'):old.unlink()
    def decode(raw,stem):
        src=temp/(stem+'.bfwav');src.write_bytes(raw);dst=temp/(stem+'.wav')
        result=subprocess.run([str(a.vgmstream.resolve()),'-i','-o',str(dst),str(src)],capture_output=True)
        if result.returncode:raise ValueError(f'vgmstream could not decode {stem}')
        return dst
    needed=sorted({l['resource'] for g in ('mario','cap') for p in out[g]['plays'].values() for l in p['layers']})
    skipped=[]
    for name in needed:
        info=sounds[name];entry={'files':[],'volume':round(info['volume'],4)}
        if info['kind']=='wave':
            dst=output/(name+'.wav')
            dst.write_bytes(decode(wave_for(files,wave_files,info['file'],info['index']),name).read_bytes())
            entry['files'].append('local/sounds/'+dst.name)
        else:
            bank=files[info['banks'][0]]
            try:notes=sequence_notes(files[info['file']],info['start'])
            except ValueError as e:
                skipped.append(f'{name} ({e})');continue
            # A randomised instrument becomes one rendered variant per program.
            programs=sorted({p for x in notes for p in range(x['random'].get('program',(0,-1))[0],x['random'].get('program',(0,-1))[1]+1)})[:8] or [None]
            for variant,chosen in enumerate(programs):
                layers=[]
                for i,note in enumerate(notes):
                    program=chosen if chosen is not None and 'program' in note['random'] else note['program']
                    archive,index,root,volume,pitch=bank_region(bank,program,note['key'],note['velocity'])
                    samples,rate=read_wav(decode(archived_wave(files,wave_files,archive,index),f'{name}_{variant}_{i}'))
                    semitones=note['key']-root+note['bend']/64
                    layers.append((samples,rate,note['tick']/96,pitch*2**(semitones/12),volume*(note['velocity']/127)**2))
                    spread=note['random']
                    if 'transpose' in spread or 'bend' in spread:
                        low=spread.get('transpose',(0,0))[0]+spread.get('bend',(0,0))[0]/64
                        high=spread.get('transpose',(0,0))[1]+spread.get('bend',(0,0))[1]/64
                        entry['random_semitones']=[0,round(high-low,3)]
                dst=output/(f'{name}_{variant}.wav' if len(programs)>1 else name+'.wav')
                render_sequence(layers,dst)
                entry['files'].append('local/sounds/'+dst.name)
        for f in entry['files']:
            with wave.open(str(extract.ROOT/'assets'/f),'rb') as audio:
                if audio.getnframes()==0:raise ValueError('Decoded audio is empty')
        out['resources'][name]=entry
    (output/'player.json').write_text(json.dumps(out,indent=1,ensure_ascii=False),encoding='utf-8')
    (output/'manifest.json').unlink(missing_ok=True)
    if skipped:print('Skipped sequences this tool cannot render: '+', '.join(skipped))
    print(f"Installed {len(needed)} sounds for {len(out['mario']['actions'])} animations, "
        f"{len(out['mario']['reactions'])+len(out['cap']['reactions'])} events and the cap.")

if __name__=='__main__':
    try:main()
    except (OSError,ValueError,KeyError,IndexError,struct.error) as e:raise SystemExit(f'Sound extraction stopped: {e}')
