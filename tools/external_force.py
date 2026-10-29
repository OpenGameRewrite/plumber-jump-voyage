"""Independent, placement-bound verification of the captured waterfall source.

Observed force/matrix fields validate the implementation; no field drives replay.
"""
import math
import struct
from key_move_parts import f32


def vector(cell, separator=','):
    tokens = cell.split(separator)
    if len(tokens) != 3 or any(len(t.removeprefix('0x')) != 8 for t in tokens):
        raise ValueError('force vector bits')
    result = [struct.unpack('>f', bytes.fromhex(t.removeprefix('0x')))[0] for t in tokens]
    if not all(math.isfinite(x) for x in result):
        raise ValueError('nonfinite force')
    return result


def add(a, b): return [f32(x+y) for x,y in zip(a,b)]
def sub(a, b): return [f32(x-y) for x,y in zip(a,b)]
def scale(a, n): return [f32(x*n) for x in a]
def dot(a, b): return f32(f32(f32(a[0]*b[0])+f32(a[1]*b[1]))+f32(a[2]*b[2]))
def length(a): return f32(math.sqrt(dot(a,a)))


def contains(area, p):
    m, s = [f32(x) for x in area['matrix']], [f32(x) for x in area['scale']]
    if any(abs(x) <= f32(.001) for x in s): return False
    d = sub(p, [m[3],m[7],m[11]])
    q = [f32(dot(d, [m[i],m[4+i],m[8+i]])/s[i]) for i in range(3)]
    return 0 <= q[1] <= 500 and f32(f32(q[0]*q[0])+f32(q[2]*q[2])) <= 250000


def force(area, p, prior, velocity):
    if not contains(area,p): return [0.,0.,0.]
    m = area['matrix']; up = [f32(m[i]) for i in (1,5,9)]
    vv = scale(up,dot(up,velocity)); vh = sub(velocity,vv)
    pv = scale(up,dot(up,prior)); ph = sub(prior,pv)
    speed, before = dot(up,vv), dot(up,pv)
    if speed < -40: change = -before
    else:
        cur = f32(speed+before)
        nxt = min(f32(cur+2),-40.) if cur < -40 else max(f32(cur-2),-40.)
        change = f32(nxt-cur)
    if dot(vh,vh) >= f32(.000001):
        direction = scale(vh,f32(1/length(vh)))
        mag = f32(max(f32(length(vh)-length(ph)),0)*f32(.98))
        h = sub(scale(direction,mag),add(ph,vh))
    else: h = scale(ph,-1)
    return add(scale(up,change),h)


class Evidence:
    def __init__(self, geometry, calls, mario, rewrite, rows, preroll):
        self.frames = set()
        try: self._verify(geometry,calls,mario,rewrite,rows,preroll)
        except (KeyError,TypeError,ValueError,IndexError,StopIteration,OverflowError,ZeroDivisionError):
            self.frames = set()

    def _verify(self, geometry, calls, mario, rewrite, rows, preroll):
        if not geometry or (geometry.get('stage'),geometry.get('scenario')) != ('WaterfallWorldHomeStage',1) or not calls or not mario:
            return
        from environment_rule import key_values
        area = next(a for a in geometry['areas'] if a['id']=='obj2386' and a['name']=='ExtForceArea')
        if (area['shape']!='AreaCylinder' or area['translate']!=[2950,550,1300]
                or area['scale']!=[1.5,3,2.5] or area['rotate']!=[0,-45,0]
                or area['links'] or area['args']!={'ForceType':'滝'}): return
        samples = {int(r['frame']):r for r in calls if r['kind']=='sample'}
        by_frame = {}
        for r in calls:
            if r['kind']=='call': by_frame.setdefault(int(r['frame']),[]).append(r)
        side = {int(r['frame']):r for r in mario}
        all_rows = dict(preroll); all_rows.update(rows)
        if set(samples) != set(all_rows) or set(side)!=set(all_rows): return
        previous = [0.,0.,0.]; pointer = set(); active = set()
        for f,r in sorted(all_rows.items()):
            sample = samples[f]
            if sample['dropped']!='0' or sample['registry_dropped']!='0': return
            observed = by_frame.get(f,[])
            if int(sample['calls']) != len(observed) or len(observed)>1: return
            prior = scale(previous, f32(.95 if all_rows.get(f-1,{}).get('ground')=='1' else .955))
            old = all_rows.get(f-1)
            if old:
                p = vector(old['bits_pos'],'/'); v = vector(old['bits_vel'],'/')
                inside = contains(area,p)
                if bool(observed) != inside: return
            else:
                if observed: return
                inside = False
            output = [0.,0.,0.]
            if inside:
                c = observed[0]
                if (c['id']!='obj2386' or c['zone'] or c['seq']!='0' or not c['area']
                        or vector(c['pos_bits'])!=p or vector(c['velocity_bits'])!=v
                        or vector(c['previous_bits'])!=prior): return
                output = force(area,p,prior,v)
                if vector(c['force_bits']) != output: return
                pointer.add(c['area'])
            previous = add(prior,output)
            if length(previous)<f32(.1): previous=[0.,0.,0.]
            m = side[f]
            if (vector(m['area_velocity_bits'])!=previous or vector(m['total_velocity_bits'])!=previous
                    or any(vector(m[k+'_bits']) != [0.,0.,0.] for k in ('external_velocity','last_ground_inertia'))): return
            if f >= 0:
                d = key_values(rewrite[f]['collision'])
                pos = vector(d['areapos'],'/'); inp = vector(d['areainput'],'/'); prev = vector(d['areaprior'],'/')
                native = force(area,pos,prev,inp)
                # Native inputs belong to its own free trajectory. It must
                # evaluate this same exported source even when movement fails.
                if (d['forcearea'] != ('obj2386' if contains(area,pos) else '')
                        or vector(d['areaforce'],'/')!=native): return
                result = add(prev,native)
                if length(result)<f32(.1): result=[0.,0.,0.]
                if vector(d['areavelocity'],'/')!=result: return
                if inside or previous!=[0.,0.,0.]: active.add(f)
        if len(pointer)==1 and active: self.frames=active
