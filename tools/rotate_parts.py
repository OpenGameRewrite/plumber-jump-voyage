"""Focused local-X rotor source and exact exported collision identity evidence."""
import math
import struct
from key_move_parts import f32, bits


def floats(cell, count, sep=','):
    values = [struct.unpack('>f',bytes.fromhex(x))[0] for x in cell.split(sep)]
    if len(values)!=count or not all(math.isfinite(v) for v in values): raise ValueError('rotor bits')
    return values


class Source:
    def __init__(self, placement, position):
        self.p, self.position = placement, position
        self.count, self.x, self.w = 0,0.,1.
        self.previous = self.matrix()

    def tick(self):
        self.previous = self.matrix()
        half = f32(f32(f32(self.p['speed']/100)*f32(.017453292))*f32(.5))
        axis = f32(f32(self.w*self.w)+f32(self.x*self.x))
        z = half*half; w4 = z*z
        sine = f32((half + (z*half)*(-.16666666641626524+z*.008333329385889463)) + (z*half)*w4*(-.00019839334836096632+z*2.718311493989822e-6))
        cosine = f32(((1+z*(-.499999997251031))+w4*.04166662332373906)+(w4*z)*(-.001388676377460993+z*2.439044879627741e-5))
        s,c = f32(sine*axis),cosine
        w = f32(f32(c*self.w)-f32(s*self.x))
        x = f32(f32(s*self.w)+f32(c*self.x))
        inv = f32(1/f32(math.sqrt(f32(f32(w*w)+f32(x*x)))))
        self.x,self.w = f32(inv*x),f32(inv*w)
        self.count += 1

    def matrix(self):
        xx = f32(self.x*f32(self.x+self.x))
        xw = f32(self.x*f32(self.w+self.w))
        x,y,z = self.position
        return [1.,0.,0.,x,0.,f32(1-xx),-xw,y,0.,xw,f32(1-xx),z]

    def diagnostic(self):
        return f"{self.p['id']}/Rotate/{self.count}/"+'/'.join(bits(v) for v in self.read_back())

    def read_back(self):
        m = self.matrix()
        square = f32(f32(f32(f32(m[0]+m[5])+m[10])+1)*f32(.25))
        w = f32(math.sqrt(square)); k=f32(.25/w)
        return [f32(f32(m[9]-m[6])*k),0.,0.,w]


class Evidence:
    def __init__(self, geometry, actors, messages, mario, rolling, rewrite, rows, preroll, start):
        self.surfaces,self.rides = set(),set()
        try: self._verify(geometry,actors,messages,mario,rolling,rewrite,rows,preroll,start)
        except (KeyError,TypeError,ValueError,IndexError,StopIteration,OverflowError,ZeroDivisionError,struct.error):
            self.surfaces,self.rides=set(),set()

    def _verify(self, geometry, actors, messages, mario, rolling, rewrite, rows, preroll, start):
        if not geometry or (geometry.get('stage'),geometry.get('scenario'))!=('ForestWorldHomeStage',1) or not actors or messages is None or not mario or not rolling or not start: return
        from environment_rule import key_values,parse_hit_hosts
        placements = geometry.get('rotate_parts',[])
        if len(placements)!=1: return
        p = placements[0]
        if p != dict(id='obj5515',model='ForestWorldHomeRotateParts000',axis=0,speed=50,host='森のワールドホーム回転パーツ000',**{'class':'RotateMapParts'}): return
        obj = next(o for o in geometry['objects'] if o['id']==p['id'])
        if (obj['model']!=p['model'] or obj['static'] is not False or obj['translate']!=[5950,3300,-13750]
                or obj['rotate']!=[0,0,0] or obj['scale']!=[1,1,1]): return
        triangles = {i:t for i,t in enumerate(geometry['triangles']) if geometry['objects'][t[13]]['id']==p['id']}
        if len(triangles)!=44 or not geometry['models'][p['model']+'#0']['prisms']: return
        raw = {t[17]:t for t in triangles.values() if t[16]==0}
        source = Source(p,obj['translate'])
        samples = {int(r['frame']):r for r in actors if r['id']==p['id']}
        all_rows=dict(preroll); all_rows.update(rows)
        c={int(r['frame']):r for r in mario}; contacts={int(r['frame']):r for r in rolling}
        if set(samples)!=set(all_rows) or set(c)!=set(all_rows) or set(contacts)!=set(all_rows) or min(samples)!=-30: return
        # PlayerTouch, PlayerFloorTouch, AskSafetyPoint, PlayerJumpTakeOffFloor.
        if any(r['id']!=p['id'] or r['answer']!='0' or r['message_vt'] not in ('1de9b18','1de93e8','1deb878','1d9dad8') for r in messages): return
        pointers=set(); surfaces=set(); rides=set(); generation=None
        for f,r in sorted(samples.items()):
            if (r['phase']!='Rotate' or r['n_vt']!='1df14a8' or r['axis']!='0' or r['speed_bits']!=bits(p['speed'])
                    or any(r[k]!='0' for k in ('dead','clipped','assist_timer','SwitchStart_valid','SwitchAppear_valid','registry_dropped','message_dropped'))
                    or r['collision_user_valid']!='1' or r['collision_system_valid']!='1' or r['zone']): return
            if generation is None: generation=r['generation']
            if r['generation']!=generation: return
            if (int(r['executions'])!=source.count or int(r['step'])!=(-1 if f==-30 else source.count)
                    or floats(r['quat_bits'],4)!=source.read_back()
                    or floats(r['matrix_bits'],12)!=source.matrix()
                    or floats(r['previous_matrix_bits'],12)!=source.previous
                    or floats(r['pos_bits'],3)!=obj['translate']
                    or floats(r['init_pos_bits'],3)!=obj['translate']
                    or floats(r['init_quat_bits'],4)!=[0.,0.,0.,1.]
                    or floats(r['reset_quat_bits'],4)!=[0.,0.,0.,1.]): return
            if f>=0 and key_values(rewrite[f]['collision']).get('rotors')!=source.diagnostic(): return
            pointers.add(r['part'])
            row=all_rows[f]; hit=contacts[f]; side=c[f]
            if any(side[k]!='0' for k in ('pressed','pressed_ground','pressed_wall','gate_true')): return
            for kind in ('ground','wall'):
                if not hit.get(kind+'_part'): continue
                if hit[kind+'_part']!=r['part'] or int(hit[kind+'_tri']) not in raw: return
                if kind=='ground':
                    if (side['ground_part']!=r['part'] or floats(side['ground_matrix_bits'],12)!=source.matrix()
                            or floats(side['ground_previous_matrix_bits'],12)!=source.previous): return
                host=parse_hit_hosts(row.get('hit_hosts')).get(kind)
                if f>=0:
                    base=tuple(round(f32(f32(x)-f32(s)),6) for x,s in zip(obj['translate'],start))
                    if not host or host['host']!=p['host'] or host['base']!=base or host['sensor']!='Collision' or host['sensor_type']!='15': return
                    surfaces.add((f,kind,host['host'],host['base']))
                    if kind=='ground': rides.add(f)
            source.tick()
        if len(pointers)==1: self.surfaces,self.rides=surfaces,rides

    def surface(self, f, kind, host):
        return (f,kind,host.get('host'),host.get('base')) in self.surfaces

    def riding(self, f): return f in self.rides
