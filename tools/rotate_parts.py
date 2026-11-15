"""Focused local-X/local-Y rotor source and exact exported collision identity evidence."""
import math
import struct
from key_move_parts import f32, bits


# The rotors whose exported collision and recorded phase this evidence can vouch for, bound by
# stage, scenario and placement id: every argument below is what the placement holds and what
# the rewrite's RotatePart reproduces (RotateMapParts.cpp: RotateAxis, RotateSpeed).
PLACEMENTS = {
    ('ForestWorldHomeStage', 1, 'obj5515'): dict(
        model='ForestWorldHomeRotateParts000', axis=0, speed=50, host='森のワールドホーム回転パーツ000',
        translate=[5950, 3300, -13750], triangles=44),
    ('LavaWorldClockExStage', 1, 'obj312'): dict(
        model='LavaWorldClockExRotateParts001', axis=1, speed=-80, host='溶岩ワールドクロックEx回転パーツ001',
        translate=[4850, 1050, -2650], triangles=160),
}


def floats(cell, count, sep=','):
    values = [struct.unpack('>f',bytes.fromhex(x))[0] for x in cell.split(sep)]
    if len(values)!=count or not all(math.isfinite(v) for v in values): raise ValueError('rotor bits')
    return values


class Source:
    """RotateMapParts' stored quaternion about its own axis (al::rotateQuatLocalDirDegree), identity start."""

    def __init__(self, placement, position, count=0):
        self.p, self.position, self.axis = placement, position, placement['axis']
        self.count, self.a, self.w = 0, 0., 1.
        self.previous = self.matrix()
        for _ in range(count): self.tick()

    def tick(self):
        self.previous = self.matrix()
        half = f32(f32(f32(self.p['speed']/100)*f32(.017453292))*f32(.5))
        axis = f32(f32(self.w*self.w)+f32(self.a*self.a))
        z = half*half; w4 = z*z
        sine = f32((half + (z*half)*(-.16666666641626524+z*.008333329385889463)) + (z*half)*w4*(-.00019839334836096632+z*2.718311493989822e-6))
        cosine = f32(((1+z*(-.499999997251031))+w4*.04166662332373906)+(w4*z)*(-.001388676377460993+z*2.439044879627741e-5))
        s,c = f32(sine*axis),cosine
        w = f32(f32(c*self.w)-f32(s*self.a))
        a = f32(f32(s*self.w)+f32(c*self.a))
        inv = f32(1/f32(math.sqrt(f32(f32(w*w)+f32(a*a)))))
        self.a,self.w = f32(inv*a),f32(inv*w)
        self.count += 1

    def matrix(self):
        """The part's base matrix: the quaternion's axes as sead::Matrix34::makeQT builds them
        (the same operation order as pose_math::quat_to_axes, so signed zeros agree)."""
        x,y,z,w = (self.a,0.,0.,self.w) if self.axis == 0 else (0.,self.a,0.,self.w)
        x2,y2,z2,w2 = f32(x+x),f32(y+y),f32(z+z),f32(w+w)
        xx,yy,zz = f32(x*x2),f32(y*y2),f32(z*z2)
        xy,xz,yz = f32(y*x2),f32(z*x2),f32(y2*z)
        xw,yw,zw = f32(x*w2),f32(y*w2),f32(z*w2)
        ax = (f32(f32(1-yy)-zz), f32(xy+zw), f32(xz-yw))
        ay = (f32(xy-zw), f32(f32(1-xx)-zz), f32(yz+xw))
        az = (f32(xz+yw), f32(yz-xw), f32(f32(1-xx)-yy))
        px,py,pz = self.position
        return [ax[0],ay[0],az[0],px, ax[1],ay[1],az[1],py, ax[2],ay[2],az[2],pz]

    def diagnostic(self):
        return f"{self.p['id']}/Rotate/{self.count}/"+'/'.join(bits(v) for v in self.read_back())

    def read_back(self):
        """al::calcQuat of the part's matrix (what the probe records as the actor quaternion):
        sead::Matrix34CalcCommon::toQuat takes the largest of the four components from the
        diagonal and the others from the differences, so a turn past 90 degrees flips the sign."""
        m = self.matrix()
        m00,m01,m02,m10,m11,m12,m20,m21,m22 = m[0],m[1],m[2],m[4],m[5],m[6],m[8],m[9],m[10]
        sq_w = f32(f32(f32(f32(f32(m00+m11)+m22)+1)*f32(.25)))
        sq_z = f32(sq_w-f32(f32(m00+m11)*f32(.5)))
        sq_y = f32(sq_w-f32(f32(m00+m22)*f32(.5)))
        sq_x = f32(sq_w-f32(f32(m11+m22)*f32(.5)))
        y_or_z = 2 if sq_y > sq_z else 3
        x_or_z = 1 if sq_x > sq_z else 3
        by_x = y_or_z if sq_x <= sq_y else x_or_z
        w_or_z = 0 if sq_w > sq_z else 3
        largest = y_or_z if sq_w <= sq_y else w_or_z
        if sq_w <= sq_x: largest = by_x
        if largest & 3 == 1:
            qx = f32(math.sqrt(sq_x)); k = f32(.25/qx)
            return [qx, f32(f32(m01+m10)*k), f32(f32(m02+m20)*k), f32(f32(m21-m12)*k)]
        if largest & 3 == 2:
            qy = f32(math.sqrt(sq_y)); k = f32(.25/qy)
            return [f32(f32(m10+m01)*k), qy, f32(f32(m12+m21)*k), f32(f32(m02-m20)*k)]
        if largest & 3 == 3:
            qz = f32(math.sqrt(sq_z)); k = f32(.25/qz)
            return [f32(f32(m20+m02)*k), f32(f32(m21+m12)*k), qz, f32(f32(m10-m01)*k)]
        qw = f32(math.sqrt(sq_w)); k = f32(.25/qw)
        return [f32(f32(m21-m12)*k), f32(f32(m02-m20)*k), f32(f32(m10-m01)*k), qw]


