"""Bounded Cascade linked-pipe evidence; side streams validate, never drive Mario.

The camera director boolean is the separately declared external camera input.
No visual column, movement error or unknown physical contact is waived here.
"""
import math

STAGE = "WaterfallWorldHomeStage"
PIPES = {"obj419": ("Dokan", "土管", "obj418", 80., 120.),
         "obj418": ("Dokan2D", "土管(2Dエリア内)", "obj419", 100., 125.)}
# Original Map.byml placements / InitSensor.byml, exported without runtime input.
MATRICES = {
    "obj419": [1.9470718e-7, .70710677, -.70710677, 2351.13,
               1., -1.8009622e-7, 9.526131e-8, 3500.,
               -5.998734e-8, -.70710677, -.70710677, -1891.5106],
    "obj418": [-.70710677, 0., .70710677, 2351.13, 0., 1., 0., 3600.,
               -.70710677, 0., -.70710677, -1891.5106],
}
AREAS = {
    "obj420": ("2DMoveArea", [1859.1152, 3096.221, -2383.5261], [1.8389943, 2.8932765, .5],
               dict(GravityOffset=0., IsFaceToPushOutDir=True, IsIgnoreArea=False,
                    PushOutDir=1, SnapGravityType=0, SurfaceDistance=5.)),
    "obj421": ("CameraArea2D", [1976.735, 3096.221, -2452.1633], [1.8824694, 2.8885438, .33659387],
               dict(IsEndNoInterpole=False, IsStartWhenCollideGround=False, IsStartWhenInWater=False)),
}
FLAGS = ("valid", "is2d", "in_area", "keep2d", "change2d", "change3d")
SENSORS = {"Foot": ("3", 40.), "Body": ("1", 50.), "Head": ("1", 40.),
           "2DFoot": ("3", 35.), "2DBody": ("1", 60.), "2DHead": ("1", 50.),
           "Carry": ("2", 130.), "HipDropKnockDown": ("2", 150.)}


def contract(g):
    if (g or {}).get("stage") != STAGE or g.get("scenario") != 1:
        return False
    areas = {a.get("id"): a for a in g.get("areas", [])}
    for id_, (name, trans, scale, args) in AREAS.items():
        a = areas.get(id_, {})
        if (a.get("name"), a.get("shape"), a.get("priority"), a.get("translate"),
                a.get("rotate"), a.get("scale"), a.get("args"), a.get("links")) != (
                name, "AreaCubeBase", 2, trans, [0., 135., 0.], scale, args, []):
            return False
        expected = [-.70710677, 0., .70710677, trans[0], 0., 1., 0., trans[1],
                    -.70710677, 0., -.70710677, trans[2]]
        if a.get("matrix") != expected:
            return False
    sources = g.get("dokans", [])
    if len(sources) != 2:
        return False
    for id_, (cls, host, dst, radius, height) in PIPES.items():
        matches = [s for s in sources if s.get("placement", {}).get("Id") == id_]
        parts = [s for s in g.get("objects", []) if s.get("id") == id_]
        if len(matches) != 1 or len(parts) != 1:
            return False
        s, part = matches[0], parts[0]
        p = s["placement"]
        if (p.get("UnitConfigName") != cls or p.get("UnitConfig", {}).get("DisplayName") != host
                or s.get("destination_id") != dst or s.get("raw_matrix") != MATRICES[id_]
                or part.get("matrix") != MATRICES[id_] or part.get("scale") != [1., 1., 1.]
                or part.get("model") != ("DokanChange2D" if id_ == "obj419" else "Dokan2D")
                or p.get("Scale") != dict(X=1., Y=1., Z=1.)
                or any(p.get(k) is not False for k in ("IsCannotIn", "IsConnectToCollision",
                       "IsExitOnly", "IsMoon", "IsNoSnap", "IsUseObjectCamera", "IsUseStartCamera"))
                or p.get("CameraInterpolate") is not True):
            return False
        expected = [dict(Name="Bind", Radius=radius, Type="BindableAllPlayer", Y=height)]
        expected += ([dict(Name="Body", Radius=80., Type="MapObj", Y=130.)] if id_ == "obj419"
                     else [dict(MaxCount=0, Name="Collision", Radius=0., Type="CollisionParts")])
        if s.get("sensors") != expected:
            return False
    return True


def pairs(text):
    return dict(p.split("=", 1) for p in text.split(";") if "=" in p)


