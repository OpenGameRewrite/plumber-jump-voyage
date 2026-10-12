"""Pack a local glTF 2.0 export into one self-contained GLB, including textures."""
import base64
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
import asset_contract
from urllib.parse import unquote

COMPONENTS={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4,'MAT4':16}

def read_floats(model,data,index):
    accessor=model['accessors'][index];view=model['bufferViews'][accessor['bufferView']]
    if accessor['componentType']!=5126:raise ValueError('Animation data must be float')
    n=COMPONENTS[accessor['type']];stride=view.get('byteStride',4*n)
    begin=view.get('byteOffset',0)+accessor.get('byteOffset',0)
    return [struct.unpack_from('<%df'%n,data,begin+k*stride) for k in range(accessor['count'])]

def slerp(a,b,t):
    dot=sum(x*y for x,y in zip(a,b))
    if dot<0:b=[-x for x in b];dot=-dot
    if dot>0.9995:
        q=[x+(y-x)*t for x,y in zip(a,b)]
    else:
        theta=math.acos(min(1.,dot));sa=math.sin(theta)
        q=[(math.sin((1-t)*theta)*x+math.sin(t*theta)*y)/sa for x,y in zip(a,b)]
    norm=math.sqrt(sum(x*x for x in q)) or 1.
    return [x/norm for x in q]

def reduce_keys(times,values,rotation):
    """Drop keys the remaining ones reproduce (to well below what can be seen)."""
    tolerance=1e-4 if rotation else 1e-3
    def close(a,b):
        if rotation:return abs(sum(x*y for x,y in zip(a,b)))>=1-tolerance*tolerance/2
        return max(abs(x-y) for x,y in zip(a,b))<=tolerance
    if all(close(v,values[0]) for v in values):return [times[0]],[values[0]]
    kept=[0];anchor=0
    for i in range(1,len(values)-1):
        # Can the segment anchor..i+1 stand in for every key between them?
        ok=True
        for j in range(anchor+1,i+1):
            t=(times[j]-times[anchor])/((times[i+1]-times[anchor]) or 1)
            guess=slerp(values[anchor],values[i+1],t) if rotation else [x+(y-x)*t for x,y in zip(values[anchor],values[i+1])]
            if not close(guess,values[j]):ok=False;break
        if not ok:kept.append(i);anchor=i
    kept.append(len(values)-1)
    return [times[k] for k in kept],[values[k] for k in kept]

def compact(model,data):
    """Rebuild the binary with reduced, shared animation keys; other data is copied."""
    out=bytearray();views=[];accessors=[]
    def append(raw,target=None):
        while len(out)%4:out.append(0)
        start=len(out);out.extend(raw)
        views.append({'buffer':0,'byteOffset':start,'byteLength':len(raw)})
        if target:views[-1]['target']=target
        return len(views)-1
    animated={i for a in model.get('animations',[]) for sm in a['samplers'] for i in (sm['input'],sm['output'])}
    view_map={};accessor_map={}
    for i,accessor in enumerate(model.get('accessors',[])):
        if i in animated:continue
        old=accessor['bufferView']
        if old not in view_map:
            v=model['bufferViews'][old];begin=v.get('byteOffset',0)
            view_map[old]=append(bytes(data[begin:begin+v['byteLength']]),v.get('target'))
            if 'byteStride' in v:views[view_map[old]]['byteStride']=v['byteStride']
        accessor=dict(accessor);accessor['bufferView']=view_map[old]
        accessor_map[i]=len(accessors);accessors.append(accessor)
    for image in model.get('images',[]):
        if 'bufferView' in image:
            v=model['bufferViews'][image['bufferView']];begin=v.get('byteOffset',0)
            image['bufferView']=append(bytes(data[begin:begin+v['byteLength']]))
    def floats(values,kind):
        flat=[x for v in values for x in v];index=append(struct.pack('<%df'%len(flat),*flat))
        accessor={'bufferView':index,'componentType':5126,'count':len(values),'type':kind}
        if kind=='SCALAR':accessor.update(min=[min(flat)],max=[max(flat)])
        accessors.append(accessor);return len(accessors)-1
    shared_inputs={};before=after=0
    for animation in model.get('animations',[]):
        paths={c['sampler']:c['target']['path'] for c in animation['channels']}
        for k,sampler in enumerate(animation['samplers']):
            times=[t[0] for t in read_floats(model,data,sampler['input'])]
            values=read_floats(model,data,sampler['output'])
            if sampler.get('interpolation','LINEAR')!='LINEAR' or len(times)!=len(values):
                raise ValueError('Expected linear baked animation keys')
            before+=len(times)
            times,values=reduce_keys(times,values,paths.get(k)=='rotation')
            after+=len(times)
            key=tuple(round(t,6) for t in times)
            if key not in shared_inputs:shared_inputs[key]=floats([(t,) for t in times],'SCALAR')
            sampler['input']=shared_inputs[key]
            sampler['output']=floats(values,model['accessors'][sampler['output']]['type'])
    for mesh in model.get('meshes',[]):
        for primitive in mesh['primitives']:
            primitive['attributes']={k:accessor_map[v] for k,v in primitive['attributes'].items()}
            if 'indices' in primitive:primitive['indices']=accessor_map[primitive['indices']]
            for target in primitive.get('targets',[]):
                for k in target:target[k]=accessor_map[target[k]]
    for skin in model.get('skins',[]):
        if 'inverseBindMatrices' in skin:skin['inverseBindMatrices']=accessor_map[skin['inverseBindMatrices']]
    model['accessors']=accessors;model['bufferViews']=views
    if before:print(f'Animation keys: {before} -> {after}')
    return out

