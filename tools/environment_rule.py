"""Which environment contamination of a real-game run is harmless, and which is not.

The probe lists every frame in which the run touched something besides flat, empty ground
(`game.env.json`: a wall, a slope, a floor change, another actor's hit sensor, ...). Most of
those frames are not contact at all or are static map geometry. This module judges them for
tools/compare-traces.py. The rule and the evidence behind it come from the Cap Kingdom analysis
of Mario's sensors (docs/fidelity-harness.md, "Environment rule"):

* Mario owns an `Eye` sensor (type PlayerEye, radius 800 cm). The engine calls his attack path
  for every actor sensor inside it, but that path only records look-at targets and sends
  queries; nothing that changes Mario happens. An eye overlap is never contamination.
* A physical overlap (any other sensor of Mario) with an actor whose handlers ignore him (cap
  ships and hangers, the cap door, cap flowers) is harmless, and so is one with a reaction
  object when nothing happened. Both are accepted only on frames without a consequence marker.
* Anything else touching Mario (NPCs, items, coins, enemies, bushes, unknown classes), a message
  Mario's own handlers act on, and Cappy's contacts are refused.
* Wall, ceiling, slope and floor entries are the map's static geometry. They are accepted only
  when the rewrite replayed the same stage's triangles (`--geometry`) and the surfaces' hosts
  are static; without that the flat-floor model cannot reproduce them.
* `floor height changed` on the frame a fall lands is the intended drop, not a moving floor.
* Damage/death episodes need an entry caused by exported static floor codes or a DeathArea.
  During Dead, a typed player attack overlap with a probe-tagged player-death Coin is accepted.
  Every other actor contact is still judged independently.

Two kinds of probe log exist. A log with the `contacts` and `hit_hosts` columns (typed) names
both sensors, the message and its answer, and the host of every surface. An older log only has
`hit-sensor contact <host name>` reasons, which cannot tell an eye overlap from a physical one,
so those names are judged as if physical and the surface hosts cannot be read (noted).
"""
import math
import re
import struct
from sand_geyser import Evidence as GeyserEvidence
from collections import OrderedDict

# ---- what the probe logs --------------------------------------------------------------------------

SENSOR_TYPE_PLAYER_EYE = "5"  # al::HitSensorType::PlayerEye

# Hosts whose handlers do not react to a player-sent physical message (receiveMsg only answers
# cap lock-on and queries, attackSensor is empty or only drives the actor's own animation):
# (label, actor class names, display-name prefixes). A typed probe log names the host's class, which
# decides; a log without it is judged by the display name (an older probe cuts names at 48 bytes, so
# a cut name matches by its remaining prefix).
HARMLESS_HOSTS = (
    ("cap ship / hanger", ("CapHanger",),
     ("帽子のワールドホーム帽子船", "帽子のワールドホーム帽子ひっかけポイント", "帽子ひっかけポイント")),
    ("cap door", ("DoorAreaChange",), ("帽子ドア",)),
    ("cap flower", ("CapFlower",), ("帽子花",)),
)
# Hosts whose handlers accept a trample/hip drop/kick: harmless unless one happened on the frame
# or the next (a trample from above bounces Mario).
CONDITIONAL_HOSTS = (("reaction object", ("ReactionObject",), ("反応するオブジェ",)),)

# Messages Mario's receiveMsg/receivePushMsg acts on (bind, hack, hit, damage, blow, snap, jump
# requests, wait/stain, geyser, hold, enemy fire/explosion/poison/needle, push, velocity).
HANDLED_MESSAGE = re.compile(
    r"SensorMsg(Bind|Hack|InitHack|StartHack|EndHack|CancelHack|TransferHack|Hit(Strong|VeryStrong)?$|"
    r".*Damage|Blow|ObjSnapForce|Request(PlayerJump|PlayerJumpBreakFloor|PlayerSpinJump|PlayerTrampleJump|PlayerWaitAnim|PlayerStainWet)|"
    r"SandGeyserRaise|LiftGeyser|HoldCancel|EnemyAttack|.*Explosion|.*CactusNeedle|AttachCactusNeedle|Push|PushToPlayer|"
    r"PushVelocity|CollisionImpulse|KnockDown|PressureDeath|ForceAbyss)")

# These states need a cause; static collision/death-area causes are proved below.
UNSCRIPTED_STATES = ("Bind", "Hack", "Damage", "Demo", "ShineGet", "Press", "SandSink", "Abyss", "Dead")
# States in which the position follows a wall, a pole or a hang: displacement differs from velocity.
SURFACE_STATES = ("Pole", "WallCatch", "WallAir", "WallGrab", "WallSlide", "Hang", "GrabCeil")

AIR_RESIDUAL = 0.05  # cm: |p(f) - p(f-1) - v(f)| measured 0.00 on every airborne frame without a wall
GROUND_RESIDUAL = 1.0  # cm: the probe's own threshold for a grounded frame
FLAT_NORMAL_DEG = 1.0  # a floor within this angle of vertical is "flat"
FLOOR_PLANE_TOLERANCE = 0.05  # cm: a flat floor under a stationary player does not change height


def decode_name(name):
    """A host name as the older probe logged it (cut at 48 bytes, ending in U+FFFD) or complete."""
    cut = name.endswith("�")
    return name.rstrip("�"), cut


def host_family(name, host_class, families):
    """The label of the family a host belongs to, or None. The class decides when the log names it."""
    base, cut = decode_name(name)
    for label, classes, prefixes in families:
        if host_class:
            if host_class in classes:
                return label
            continue
        for prefix in prefixes:
            if base.startswith(prefix) or (cut and base and prefix.startswith(base)):
                return label
    return None


def unescape(text):
    """Undo the %XX escaping the probe tooling applies to separators inside names."""
    return re.sub(r"(?:%[0-9A-Fa-f]{2})+",
                  lambda m: bytes.fromhex(m.group(0).replace("%", "")).decode("utf-8", "replace"), text)


def parse_contacts(cell):
    """The `contacts` column: items `k=a;w=player;s=Body:1;o=Body:12;h=<host>;c=<class>;m=<message class>;r=1`."""
    out = []
    for item in (cell or "").split("|"):
        if not item:
            continue
        fields = {}
        for part in item.split(";"):
            key, _, value = part.partition("=")
            fields[key] = value
        sname, _, stype = fields.get("s", ":").rpartition(":")
        oname, _, otype = fields.get("o", ":").rpartition(":")
        out.append({
            "kind": fields.get("k", ""), "who": fields.get("w", ""),
            "self": unescape(sname), "self_type": stype, "other": unescape(oname), "other_type": otype,
            "host": unescape(fields.get("h", "")), "host_class": unescape(fields.get("c", "")),
            "message": unescape(fields.get("m", "")), "answer": fields.get("r", ""),
            "actor": fields.get("actor", ""), "origin": fields.get("origin", ""),
        })
    return out