class Evidence:
    def __init__(self, geometry, dimensions, sensors, messages, rewrite, game):
        self.frames, self.bound, self.sensors, self.messages = set(), set(), {}, {}
        self.hosts = {p[1]: id_ for id_, p in PIPES.items()}
        if not contract(geometry) or not dimensions or not sensors or not messages:
            return
        samples = {}
        for d in dimensions:
            f = int(d["frame"])
            if f in samples:
                return
            samples[f] = d
        for s in sensors:
            key = (int(s["frame"]), s["id"], s["sensor"])
            if key in self.sensors:
                return
            self.sensors[key] = s
        for m in messages:
            if m.get("kind") == "send_after":
                self.messages.setdefault(int(m["frame"]), []).append(m)
        for f in game:
            d, r = samples.get(f, {}), rewrite.get(f, {})
            flags = "/".join(d.get(k, "") for k in FLAGS)
            native = pairs(r.get("dimension_keeper", ""))
            pipe = pairs(r.get("dokan_source", ""))
            sources = [s["placement"]["Id"] for s in geometry["dokans"]]
            bound = pipe.get("bound")
            source = sources[int(bound)] if bound and bound.isdecimal() and int(bound) < len(sources) else "none"
            if (source != d.get("puppet_source_id") or
                    any(pipe.get(k) != d.get(v) for k, v in
                        [("hidden", "hidden"), ("collider", "collision_enabled"), ("sensors", "sensors_enabled")])):
                continue
            if (d.get("dropped") != "0" or d.get("registry_dropped") != "0"
                    or d.get("keeper_present") != "1" or native.get("flags") != flags
                    or d.get("model2d") != d.get("is2d")
                    or d.get("filter_is_2donly") != d.get("is2d")
                    or d.get("shape_is_2dnormal") != d.get("is2d")):
                continue
            if d.get("is2d") == "1" and (native.get("current"), native.get("last")) != ("obj420", "obj420"):
                continue
            if d.get("binding") == "1":
                if (d.get("puppet_source_id") not in PIPES or r.get("state") != "Bind/Bind"):
                    continue
                self.bound.add(f)
            self.frames.add(f)
        # BindEnd releases the puppet before Mario runs EndFall. The actor state
        # still belongs to the proved bind episode for that final update.
        self.bound.update(f for f in self.frames
                          if rewrite[f].get("state") == "Bind/EndFall" and f - 1 in self.bound)

    def contact(self, f, c):
        if f not in self.frames or c["who"] != "player" or c["host_class"] != "Dokan":
            return False
        id_ = self.hosts.get(c["host"])
        if id_ is None or (c["other"], c["other_type"]) not in (
                (("Bind", "20"), ("Body", "12")) if id_ == "obj419" else (("Bind", "20"),)):
            return False
        other = self.sensors.get((f, id_, c["other"]), {})
        if c["actor"] != other.get("actor"):
            return False
        if c["kind"] in ("r", "p"):
            return (c["kind"] == "r" and c["message"] == "al::SensorMsgBindEnd" and c["answer"] == "1"
                    and any(m.get("sender_id") == id_ and m.get("receiver_id") == "Mario"
                            and m.get("message_type") == c["message"] and m.get("answer") == "1"
                            for m in self.messages.get(f, [])))
        if c["kind"] != "a" or c["message"] or c["answer"]:
            return False
        if (c["self"], c["self_type"]) == ("Eye", "5"):
            return True
        own = self.sensors.get((f, "Mario", c["self"]), {})
        expected = SENSORS.get(c["self"])
        if expected is None or expected != (c["self_type"], float(own.get("radius", "nan"))):
            return False
        # HitSensor director uses the preceding sensor positions. A bind can disable
        # those sensors before this sample; require the actual prior overlap too.
        for frame in (f - 1, f):
            a = self.sensors.get((frame, "Mario", c["self"]), {})
            b = self.sensors.get((frame, id_, c["other"]), {})
            if a.get("valid") != "1" or b.get("valid") != "1":
                continue
            try:
                delta = [float(a["centre_" + k]) - float(b["centre_" + k]) for k in "xyz"]
                radius = float(a["radius"]) + float(b["radius"])
                if all(math.isfinite(x) for x in delta) and sum(x*x for x in delta) < radius*radius:
                    return True
            except (KeyError, ValueError):
                pass
        return False


