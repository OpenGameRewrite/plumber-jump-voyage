"""Pack a local glTF 2.0 export into one self-contained GLB, including textures."""
import base64
import json
from pathlib import Path
import struct
import sys
import tempfile
from urllib.parse import unquote

def pack(path,animations_from=None):
    model=json.loads(path.read_text(encoding='utf-8'))
    if model.get('asset',{}).get('version')!='2.0':raise ValueError('Expected glTF 2.0')
    clips=path.with_suffix('.clips.txt')
    animations=model.get('animations',[])
    if clips.is_file() and animations:
        groups=[line.rsplit('\t',1) for line in clips.read_text().splitlines()]
        if len(animations)==sum(int(count) for _,count in groups) and len(animations)!=len(groups):
            joined=[];start=0
            for name,count in groups:
                clip={'name':name,'samplers':[],'channels':[]}
                for track in animations[start:start+int(count)]:
                    offset=len(clip['samplers']);clip['samplers'].extend(track['samplers'])
                    for channel in track['channels']:
                        channel['sampler']+=offset;clip['channels'].append(channel)
                joined.append(clip);start+=int(count)
            model['animations']=joined
        elif len(animations)!=len(groups):raise ValueError('Animation track count does not match converter metadata')
    data=bytearray();offsets=[]
    def append(raw):
        while len(data)%4:data.append(0)
        start=len(data);data.extend(raw);return start
    def read(uri):
        if uri.startswith('data:'):return base64.b64decode(uri.split(',',1)[1],validate=True)
        p=(path.parent/unquote(uri)).resolve()
        if not p.is_relative_to(path.parent.resolve()):raise ValueError('External path escapes export folder')
        return p.read_bytes()
    for buffer in model.get('buffers',[]):offsets.append(append(read(buffer['uri'])))
    for view in model.get('bufferViews',[]):view['byteOffset']=view.get('byteOffset',0)+offsets[view.get('buffer',0)];view['buffer']=0
    # Old Assimp writes floating point joint indices, which glTF forbids.
    joints={primitive['attributes']['JOINTS_0'] for mesh in model.get('meshes',[]) for primitive in mesh['primitives'] if 'JOINTS_0' in primitive['attributes']}
    for index in joints:
        accessor=model['accessors'][index]
        if accessor['componentType']!=5126:continue
        view=model['bufferViews'][accessor['bufferView']]
        begin=view.get('byteOffset',0)+accessor.get('byteOffset',0);stride=view.get('byteStride',16)
        values=[v for i in range(accessor['count']) for v in struct.unpack_from('<4f',data,begin+i*stride)]
        if any(v!=int(v) or not 0<=v<65536 for v in values):raise ValueError('Invalid skin joint index')
        raw=struct.pack('<'+'H'*len(values),*(int(v) for v in values));offset=append(raw)
        accessor.update(bufferView=len(model['bufferViews']),byteOffset=0,componentType=5123)
        accessor.pop('normalized',None)
        model['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(raw),'target':34962})
    for image in model.get('images',[]):
        if 'uri' not in image:continue
        uri=image.pop('uri')
        # Assimp sometimes retains a source JPG name after Toolbox writes PNG.
        if not uri.startswith('data:') and not (path.parent/unquote(uri)).is_file():
            png=Path(unquote(uri)).with_suffix('.png')
            if (path.parent/png).is_file():uri=png.as_posix()
        raw=read(uri);offset=append(raw)
        image['bufferView']=len(model.setdefault('bufferViews',[]));image['mimeType']='image/png' if raw.startswith(b'\x89PNG') else 'image/jpeg'
        model['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(raw)})
    if animations_from:
        source=Path(animations_from).read_bytes();json_size=struct.unpack_from('<I',source,12)[0]
        shared=json.loads(source[20:20+json_size]);binary=source[28+json_size:]
        # ponytail: copy the whole shared buffer; compact it if pack size matters.
        offset=append(binary);views=len(model['bufferViews']);accessors=len(model['accessors'])
        for view in shared['bufferViews']:
            view['byteOffset']=view.get('byteOffset',0)+offset;view['buffer']=0
        model['bufferViews'].extend(shared['bufferViews'])
        for accessor in shared['accessors']:accessor['bufferView']+=views
        model['accessors'].extend(shared['accessors'])
        nodes={n.get('name'):i for i,n in enumerate(model['nodes']) if n.get('name')}
        copied=[]
        for animation in shared.get('animations',[]):
            channels=[]
            for channel in animation['channels']:
                name=shared['nodes'][channel['target']['node']].get('name')
                if name in nodes:
                    channel['target']['node']=nodes[name];channels.append(channel)
            if not channels:continue
            animation['channels']=channels
            for sampler in animation['samplers']:
                sampler['input']+=accessors;sampler['output']+=accessors
            copied.append(animation)
        model['animations']=copied
    model['buffers']=[{'byteLength':len(data)}]
    encoded=json.dumps(model,separators=(',',':')).encode();encoded+=b' '*(-len(encoded)%4);data.extend(b'\0'*(-len(data)%4))
    output=path.with_suffix('.glb');output.write_bytes(struct.pack('<4sII',b'glTF',2,28+len(encoded)+len(data))+struct.pack('<II',len(encoded),0x4e4f534a)+encoded+struct.pack('<II',len(data),0x004e4942)+data)
    print(f'{output.name}: {len(model.get("meshes",[]))} meshes, {len(model.get("animations",[]))} animations, {len(model.get("images",[]))} embedded images')
    return output
def self_check():
    with tempfile.TemporaryDirectory() as folder:
        path=Path(folder)/'sample.gltf'
        model={'asset':{'version':'2.0'},'scenes':[{'nodes':[0]}],'nodes':[{'name':'rig'}],
            'meshes':[{'primitives':[{'attributes':{'JOINTS_0':0}}]}],
            'buffers':[{'uri':'mesh.bin','byteLength':16}], 'bufferViews':[{'buffer':0,'byteLength':16}],
            'accessors':[{'bufferView':0,'componentType':5126,'count':1,'type':'VEC4'}],
            'animations':[{'name':'jump','samplers':[{'input':0,'output':0}], 'channels':[{'sampler':0,'target':{'node':0,'path':'translation'}}]}]}
        (path.parent/'mesh.bin').write_bytes(struct.pack('<4f',0,1,2,3))
        model['animations']*=2
        path.with_suffix('.clips.txt').write_text('jump\t2\n',encoding='utf-8')
        path.write_text(json.dumps(model),encoding='utf-8');output=pack(path)
        raw=output.read_bytes();size=struct.unpack_from('<I',raw,12)[0];packed=json.loads(raw[20:20+size])
        assert packed['accessors'][0]['componentType']==5123
        assert len(packed['animations'])==1 and len(packed['animations'][0]['channels'])==2
        assert packed['animations'][0]['channels'][1]['sampler']==1
        assert struct.unpack_from('<4H',raw,28+size+packed['bufferViews'][1]['byteOffset'])==(0,1,2,3)
        model['nodes'].insert(0,{'name':'other'});model.pop('animations')
        path=path.with_name('costume.gltf');path.write_text(json.dumps(model),encoding='utf-8')
        raw=pack(path,output).read_bytes();size=struct.unpack_from('<I',raw,12)[0];copied=json.loads(raw[20:20+size])
        assert copied['animations'][0]['channels'][0]['target']['node']==1
        assert copied['animations'][0]['samplers'][0]['input']==len(model['accessors'])
        model['buffers'][0]['uri']='../escape.bin';path.write_text(json.dumps(model),encoding='utf-8')
        try:pack(path)
        except ValueError:pass
        else:raise AssertionError('External path escape accepted')
    print('GLB packing checks passed')

if __name__=='__main__':
    try:
        if '--self-check' in sys.argv:self_check()
        elif len(sys.argv)==2:pack(Path(sys.argv[1]))
        else:raise ValueError('Use pack-glb.py file.gltf or --self-check')
    except (OSError,ValueError,KeyError) as e:raise SystemExit(f'GLB packing stopped: {e}')