def parse_hit_hosts(cell):
    """The `hit_hosts` column: `ground=<host>/<sensor>/<type>/<_15c>/<isMoving>/<bx>/<by>/<bz>;wall=...;ceiling=...`."""
    out = {}
    for item in (cell or "").split(";"):
        label, _, rest = item.partition("=")
        if not label or not rest:
            continue
        fields = rest.split("/")
        fields += [""] * (8 - len(fields))
        base = None
        try:
            base = tuple(float(x) for x in fields[5:8])
        except ValueError:
            pass
        out[label] = {"host": unescape(fields[0]), "sensor": unescape(fields[1]), "sensor_type": fields[2],
                      "parts_flag": fields[3], "moving": fields[4], "base": base}
    return out


def number(text, default=None):
    try:
        return float(text)
    except (TypeError, ValueError):
        return default


def key_values(text, sep=";"):
    out = {}
    for item in (text or "").split(sep):
        key, _, value = item.partition("=")
        if key:
            out[key] = value
    return out


def ranges(frames):
    """Frame numbers as `3-7,9`."""
    frames = sorted(set(frames))
    parts, start = [], None
    for i, f in enumerate(frames):
        if start is None:
            start = prev = f
        elif f == prev + 1:
            prev = f
        else:
            parts.append((start, prev))
            start = prev = f
        if i == len(frames) - 1:
            parts.append((start, prev))
    return ",".join(f"{a}" if a == b else f"{a}-{b}" for a, b in parts)


class Verdict:
    """What the rule decided about a run."""

    def __init__(self):
        self.typed = False  # the log names sensors and surface hosts
        self.geometry_replay = False  # the rewrite replayed the stage's triangles
        self.accepted = OrderedDict()  # label -> {"why": text, "frames": set}
        self.refused = OrderedDict()  # frame -> [reason text]
        self.eye_overlaps = OrderedDict()  # host -> set of frames (listed for information, never contamination)
        self.notes = []

    def accept(self, frame, label, why):
        entry = self.accepted.setdefault(label, {"why": why, "frames": set()})
        entry["frames"].add(frame)

    def refuse(self, frame, reason):
        reasons = self.refused.setdefault(frame, [])
        if reason not in reasons:
            reasons.append(reason)

    def lines(self):
        """Markdown bullet lines describing every accepted contamination and every refusal."""
        out = []
        for label, entry in self.accepted.items():
            out.append(f"accepted {label} on frames {ranges(entry['frames'])}: {entry['why']}")
        for host, frames in self.eye_overlaps.items():
            out.append(f"accepted eye overlap with {host} on frames {ranges(frames)}: Mario's 800 cm eye sensor only "
                       "records look-at targets and answers queries; nothing reaches Mario")
        by_reason = OrderedDict()
        for frame, reasons in self.refused.items():
            for reason in reasons:
                by_reason.setdefault(reason, set()).add(frame)
        for reason, frames in by_reason.items():
            out.append(f"REFUSED on frames {ranges(frames)}: {reason}")
        return out