def self_check():
    import copy
    from environment_rule import judge_run, parse_contacts
    g = dict(stage=STAGE, scenario=1, areas=[], dokans=[], objects=[])
    for id_, (name, trans, scale, args) in AREAS.items():
        g["areas"].append(dict(id=id_, name=name, shape="AreaCubeBase", priority=2,
            translate=trans, rotate=[0., 135., 0.], scale=scale, args=args, links=[],
            matrix=[-.70710677, 0., .70710677, trans[0], 0., 1., 0., trans[1],
                    -.70710677, 0., -.70710677, trans[2]]))
    for id_, (cls, host, dst, radius, height) in PIPES.items():
        p = dict(Id=id_, UnitConfigName=cls, UnitConfig=dict(DisplayName=host),
                 Scale=dict(X=1., Y=1., Z=1.), CameraInterpolate=True)
        p.update({k: False for k in ("IsCannotIn", "IsConnectToCollision", "IsExitOnly",
                                   "IsMoon", "IsNoSnap", "IsUseObjectCamera", "IsUseStartCamera")})
        sensors = [dict(Name="Bind", Radius=radius, Type="BindableAllPlayer", Y=height)]
        sensors += ([dict(Name="Body", Radius=80., Type="MapObj", Y=130.)] if id_ == "obj419"
                    else [dict(MaxCount=0, Name="Collision", Radius=0., Type="CollisionParts")])
        g["dokans"].append(dict(placement=p, destination_id=dst, raw_matrix=MATRICES[id_], sensors=sensors))
        g["objects"].append(dict(id=id_, matrix=MATRICES[id_], scale=[1., 1., 1.],
                               model="DokanChange2D" if id_ == "obj419" else "Dokan2D"))
    host = PIPES["obj419"][1]
    contact = f"k=a;w=player;s=Body:1;o=Bind:20;h={host};c=Dokan;m=;r=;actor=7"
    game = {0: dict(frame="0", state="Bind/Bind", contacts=contact)}
    native = {0: dict(state="Bind/Bind", dimension_keeper="flags=0/0/1/0/0/0;current=;last=",
                      dokan_source="bound=0;hidden=0;collider=1;sensors=1",
                      collision="gcode=Ground")}
    sample = dict(frame="0", dropped="0", registry_dropped="0", keeper_present="1", model2d="0",
                  filter_is_2donly="0", shape_is_2dnormal="0", binding="1", puppet_source_id="obj419",
                  hidden="0", collision_enabled="1", sensors_enabled="1")
    sample.update(dict(zip(FLAGS, ("0", "0", "1", "0", "0", "0"))))
    sensors = [dict(frame=str(f), id=id_, sensor=name, actor=actor, radius=str(radius),
                    valid="1", centre_x="0", centre_y="0", centre_z="0")
               for f in [-1, 0] for id_, name, actor, radius in
               [("Mario", "Body", "0", 50.), ("obj419", "Bind", "7", 80.)]]
    messages = [dict(frame="0", kind="send_after", sender_id="Mario", receiver_id="obj419",
                     message_type="al::SensorMsgBindInit", answer="1")]
    evidence = dict(geometry=g, dimensions=[sample], dimension_sensors=sensors, dimension_events=messages)
    listed = {0: [f"hit-sensor contact {host}"]}
    assert not judge_run(game, native, listed, **evidence).refused
    for field, value in [("stage", "OtherStage"), ("scenario", 2), ("dokans", [])]:
        bad = copy.deepcopy(g); bad[field] = value
        assert judge_run(game, native, listed, **dict(evidence, geometry=bad)).refused
    for mutate in [lambda x: x["areas"][0].update(id="obj999"),
                   lambda x: x["areas"][1].update(priority=3),
                   lambda x: x["dokans"][0]["placement"].update(IsExitOnly=True),
                   lambda x: x["dokans"][0].update(destination_id="obj999"),
                   lambda x: x["dokans"][0]["sensors"][0].update(Radius=81.),
                   lambda x: x["objects"][0].update(matrix=[0.] * 12)]:
        bad = copy.deepcopy(g); mutate(bad)
        assert judge_run(game, native, listed, **dict(evidence, geometry=bad)).refused
    for bad in [contact.replace("actor=7", "actor=8"), contact.replace("w=player", "w=cap"),
                contact.replace("Body:1", "Body:2"), contact.replace("c=Dokan", "c=Enemy"),
                contact.replace("m=;r=", "m=al::SensorMsgPush;r=1"),
                contact.replace("o=Bind:20", "o=Other:20")]:
        assert judge_run({0: dict(game[0], contacts=bad)}, native, listed, **evidence).refused
    assert judge_run(game, {0: dict(native[0], dimension_keeper="")}, listed, **evidence).refused
    assert judge_run(game, native, listed, **dict(evidence, dimension_sensors=sensors + sensors[:1])).refused
    assert judge_run(game, native, listed, **dict(evidence, dimensions=[sample, sample])).refused
    end = contact.replace("k=a", "k=r").replace("m=;r=", "m=al::SensorMsgBindEnd;r=1")
    e = Evidence(g, [sample], sensors, messages, native, game)
    assert not e.contact(0, parse_contacts(end)[0])
    messages.append(dict(frame="0", kind="send_after", sender_id="obj419", receiver_id="Mario",
                         message_type="al::SensorMsgBindEnd", answer="1"))
    e = Evidence(g, [sample], sensors, messages, native, game)
    assert e.contact(0, parse_contacts(end)[0])
    assert not e.contact(0, parse_contacts(end.replace("BindEnd", "BindStart"))[0])