class Evidence:
    def __init__(self, geometry, actors, messages, mario, rolling, rewrite, rows, preroll, start):
        self.surfaces,self.rides = set(),set()
        self.id = self.host = None
        try: self._verify(geometry,actors,messages,mario,rolling,rewrite,rows,preroll,start)
        except (KeyError,TypeError,ValueError,IndexError,StopIteration,OverflowError,ZeroDivisionError,struct.error):
            self.surfaces,self.rides=set(),set()

    def _verify(self, geometry, actors, messages, mario, rolling, rewrite, rows, preroll, start):
        if not geometry or not actors or messages is None or not mario or not rolling or not start: return
        from environment_rule import key_values,parse_hit_hosts
        placements = geometry.get('rotate_parts',[])
        if len(placements)!=1: return
        p = placements[0]
        expected = PLACEMENTS.get((geometry.get('stage'),geometry.get('scenario'),p.get('id')))
        if not expected or p != dict(id=p['id'],model=expected['model'],axis=expected['axis'],speed=expected['speed'],host=expected['host'],**{'class':'RotateMapParts'}): return
        self.id, self.host = p['id'], p['host']
        obj = next(o for o in geometry['objects'] if o['id']==p['id'])
        if (obj['model']!=p['model'] or obj['static'] is not False or obj['translate']!=expected['translate']
                or obj['rotate']!=[0,0,0] or obj['scale']!=[1,1,1]): return
        triangles = {i:t for i,t in enumerate(geometry['triangles']) if geometry['objects'][t[13]]['id']==p['id']}
        if len(triangles)!=expected['triangles'] or not geometry['models'][p['model']+'#0']['prisms']: return
        raw = {t[17]:t for t in triangles.values() if t[16]==0}
        samples = {int(r['frame']):r for r in actors if r['id']==p['id']}
        # Only the phase at the first sample comes from the recording (the rotor ran before the
        # replay starts); every later sample must then follow the independent simulation.
        first = samples[min(samples)]
        source = Source(p,obj['translate'],int(first['executions']))
        all_rows=dict(preroll); all_rows.update(rows)
        c={int(r['frame']):r for r in mario}; contacts={int(r['frame']):r for r in rolling}
        if set(samples)!=set(all_rows) or set(c)!=set(all_rows) or set(contacts)!=set(all_rows) or min(samples)!=-30: return
        # PlayerTouch, PlayerFloorTouch, AskSafetyPoint, PlayerJumpTakeOffFloor.
        if any(r['answer']!='0' or r['message_vt'] not in ('1de9b18','1de93e8','1deb878','1d9dad8') for r in messages if r['id']==p['id']): return
        pointers=set(); surfaces=set(); rides=set(); generation=None
        for f,r in sorted(samples.items()):
            # At the first sample the collision part has just been synchronised: its previous
            # matrix is the current one (every later sample's previous is the last current).
            previous = source.matrix() if f==min(samples) else source.previous
            if (r['phase']!='Rotate' or r['n_vt']!='1df14a8' or r['axis']!=str(p['axis']) or r['speed_bits']!=bits(p['speed'])
                    or any(r[k]!='0' for k in ('dead','clipped','assist_timer','SwitchStart_valid','SwitchAppear_valid','registry_dropped','message_dropped'))
                    or r['collision_user_valid']!='1' or r['collision_system_valid']!='1' or r['zone']): return
            if generation is None: generation=r['generation']
            if r['generation']!=generation: return
            if (int(r['executions'])!=source.count or int(r['step'])!=(-1 if source.count==0 else source.count)
                    or floats(r['quat_bits'],4)!=source.read_back()
                    or floats(r['matrix_bits'],12)!=source.matrix()
                    or floats(r['previous_matrix_bits'],12)!=previous
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
                            or floats(side['ground_previous_matrix_bits'],12)!=previous): return
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