class Judge:
    """Judges the frames of one run. See the module text for the rule."""

    def __init__(self, game_rows, rewrite_rows, listed, geometry=None, start=None, preroll=None, start_triangle=None, geysers=None, geyser_seed=None, geyser_messages=None):
        self.rows = game_rows
        self.preroll = preroll or {}
        self.listed = listed
        self.verdict = Verdict()
        v = self.verdict
        v.typed = any(r.get("hit_hosts") or r.get("contacts") for r in game_rows.values())
        v.geometry_replay = any(
            re.search(r"(^|;)(gcode|gmat|ceiling)=", r.get("collision", "")) for r in rewrite_rows.values()
        )
        self._hosts = {f: parse_hit_hosts(r.get("hit_hosts")) for f, r in game_rows.items()} if v.typed else {}
        self._markers = {}
        # (surface, host) pairs whose collision part changes position during the run
        spans = {}
        for f, hosts in self._hosts.items():
            for surface, host in hosts.items():
                if host["base"] is not None:
                    spans.setdefault((surface, host["host"]), []).append(host["base"])
        self._moving_by_position = {
            key for key, bases in spans.items()
            if any(max(b[i] for b in bases) - min(b[i] for b in bases) > 0.01 for i in range(3))
        }
        self._geysers = GeyserEvidence(geometry, geysers, rewrite_rows, geyser_seed, geyser_messages)
        self._scripted = {}
        self._candles = []
        self._start = start
        if geometry and start is not None and v.geometry_replay:
            for candle in geometry.get("candlestands", []):
                part = next((o for o in geometry.get("objects", []) if o.get("id") == candle.get("id")), {})
                position = part.get("translate", [])
                if (candle.get("is_burn_out") is False and candle.get("settled") is True
                        and part.get("static") is True and part.get("model") == "LavaWorldCandlestand000"
                        and part.get("rotate") == [0, 0, 0] and part.get("scale") == [1, 1, 1]
                        and len(position) == 3
                        and candle.get("fire_position") == [position[0], f32(position[1] + 200), position[2]]
                        and candle.get("body_position") == [position[0], f32(position[1] + 210), position[2]]):
                    self._candles.append(candle)
        if geometry and start is not None and v.geometry_replay:
            self._classify_damage(geometry, start, preroll or {}, start_triangle or {})

    def _classify_damage(self, geometry, start, preroll, start_triangle):
        """Prove an episode's entry, never infer it from its scenario name or its outcome.

        checkDamageFromCollision reads the previous collision before the movement step;
        checkDeathArea reads the position. A fire episode includes its own Dead substate.
        Actor messages/contacts are still judged independently, even in a proved episode.
        """
        surfaces = {}
        initial = set()
        vertices = start_triangle.get("vertices", [])
        # Exported world vertices are serialized to three decimals. Equality here is
        # equality of that serialization, not a distance tolerance or the runner's
        # older approximate floor_triangle_match claim.
        vertex_key = lambda vs: sorted(tuple(round(x, 3) for x in p) for p in vs)
        for tri in geometry.get("triangles", []):
            obj = geometry["objects"][tri[13]]
            if obj.get("static") is not True:
                continue
            attr = geometry["attributes"][tri[12]]
            code = (attr.get("FloorCode"), attr.get("MaterialCode"))
            base = tuple(float(f"{f32(f32(x) - f32(s)):.6f}") for x, s in zip(obj["translate"], start))
            surfaces.setdefault(base, set()).add(code)
            if (start_triangle.get("found") and len(vertices) == 3 and len(tri) >= 16
                    and tri[14:16] == [start_triangle.get("kcl_triangle_index"), start_triangle.get("kcl_attribute_index")]
                    and vertex_key([tri[0:3], tri[3:6], tri[6:9]]) == vertex_key(vertices)):
                initial.add(code)

        rows = dict(preroll)
        rows.update(self.rows)
        previous_family, cause = "", None
        for f, row in sorted(rows.items()):
            family = row.get("state", "").split("/")[0]
            if family != previous_family:
                cause = cause if family == "Dead" and previous_family in ("Damage", "DamageFire", "Abyss", "SandSink") else None
                if family in ("DamageFire", "Damage", "DamageSwim", "SandSink", "Dead") and cause is None:
                    codes = ({"DamageFire"} if family == "DamageFire" else {"Needle", "Poison"}
                             if family == "Damage" else {"Needle"} if family == "DamageSwim"
                             else {"SandSink"} if family == "SandSink" else {"Poison"})
                    if family == "DamageFire":
                        for c in parse_contacts(row.get("contacts")):
                            if self._candle_contact(f, c, message=True) and c["answer"] == "1":
                                cause = f"exported candlestand EnemyAttackFire at frame {f}"
                                break
                    for g in (f - 1, f):
                        if cause:
                            break
                        sample = rows.get(g, {})
                        flags = key_values(sample.get("collision", ""))
                        hosts = parse_hit_hosts(sample.get("hit_hosts"))
                        # Current probe exposes the floor code only. Wall/ceiling
                        # material names alone cannot establish their damage code.
                        code = (flags.get("gcode"), flags.get("gmat"))
                        host = hosts.get("ground", {})
                        static = (host.get("moving") == "0" and host.get("parts_flag") != "0"
                                  and ("ground", host.get("host")) not in self._moving_by_position)
                        exported = static and code in surfaces.get(host.get("base"), set())
                        # Preroll lacks hit_hosts. Its first sample can instead use
                        # the probe's actual start KCL triangle, verified above.
                        exported = exported or (g == min(rows) and g in preroll and code in initial)
                        if flags.get("ground") == "1" and code[0] in codes and exported:
                            cause = f"exported static {code[0]} floor ({code[1]}) at frame {g}"
                            break
                elif family == "Abyss":
                    for g in (f - 1, f):
                        sample = rows.get(g, {})
                        p = [number(sample.get(k)) for k in ("px", "py", "pz")]
                        if None not in p:
                            absolute = [f32(f32(p[i]) + f32(start[i])) for i in range(3)]
                            for area in geometry.get("areas", []):
                                if in_death_area(area, absolute):
                                    cause = f"exported DeathArea {area.get('id', '')} at frame {g}"
                                    break
            if cause:
                self._scripted[f] = cause
            previous_family = family

    def _unscripted(self, f):
        return self._state(f).startswith(UNSCRIPTED_STATES) and f not in self._scripted

    # ---- facts about a frame, from the game trace ----

    def _row(self, f):
        return self.rows.get(f)

    def _flags(self, f):
        return key_values(self.rows[f].get("collision", "")) if f in self.rows else {}

    def _state(self, f):
        row = self._row(f)
        return row.get("state", "").split("/")[0] if row else ""

    def _normal_off_vertical(self, f):
        row = self._row(f)
        parts = (row.get("floor_normal", "") if row else "").split("/")
        if len(parts) != 3 or None in (number(parts[1]),):
            return None
        return math.degrees(math.acos(max(-1.0, min(1.0, number(parts[1])))))

    def _transition(self, f):
        """A state or ground change between f-1 and f or f and f+1, or a jump frame (`action` trigger)."""
        for a, b in ((f - 1, f), (f, f + 1)):
            if a in self.rows and b in self.rows:
                if self._state(a) != self._state(b) or self.rows[a].get("ground") != self.rows[b].get("ground"):
                    return True
        trig = key_values(self.rows[f].get("triggers", "")) if f in self.rows else {}
        return int(trig.get("action", "0") or "0", 16) != 0

    def markers(self, f):
        """Consequence markers on frame f: signs that something changed Mario, not just the map."""
        if f in self._markers:
            return self._markers[f]
        found = []
        row = self._row(f)
        if row is None:
            self._markers[f] = found
            return found
        trig = key_values(row.get("triggers", ""))
        for key in ("attack", "receive", "collision"):
            if int(trig.get(key, "0") or "0", 16) != 0:
                found.append(f"trigger {key} set")
        state = self._state(f)
        if self._unscripted(f):
            found.append(f"state {state}")
        prev = self._row(f - 1)
        flags = self._flags(f)
        if prev is not None and not (
            flags.get("wall") == "1" or flags.get("ceiling") == "1" or self._transition(f)
            or state.startswith(SURFACE_STATES) or self._state(f - 1).startswith(SURFACE_STATES)
        ):
            p = [number(row.get(k)) for k in ("px", "py", "pz")]
            q = [number(prev.get(k)) for k in ("px", "py", "pz")]
            v = [number(row.get(k)) for k in ("vx", "vy", "vz")]
            if None not in p + q + v:
                residual = [p[i] - q[i] - v[i] for i in range(3)]
                if row.get("ground") == "1":
                    tilt = self._normal_off_vertical(f)
                    if (tilt is None or tilt <= FLAT_NORMAL_DEG) and math.hypot(residual[0], residual[2]) > GROUND_RESIDUAL:
                        found.append(f"ground displacement differs from velocity by {math.hypot(residual[0], residual[2]):.2f} cm")
                elif math.sqrt(sum(r * r for r in residual)) > AIR_RESIDUAL:
                    found.append(f"air displacement differs from velocity by {math.sqrt(sum(r * r for r in residual)):.2f} cm")
        self._markers[f] = found
        return found

    # ---- reasons ----

    def judge(self):
        v = self.verdict
        if v.typed:
            for f in self.rows:
                self._judge_typed_contacts(f)
        for f, cause in self._scripted.items():
            if f in self.rows:
                v.accept(f, "scripted damage/death", cause)
        for f, reasons in self.listed.items():
            if self._unscripted(f):
                v.refuse(f, f"state {self._state(f)} that the scenario does not script")
            for reason in reasons:
                self._judge_reason(f, reason)
        return v

    def _judge_reason(self, f, reason):
        v = self.verdict
        m = re.match(r"hit-sensor contact (.*)$", reason)
        if m:
            if v.typed:
                return  # judged from the typed records of the frame
            self._judge_named_contact(f, m.group(1))
            return
        if reason.startswith("wall contact"):
            self._geometry(f, reason, "wall contact", ("wall",))
        elif reason.startswith("ceiling contact"):
            self._geometry(f, reason, "ceiling contact", ("ceiling",))
        elif reason.startswith("floor normal"):
            self._geometry(f, reason, "slope", ("ground",))
        elif reason.startswith("floor material changed"):
            self._geometry(f, reason, "floor material change", ("ground",))
        elif reason.startswith("floor height changed"):
            self._floor_height(f, reason)
        elif reason.startswith("horizontal displacement differs"):
            self._displacement(f, reason)
        else:
            v.refuse(f, reason)

    # ---- actor contacts ----

    def _judge_named_contact(self, f, name):
        """Old log: only the host name. It cannot tell an eye overlap from a physical one, so judge as physical."""
        v = self.verdict
        if name.endswith("@cap"):
            v.refuse(f, f"Cappy touched {name[:-4]}")
            return
        self._physical_overlap(f, name, "", note=" (older probe log: the name alone, eye and physical overlaps are not told apart)")

    def _physical_overlap(self, f, host, detail, note="", host_class=""):
        v = self.verdict
        family = host_family(host, host_class, HARMLESS_HOSTS)
        conditional = host_family(host, host_class, CONDITIONAL_HOSTS)
        if family is None and conditional is None:
            v.refuse(f, f"physical contact with {host or '(unnamed)'}{detail}: not a host the engine's handlers ignore")
            return
        frames = (f, f + 1) if conditional else (f,)
        for g in frames:
            marks = self.markers(g)
            if marks:
                v.refuse(f, f"physical contact with {host}{detail} on a frame with consequence markers ({'; '.join(marks)})")
                return
        label = conditional or family
        why = ("the host's handlers have no reaction to a player-sent physical message, no consequence marker on the frame"
               if family else
               "a reaction object only reacts to a trample/hip drop/kick; no trigger, state change or displacement residual "
               "on the frame or the next") + note
        v.accept(f, f"physical overlap with {label}", why)

    def _candle_contact(self, f, c, message=False):
        """Only the reproduced Fire/Body pair, never a generic Candlestand waiver.

        Hit checks precede movement and use the preceding pose. PlayerAttack
        overlaps do not damage the player; received fire requires Player/PlayerFoot.
        """
        if not self._candles or c["who"] != "player" or c["host_class"] != "Candlestand":
            return False
        if message:
            if (c["kind"] not in ("r", "p") or (c["kind"] == "p" and c["answer"] != "0") or c["message"] != "al::SensorMsgEnemyAttackFire"
                    or c["answer"] not in ("0", "1") or (c["other"], c["other_type"]) != ("Fire", "10")):
                return False
        elif c["kind"] != "a" or c["message"] or c["answer"]:
            return False
        player = {("Foot", "3"): (40., 40.), ("Body", "1"): (75., 50.), ("Head", "1"): (110., 40.)}
        if not message:
            player.update({("Carry", "2"): (100., 130.), ("SpinAttack", "2"): (100., 130.),
                           ("HipDropKnockDown", "2"): (0., 150.)})
        own = player.get((c["self"], c["self_type"]))
        other = {("Fire", "10"): ("fire_position", 40.), ("Body", "12"): ("body_position", 80.)}.get(
            (c["other"], c["other_type"]))
        if own is None or other is None:
            return False
        row = self.rows.get(f - 1, self.preroll.get(f - 1, {}))
        feet = [number(row.get(k)) for k in ("px", "py", "pz")]
        if None in feet:
            return False
        feet = [f32(f32(x) + f32(s)) for x, s in zip(feet, self._start)]
        feet[1] = f32(feet[1] + own[0])
        for candle in self._candles:
            if c["host"] != candle["host"]:
                continue
            delta = [f32(a - f32(b)) for a, b in zip(feet, candle[other[0]])]
            squared = f32(f32(f32(delta[0] * delta[0]) + f32(delta[1] * delta[1])) + f32(delta[2] * delta[2]))
            if squared < (own[1] + other[1]) ** 2:
                return True
        return False

    def _judge_typed_contacts(self, f):
        v = self.verdict
        for c in parse_contacts(self.rows[f].get("contacts")):
            host = c["host"]
            where = f" ({c['self']}:{c['self_type']} vs {c['other']}:{c['other_type']}" + (f", {c['host_class']}" if c["host_class"] else "") + ")"
            if c["who"] == "cap":
                v.refuse(f, f"Cappy's {c['kind']} contact with {host}{where}")
                continue
            if self._geysers.contact(f, c, self.rows, self.preroll, self._start):
                v.accept(f, "exported SandGeyser source", "placement-bound source with independently verified phase and reproduced timeline")
                continue
            if self._candle_contact(f, c, message=c["kind"] in ("r", "p")):
                v.accept(f, "exported candlestand sensor",
                         "settled initially lit static placement; exact Fire/Body sphere overlap replayed")
                continue
            if c["kind"] in ("r", "p"):
                message = c["message"]
                if HANDLED_MESSAGE.search(message) or (not message and c["answer"] == "1"):
                    v.refuse(f, f"Mario received {message or 'an unresolved message'} from {host}{where}, answered {c['answer'] or '?'}")
                    continue
                v.accept(f, f"message {message or '(unresolved, answered false)'}",
                         "Mario's receive handlers ignore this message (or answered false)")
            if c["self_type"] == SENSOR_TYPE_PLAYER_EYE:
                v.eye_overlaps.setdefault(host, set()).add(f)
                continue
            if c["kind"] == "a":
                if (c["who"] == "player" and c["host_class"] == "Coin"
                        and re.fullmatch(r"[1-9][0-9]*", c["actor"])
                        and c["origin"] == "player_death_coin"
                        and "Dead" in self.rows[f].get("state", "").split("/") and f in self._scripted):
                    v.accept(f, "player death-drop coin",
                             f"probe actor {c['actor']} tagged by Coin death appearance, during proved Dead episode")
                    continue
                self._physical_overlap(f, host, where, host_class=c["host_class"])

    # ---- map geometry ----

    def _hosts_static(self, f, surfaces):
        """(static?, why not) for the hosts of the given surfaces on frame f, from the typed log."""
        hosts = self._hosts.get(f, {})
        for surface in surfaces:
            host = hosts.get(surface)
            if host is None:
                return False, f"no {surface} host logged on this frame"
            if self._geysers.floor(f, host, self._start):
                continue
            if host["moving"] == "1" or host["parts_flag"] == "0":
                return False, f"{surface} host {host['host']} is a moving collision part"
            if (surface, host["host"]) in self._moving_by_position:
                return False, f"{surface} host {host['host']} changes position during the run"
        return True, ""

    def _geometry(self, f, reason, label, surfaces):
        v = self.verdict
        if not v.geometry_replay:
            v.refuse(f, f"{label} without a --geometry replay (the flat floor cannot reproduce it): {reason}")
            return False
        if v.typed:
            static, why_not = self._hosts_static(f, surfaces)
            if not static:
                v.refuse(f, f"{label}: {why_not}")
                return False
            v.accept(f, label, "static map geometry replayed with --geometry; the surface host is a static collision part")
        else:
            v.accept(f, label, "map geometry replayed with --geometry; the surface hosts are not in this probe log (older build) "
                               "and were not verified here")
        return True

    def _floor_height(self, f, reason):
        v = self.verdict
        row, prev = self._row(f), self._row(f - 1)
        if self._geysers.floor(f, self._hosts.get(f, {}).get("ground"), self._start):
            v.accept(f, "exported geyser Top floor", "raw KCL follows verified source phase and original joint animation")
            return
        landing = prev is not None and not prev.get("floor_y") and bool(row and row.get("floor_y"))
        if landing:
            if v.typed:
                static, why_not = self._hosts_static(f, ("ground",))
                if not static:
                    v.refuse(f, f"landing floor: {why_not}")
                    return
            v.accept(f, "floor height change at a landing",
                     "the previous floor sample is older than the previous frame (an airborne gap): the entry is the intended drop's landing, "
                     "not a moving floor")
            return
        tilt, tilt_prev = self._normal_off_vertical(f), self._normal_off_vertical(f - 1)
        flat = tilt is not None and tilt <= FLAT_NORMAL_DEG and tilt_prev is not None and tilt_prev <= FLAT_NORMAL_DEG
        if prev is not None and flat:
            dx = number(row.get("px"), 0) - number(prev.get("px"), 0)
            dz = number(row.get("pz"), 0) - number(prev.get("pz"), 0)
            dy = number(row.get("floor_y"), 0) - number(prev.get("floor_y"), 0)
            if math.hypot(dx, dz) < 0.01 and abs(dy) > FLOOR_PLANE_TOLERANCE:
                v.refuse(f, f"floor under a stationary player changed height by {dy:+.2f} cm (a moving floor): {reason}")
                return
        self._geometry(f, reason, "floor height change on consecutive grounded frames", ("ground",))

    def _displacement(self, f, reason):
        """Grounded displacement differing from velocity: allowed only where the geometry explains it."""
        v = self.verdict
        # The recorder's legacy env_report measured row 0 from the teleport origin,
        # although FIDP samples -N..-1 are movement updates after that teleport.
        # Re-evaluate that one listed check against its actual immediate predecessor.
        if f == 0 and -1 in self.preroll:
            row, prev = self.rows[f], self.preroll[-1]
            values = [number(r.get(k)) for r, k in
                      ((row, "px"), (row, "pz"), (prev, "px"), (prev, "pz"), (row, "vx"), (row, "vz"))]
            if None not in values and all(math.isfinite(x) for x in values):
                px, pz, qx, qz, vx, vz = values
                residual = math.hypot(px - qx - vx, pz - qz - vz)
                if residual <= GROUND_RESIDUAL:
                    v.accept(f, "frame-zero displacement from preroll",
                             "the immediate frame -1 sample explains the legacy origin-based displacement flag")
                    return
        if self._geysers.carries(f, self._hosts.get(f, {}).get("ground"), self._start, self.rows, GROUND_RESIDUAL):
            v.accept(f, "exported geyser platform carry", "verified Top matrix motion carries the grounded player")
            return
        flags = self._flags(f)
        tilt = self._normal_off_vertical(f)
        state = self._state(f)
        explained = (
            (tilt is not None and tilt > FLAT_NORMAL_DEG) or flags.get("wall") == "1" or flags.get("ceiling") == "1"
            or state.startswith(SURFACE_STATES) or self._transition(f)
        )
        if not explained:
            v.refuse(f, f"displacement differs from velocity on flat ground with no wall, surface state or transition: {reason}")
            return
        if self._unscripted(f):
            v.refuse(f, f"displacement in unscripted state {state}: {reason}")
            return
        self._geometry(f, reason, "displacement against the map geometry", ("ground",))


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def in_death_area(area, p):
    """AreaShapeCube::isInVolume / calcLocalPos, with no boundary margin.

    Linked/switch-controlled areas and other shapes need further evidence; fail closed.
    """
    spans = {"AreaCubeBase": (0, 1000), "AreaCubeTop": (-1000, 0), "AreaCubeCenter": (-500, 500)}
    if area.get("name") != "DeathArea" or area.get("shape") not in spans or area.get("links") or area.get("args"):
        return False
    m, scale = area.get("matrix", []), area.get("scale", [])
    if len(m) != 12 or len(scale) != 3 or any(abs(f32(s)) <= f32(0.001) for s in scale):
        return False  # the game's isNearZero scale guard, not a fitted tolerance
    delta = [f32(p[i] - f32(m[i * 4 + 3])) for i in range(3)]
    local = [f32(f32(f32(f32(delta[0] * f32(m[i])) + f32(delta[1] * f32(m[i + 4])))
                           + f32(delta[2] * f32(m[i + 8]))) / f32(scale[i])) for i in range(3)]
    bottom, top = spans[area["shape"]]
    return all(math.isfinite(x) for x in local) and -500 <= local[0] <= 500 and bottom <= local[1] <= top and -500 <= local[2] <= 500