def pack(path,animations_from=None):
    model=json.loads(path.read_text(encoding='utf-8'))
    if model.get('asset',{}).get('version')!='2.0':raise ValueError('Expected glTF 2.0')
    clips=path.with_suffix('.clips.txt')
    animations=model.get('animations',[])
    expressions=path.with_suffix('.expressions.json')
    if expressions.is_file():
        model.setdefault('extras',{})['expressions']=json.loads(expressions.read_text(encoding='utf-8-sig'))
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
    # A stable name for the scene root keeps animation target paths equal across
    # outfits (an unnamed root is named after its index), so clips can be shared.
    for root in model.get('scenes',[{}])[model.get('scene',0)].get('nodes',[]):
        model['nodes'][root].setdefault('name','Root')
    if animations_from:
        source=Path(animations_from).read_bytes();json_size=struct.unpack_from('<I',source,12)[0]
        shared=json.loads(source[20:20+json_size])
        model.setdefault('extras',{})['expressions']=shared.get('extras',{}).get('expressions',{})
        model['extras']['animation_source']=Path(animations_from).name
        # The game plays the shared file's clips by bone path. One constant key
        # still marks this skeleton as animated so the loader sets up its targets.
        nodes={n.get('name'):i for i,n in enumerate(model['nodes']) if n.get('name')}
        names=[n.get('name') for n in shared['nodes'] if n.get('name') in nodes]
        bone=next((nodes[n] for n in names if n.startswith('Armature_')),nodes[names[0]] if names else None)
        if bone is None:raise ValueError('No shared skeleton bone')
        rest=model['nodes'][bone].get('translation',[0.,0.,0.])
        raw=struct.pack('<4f',0.,*rest);offset=append(raw);views=len(model['bufferViews'])
        model['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(raw)})
        model['accessors']+= [{'bufferView':views,'componentType':5126,'count':1,'type':'SCALAR','min':[0.],'max':[0.]},
            {'bufferView':views,'byteOffset':4,'componentType':5126,'count':1,'type':'VEC3'}]
        n=len(model['accessors'])
        model['animations']=[{'name':'Rest','samplers':[{'input':n-2,'output':n-1}],
            'channels':[{'sampler':0,'target':{'node':bone,'path':'translation'}}]}]
    asset_contract.annotate(model, metal='Metal' in path.stem)
    if model.get('animations') and not animations_from:data=compact(model,data)
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
            'buffers':[{'uri':'mesh.bin','byteLength':16},{'uri':'anim.bin','byteLength':44}],
            'bufferViews':[{'buffer':0,'byteLength':16},{'buffer':1,'byteLength':8},{'buffer':1,'byteOffset':8,'byteLength':36}],
            'accessors':[{'bufferView':0,'componentType':5126,'count':1,'type':'VEC4'},
                {'bufferView':1,'componentType':5126,'count':2,'type':'SCALAR'},{'bufferView':2,'componentType':5126,'count':3,'type':'VEC3'}],
            'animations':[{'name':'jump','samplers':[{'input':1,'output':2}], 'channels':[{'sampler':0,'target':{'node':0,'path':'translation'}}]}]}
        (path.parent/'mesh.bin').write_bytes(struct.pack('<4f',0,1,2,3))
        # Two keys at 0 and 1 (third value unused by the count check below).
        model['accessors'][2]['count']=2
        (path.parent/'anim.bin').write_bytes(struct.pack('<2f',0,1)+struct.pack('<9f',5,5,5,5,5,5,0,0,0))
        model['animations']*=2
        expressions={'jump':{'looping':False,'tracks':{'EyelidLeft':[True,False]},'uv':{'EyePupilLeftMT':[[0,0,0],[0,0.1,0.2]]}}}
        path.with_suffix('.expressions.json').write_text(json.dumps(expressions),encoding='utf-8')
        path.with_suffix('.clips.txt').write_text('jump\t2\n',encoding='utf-8')
        path.write_text(json.dumps(model),encoding='utf-8');output=pack(path)
        raw=output.read_bytes();size=struct.unpack_from('<I',raw,12)[0];packed=json.loads(raw[20:20+size])
        assert packed['accessors'][0]['componentType']==5123
        assert packed['extras']['expressions']==expressions
        assert len(packed['animations'])==1 and len(packed['animations'][0]['channels'])==2
        assert packed['animations'][0]['channels'][1]['sampler']==1
        joints=packed['bufferViews'][packed['accessors'][0]['bufferView']]
        assert struct.unpack_from('<4H',raw,28+size+joints['byteOffset'])==(0,1,2,3)
        # A constant track keeps one key, and identical key times are shared.
        sampler=packed['animations'][0]['samplers'][0]
        assert packed['accessors'][sampler['input']]['count']==1
        assert sampler['input']==packed['animations'][0]['samplers'][1]['input']
        assert packed['nodes'][0]['name']=='rig'
        assert reduce_keys([0,1,2],[(0.,),(1.,),(2.,)],False)==([0,2],[(0.,),(2.,)])
        assert len(reduce_keys([0,1,2],[(0.,),(1.,),(0.,)],False)[0])==3
        model['nodes'].insert(0,{'name':'other'});model.pop('animations')
        path=path.with_name('costume.gltf');path.write_text(json.dumps(model),encoding='utf-8')
        raw=pack(path,output).read_bytes();size=struct.unpack_from('<I',raw,12)[0];sharing=json.loads(raw[20:20+size])
        assert sharing['extras']['animation_source']==output.name and [a['name'] for a in sharing['animations']]==['Rest']
        assert sharing['extras']['expressions']==expressions
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