def judge_run(game_rows, rewrite_rows, listed, **evidence):
    """Judge a run. `listed` is {frame: [reason text]} from game.env.json (the player's own cap already discounted)."""
    return Judge(game_rows, rewrite_rows, listed, **evidence).judge()


def _check_candlestands():
    import copy
    part = dict(id="candle", model="LavaWorldCandlestand000", static=True,
                translate=[0, -200, 0], rotate=[0, 0, 0], scale=[1, 1, 1])
    candle = dict(id="candle", host="Candle", is_burn_out=False, settled=True,
                  fire_position=[0, 0, 0], body_position=[0, 10, 0])
    mesh = dict(objects=[part], attributes=[], triangles=[], candlestands=[candle])
    fire = "k=r;w=player;s=Foot:3;o=Fire:10;h=Candle;c=Candlestand;m=al::SensorMsgEnemyAttackFire;r=1"
    forward = fire.replace("k=r", "k=p").replace("r=1", "r=0")
    physical = "k=a;w=player;s=Foot:3;o=Body:12;h=Candle;c=Candlestand;m=;r="
    game = {i: dict(px="-60", py="-40", pz="0", state="Wait/Wait" if i == 0 else "DamageFire/Jump",
                    collision="ground=1;gcode=Ground", contacts=physical if i == 0 else forward + "|" + fire if i == 1 else "")
            for i in range(3)}
    evidence = dict(geometry=mesh, start=[0, 0, 0], preroll={-1: game[0]})
    listed = {1: ["hit-sensor contact Candle"], 2: ["hit-sensor contact Candle"]}
    assert not judge_run(game, game, listed, **evidence).refused
    assert judge_run(game, game, listed).refused
    assert judge_run(game, {0: {}}, listed, **evidence).refused
    for key, value in (("settled", False), ("is_burn_out", True), ("fire_position", [1, 0, 0])):
        unproved = copy.deepcopy(mesh)
        unproved["candlestands"][0][key] = value
        assert judge_run(game, game, listed, **dict(evidence, geometry=unproved)).refused
    for key, value in (("static", False), ("id", "other"), ("model", "Unknown")):
        unproved = copy.deepcopy(mesh)
        unproved["objects"][0][key] = value
        assert judge_run(game, game, listed, **dict(evidence, geometry=unproved)).refused
    for invalid in (fire.replace("Foot:3", "Foot:2"), fire.replace("w=player", "w=cap"),
                    fire.replace("h=Candle", "h=Other"), fire.replace("c=Candlestand", "c=Enemy"),
                    fire.replace("Fire:10", "Wick:12"), fire.replace("EnemyAttackFire", "PushToPlayer"),
                    fire.replace("k=r", "k=p"), fire.replace("r=1", "r=0")):
        unproved = {i: dict(row, contacts=invalid if i == 1 else row["contacts"]) for i, row in game.items()}
        assert judge_run(unproved, game, listed, **evidence).refused
    tangent = {i: dict(row, px="-80") for i, row in game.items()}
    assert judge_run(tangent, game, listed, **dict(evidence, preroll={-1: tangent[0]})).refused


def self_check():
    """Synthetic runs for every branch of the rule."""
    from sand_geyser import self_check as check_geysers
    check_geysers()
    _check_candlestands()

    def rows(count=6, **overrides):
        out = OrderedDict()
        for i in range(count):
            out[i] = {
                "frame": str(i), "px": "0", "py": "0", "pz": str(i * 5), "vx": "0", "vy": "0", "vz": "5", "ground": "1",
                "state": "Run/Run", "triggers": "collision=0;attack=0;action=0;receive=0", "floor_y": "0",
                "collision": "ground=1;wall=0;ceiling=0", "floor_normal": "0/1/0", "contacts": "", "hit_hosts": "",
            }
            out[i].update({k: (v(i) if callable(v) else v) for k, v in overrides.items()})
        return out

    geometry = rows(collision="ground=1;wall=0;ceiling=0;gmat=A;gcode=Ground")
    flat = rows(collision="ground=1;wall=0")
    static_floor = "ground=Tower001/Collision/15/1/0/0/0/0"
    eye = "k=a;w=player;s=Eye:5;o=Body:6;h=%E9%80%83%E3%81%92%E3%82%8BNpc;c=RunAwayNpc;m=;r="
    hanger = "k=a;w=player;s=Body:1;o=Body:12;h=帽子ひっかけポイント;c=CapHanger;m=;r="
    npc_body = "k=a;w=player;s=Body:1;o=Body:6;h=逃げるNpc;c=RunAwayNpc;m=;r="
    push = "k=p;w=player;s=Body:1;o=Body:6;h=逃げるNpc;c=RunAwayNpc;m=rs::SensorMsgPushToPlayer;r=1"
    ignored = "k=r;w=player;s=Body:1;o=Body:12;h=帽子ひっかけポイント;c=CapHanger;m=al::SensorMsgPlayerDisregard;r=0"

    def verdict(game, rewrite, listed):
        return judge_run(game, rewrite, listed)

    # eye overlaps are accepted and never listed as contamination; a typed log names them
    g = rows(contacts=eye, hit_hosts=static_floor)
    v = verdict(g, flat, {})
    assert not v.refused and "逃げるNpc" in v.eye_overlaps, (v.refused, v.eye_overlaps)
    # a physical overlap with a host that ignores Mario: accepted without a consequence marker, refused with one
    g = rows(contacts=hanger, hit_hosts=static_floor)
    assert not verdict(g, flat, {}).refused
    g = rows(contacts=hanger, hit_hosts=static_floor, triggers=lambda i: "collision=0;attack=%d;action=0;receive=0" % (i == 2))
    assert 2 in verdict(g, flat, {}).refused
    # an NPC's body, a message Mario handles: refused; a message he ignores: accepted
    assert verdict(rows(contacts=npc_body, hit_hosts=static_floor), flat, {}).refused.keys() == set(range(6))
    assert verdict(rows(contacts=push, hit_hosts=static_floor), flat, {}).refused
    assert not verdict(rows(contacts=ignored, hit_hosts=static_floor), flat, {}).refused
    # the class decides when the log names it: a harmless-looking name on an NPC class is refused
    assert verdict(rows(contacts=hanger.replace("c=CapHanger", "c=RunAwayNpc"), hit_hosts=static_floor), flat, {}).refused
    # Cappy's contacts are refused
    assert verdict(rows(contacts=hanger.replace("w=player", "w=cap"), hit_hosts=static_floor), flat, {}).refused

    # map geometry: needs the --geometry replay and static hosts
    wall = {2: ["wall contact (1/0/0, Metal)"]}
    g = rows(hit_hosts=lambda i: static_floor + ";wall=Pole000/Collision/15/1/0/0/0/0")
    assert not verdict(g, geometry, wall).refused
    assert 2 in verdict(g, flat, wall).refused, "no geometry replay"
    moving = rows(hit_hosts=lambda i: static_floor + ";wall=Lift/Collision/15/1/1/0/0/0")
    assert 2 in verdict(moving, geometry, wall).refused, "moving host"
    shifting = rows(hit_hosts=lambda i: f"ground=Lift/Collision/15/1/0/0/{i}/0")
    assert 2 in verdict(shifting, geometry, {2: ["floor material changed A -> B"]}).refused, "host that changes position"
    # `floor height changed`: the landing of a drop is accepted, a change under a stationary player is not
    land = rows(contacts="", hit_hosts=static_floor, floor_y=lambda i: "" if i < 3 else "5")
    assert not verdict(land, geometry, {3: ["floor height changed +5.00 cm"]}).refused
    moved = rows(hit_hosts=static_floor, pz="0", vz="0", floor_y=lambda i: "0" if i < 3 else "5")
    assert 3 in verdict(moved, geometry, {3: ["floor height changed +5.00 cm"]}).refused
    # displacement differing from velocity on flat ground without a reason is refused
    assert 2 in verdict(rows(hit_hosts=static_floor), geometry,
                        {2: ["horizontal displacement differs from velocity by 3.0 cm (blocked/pushed)"]}).refused
    assert not verdict(rows(hit_hosts=static_floor, floor_normal="0.3/0.9/0"), geometry,
                       {2: ["horizontal displacement differs from velocity by 3.0 cm (blocked/pushed)"]}).refused

    # Frame zero follows a real idle/preroll update, not the teleport origin.
    g = rows(px="-26.169434", pz="0", vx="-2.328990", vz="0", hit_hosts=static_floor)
    first = {0: ["horizontal displacement differs from velocity by 23.8 cm (blocked/pushed)"]}
    pre = {-1: dict(g[0], frame="-1", px="-23.840332")}
    assert not judge_run(g, geometry, first, preroll=pre).refused
    assert 0 in judge_run(g, geometry, first).refused
    assert 0 in judge_run(g, geometry, first, preroll={-2: pre[-1]}).refused
    assert 0 in judge_run(g, geometry, first, preroll={-1: dict(pre[-1], px="-20")}).refused

    # an older probe log names the host only: harmless names pass, the others do not
    old = rows(contacts="", hit_hosts="")
    named = {1: ["hit-sensor contact 帽子のワールドホーム帽子ひっか\ufffd"], 2: ["hit-sensor contact 逃げるNpc"],
             3: ["hit-sensor contact 帽子花"], 4: ["hit-sensor contact 帽子花@cap"], 5: ["hit-sensor contact 反応するオブジェ"]}
    v = verdict(old, flat, named)
    assert set(v.refused) == {2, 4}, v.refused
    assert verdict(old, flat, {1: ["something new"]}).refused
    # states the scenario never scripts refuse a listed frame
    assert 1 in verdict(rows(state="Damage/Wait"), flat, {1: ["hit-sensor contact 帽子花"]}).refused
    # Static damage is scripted by its collision code and exported host, never
    # by a scenario label, the LavaPink material alone, or the eventual death.
    mesh = {"objects": [{"static": True, "translate": [0, 0, 0]}],
            "attributes": [{"FloorCode": "DamageFire", "MaterialCode": "LavaPink"}],
            "triangles": [[0] * 12 + [0, 0, 7, 4]], "areas": []}
    evidence = dict(geometry=mesh, start=[0, 0, 0])
    g = rows(hit_hosts=static_floor,
             state=lambda i: "Wait/Wait" if i == 0 else "DamageFire/Dead" if i >= 4 else "DamageFire/Jump",
             collision="ground=1;wall=0;ceiling=0;gcode=DamageFire;gmat=LavaPink")
    listed = {i: ["floor material changed A -> LavaPink"] for i in range(1, 6)}
    assert not judge_run(g, geometry, listed, **evidence).refused
    assert judge_run(g, flat, listed, **evidence).refused
    assert judge_run(g, geometry, listed).refused
    # checkDamageFromCollision's Needle trigger dispatches to DamageSwim while
    # swimming (executePreMovementNerveChange). No new water/actor cause waiver.
    swim_mesh = dict(mesh, attributes=[{"FloorCode": "Needle", "MaterialCode": "Stone"}])
    swim = rows(hit_hosts=static_floor, state=lambda i: "Swim/SwimMove" if i == 0 else "DamageSwim/DamageSwim",
                collision="ground=1;gcode=Needle;gmat=Stone")
    swim_evidence = dict(geometry=swim_mesh, start=[0, 0, 0])
    assert not judge_run(swim, geometry, listed, **swim_evidence).refused
    for code in ("Ground", "DamageFire", "Poison"):
        unproved = {f: dict(r, collision=f"ground=1;gcode={code};gmat=Stone") for f, r in swim.items()}
        assert judge_run(unproved, geometry, listed, **swim_evidence).refused
    assert judge_run(swim, flat, listed, **swim_evidence).refused
    attacked = {f: dict(r, contacts=push if f == 3 else "") for f, r in swim.items()}
    assert 3 in judge_run(attacked, geometry, listed, **swim_evidence).refused
    bad = rows(hit_hosts=static_floor, state="DamageFire/Jump", collision="ground=1;gcode=Ground;gmat=LavaPink")
    assert judge_run(bad, geometry, listed, **evidence).refused
    mesh["objects"][0]["static"] = False
    assert judge_run(g, geometry, listed, **evidence).refused
    mesh["objects"][0]["static"] = True
    moving = {f: dict(r, hit_hosts=r["hit_hosts"].replace("/15/1/0/", "/15/1/1/")) for f, r in g.items()}
    assert judge_run(moving, geometry, listed, **evidence).refused
    # A fire cause does not waive enemy messages, unknown coins (even after
    # death), or the static SignBoardDanger obstacle's physical sensor.
    for contact in (push, "k=a;w=player;s=Body:1;o=Body:12;h=Coin;c=Coin;m=;r=",
                    "k=a;w=player;s=Body:1;o=Cap:6;h=Sign;c=SignBoardDanger;m=;r="):
        touched = {f: dict(r, contacts=contact if f == 5 else "") for f, r in g.items()}
        assert 5 in judge_run(touched, geometry, listed, **evidence).refused
    # Only explicit origin + positive actor identity during a proved Dead state
    # exempts a player attack overlap. Legacy/placed/reused coins fail closed.
    coin = "k=a;w=player;s=Body:1;o=Body:12;h=Coin;c=Coin;m=;r=;actor=7;origin=player_death_coin"
    # SandSink is caused by its floor code, including re-entry after Slide.
    # Its child Dead inherits the proof; tagged coins are still Dead-only.
    sand_mesh = dict(mesh, attributes=[{"FloorCode": "SandSink", "MaterialCode": "SandDesert"}])
    sand_evidence = dict(geometry=sand_mesh, start=[0, 0, 0])
    sand = rows(hit_hosts=static_floor,
                state=lambda i: "Run/Run" if i == 0 else "SandSink/Dead" if i >= 4 else "SandSink/Wait",
                collision="ground=1;gcode=SandSink;gmat=SandDesert")
    assert not judge_run(sand, geometry, listed, **sand_evidence).refused
    assert judge_run(sand, flat, listed, **sand_evidence).refused
    assert judge_run(sand, geometry, listed).refused
    for code in ("Ground", "Slide", "DamageFire"):
        unproved = {f: dict(r, collision=f"ground=1;gcode={code};gmat=SandDesert") for f, r in sand.items()}
        assert judge_run(unproved, geometry, listed, **sand_evidence).refused
    for contact in (coin.replace("origin=player_death_coin", "origin=unknown"), push,
                    push.replace("PushToPlayer", "SandGeyserRaise")):
        touched = {f: dict(r, contacts=contact if f == 5 else "") for f, r in sand.items()}
        assert 5 in judge_run(touched, geometry, listed, **sand_evidence).refused
    touched = {f: dict(r, contacts=coin) for f, r in sand.items()}
    assert set(judge_run(touched, geometry, listed, **sand_evidence).refused) == {0, 1, 2, 3}
    moving = {f: dict(r, hit_hosts=r["hit_hosts"].replace("/15/1/0/", "/15/1/1/")) for f, r in sand.items()}
    assert judge_run(moving, geometry, listed, **sand_evidence).refused
    separate_dead = {f: dict(r, state="Dead/Dead" if f >= 4 else r["state"], contacts=coin if f >= 4 else "")
                     for f, r in sand.items()}
    assert not judge_run(separate_dead, geometry, listed, **sand_evidence).refused
    reentry = {f: dict(r, state="Slide/Slide" if f == 3 else r["state"]) for f, r in sand.items()}
    assert not judge_run(reentry, geometry, listed, **sand_evidence).refused
    unproved_reentry = {f: dict(r, collision="ground=1;gcode=Ground;gmat=SandDesert" if f >= 3 else r["collision"])
                        for f, r in reentry.items()}
    assert 4 in judge_run(unproved_reentry, geometry, listed, **sand_evidence).refused
    geyser = {f: dict(r, state="SandGeyser/Wait", contacts=push.replace("PushToPlayer", "SandGeyserRaise"))
              for f, r in sand.items()}
    assert judge_run(geyser, geometry, listed, **sand_evidence).refused
    touched = {f: dict(r, contacts=coin) for f, r in g.items()}
    v = judge_run(touched, geometry, listed, **evidence)
    assert set(v.refused) == {0, 1, 2, 3}, v.refused
    assert v.accepted["player death-drop coin"]["frames"] == {4, 5}
    for invalid in (coin.replace(";origin=player_death_coin", ""),
                    coin.replace(";actor=7", ""), coin.replace("actor=7", "actor=0"),
                    coin.replace("actor=7", "actor=-1"), coin.replace("actor=7", "actor=bad"),
                    coin.replace("origin=player_death_coin", "origin=unknown"),
                    coin.replace("c=Coin;", "c=RunAwayNpc;"), coin.replace("w=player", "w=cap"),
                    coin.replace("k=a", "k=p").replace("m=;r=", "m=rs::SensorMsgPushToPlayer;r=1")):
        touched = {f: dict(r, contacts=invalid if f == 5 else "") for f, r in g.items()}
        assert 5 in judge_run(touched, geometry, listed, **evidence).refused, invalid
    touched = {f: dict(r, contacts=coin if f == 5 else "") for f, r in g.items()}
    assert 5 in judge_run(touched, geometry, listed).refused, "unproved damage cause"
    reused = {f: dict(r, contacts=coin if f == 4 else coin.replace("origin=player_death_coin", "origin=unknown"))
              for f, r in g.items()}
    assert 5 in judge_run(reused, geometry, listed, **evidence).refused, "origin cleared on actor reuse"
    ordinary = coin.replace("actor=7;origin=player_death_coin", "actor=8;origin=unknown")
    mixed = {f: dict(r, contacts=coin + "|" + ordinary if f == 5 else "") for f, r in g.items()}
    assert 5 in judge_run(mixed, geometry, listed, **evidence).refused, "same-name ordinary coin still refused"
    respawn = {f: dict(r, state="Run/Run" if f == 5 else r["state"], contacts=coin if f == 5 else "")
               for f, r in g.items()}
    assert 5 in judge_run(respawn, geometry, {}, **evidence).refused, "no waiver after leaving Dead"
    # Direct-start damage may begin in preroll, whose missing hosts require
    # exact start triangle identity plus vertices at the export's precision.
    pre = {-2: dict(g[0], hit_hosts=""), -1: dict(g[1], hit_hosts="", collision="ground=0")}
    direct = rows(state="DamageFire/Jump", collision="ground=0")
    initial = dict(found=True, vertices=[[0, 0, 0]] * 3, kcl_triangle_index=7, kcl_attribute_index=4)
    v = judge_run(direct, geometry, {1: ["hit-sensor contact 帽子花"]}, preroll=pre, start_triangle=initial, **evidence)
    assert not v.refused and 1 in v.accepted["scripted damage/death"]["frames"]
    initial["kcl_triangle_index"] = 8
    assert judge_run(direct, geometry, {1: ["hit-sensor contact 帽子花"]}, preroll=pre, start_triangle=initial, **evidence).refused
    area = dict(name="DeathArea", id="pit", shape="AreaCubeTop", scale=[1, 1, 1],
                matrix=[1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0], args={}, links=[])
    mesh["areas"] = [area]
    abyss = rows(state="Abyss/Fall", py="-10", hit_hosts=static_floor)
    assert not judge_run(abyss, geometry, {1: ["hit-sensor contact 帽子花"]}, **evidence).refused
    assert not in_death_area(area, [0, 0.000001, 0])
    assert in_death_area(area, [500, -1000, 500])
    assert not in_death_area(dict(area, links=["SwitchAppear"]), [0, -10, 0])
    assert not in_death_area(dict(area, shape="Unknown"), [0, -10, 0])
    assert ranges([1, 2, 3, 5, 7, 8]) == "1-3,5,7-8"
