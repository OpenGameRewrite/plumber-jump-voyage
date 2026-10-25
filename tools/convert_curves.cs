// The skeletal curves of the clips, exactly as the archive stores them, for a game that
// evaluates them between frames (docs/asset-format.md, "Animation curves").
using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Collections.Generic;
using Bfres.Structs;
using Toolbox.Library;
using Toolbox.Library.Animations;
using NSW = Syroot.NintenTools.NSW.Bfres;

partial class ConvertOutfit {
    sealed partial class Export {
        // Scale x y z, rotation x y z w, translation x y z: the order of a record's channels.
        const int ChannelCount = 10;
        static Animation.KeyGroup[] Channels(Animation.KeyNode node) {
            return new[] {node.XSCA,node.YSCA,node.ZSCA,node.XROT,node.YROT,node.ZROT,node.WROT,node.XPOS,node.YPOS,node.ZPOS};
        }
        // Coefficient j of key k in the units of the value: 4-byte keys are used as they are,
        // the 16 and 8 bit ones are scaled, and the first one also offset.
        static float Coefficient(NSW.AnimCurve curve, int key, int j) {
            if(curve.KeyType==NSW.AnimCurveKeyType.Single)return curve.Keys[key,j];
            return curve.Keys[key,j]*curve.Scale+(j==0?curve.Offset.Single:0f);
        }
        static NSW.AnimCurve CurveOf(Animation.KeyGroup group, NSW.BoneAnim bone) {
            return group.AnimDataOffset==0?null:bone.Curves.FirstOrDefault(c=>c.AnimDataOffset==group.AnimDataOffset);
        }
        // The value the archive's cubic polynomials give at a frame: the key at or before it,
        // with the time inside the segment normalised to 0..1; clamped to the first and last key.
        static float Evaluate(NSW.AnimCurve curve, float frame) {
            int last=curve.Frames.Length-1;
            if(frame<=curve.Frames[0])return Coefficient(curve,0,0);
            if(frame>=curve.Frames[last])return Coefficient(curve,last,0);
            int i=0;while(i+1<last&&curve.Frames[i+1]<=frame)i++;
            float t=(frame-curve.Frames[i])/(curve.Frames[i+1]-curve.Frames[i]);
            return ((Coefficient(curve,i,3)*t+Coefficient(curve,i,2))*t+Coefficient(curve,i,1))*t+Coefficient(curve,i,0);
        }
        static void Text(BinaryWriter w, string text) {
            var bytes=Encoding.UTF8.GetBytes(text);
            w.Write((ushort)bytes.Length);w.Write(bytes);
        }
        // Layout (little endian): "PJVCURV1"; u32 bones; per bone: name, i32 parent, u8 segment scale
        // compensated, f32 translation[3], rotation[4] (x y z w), scale[3] of the bind pose;
        // u32 clips; per clip: name, f32 frames, u32 records; per record: u32 bone, u8 rotation kind
        // (0 Euler XYZ in radians, 1 quaternion), then the 10 channels: u32 keys (0: a constant,
        // then f32 value) or per key f32 frame and f32 c0..c3 (value = ((c3 t + c2) t + c1) t + c0).
        void WriteCurves() {
            var partNames=new HashSet<string>(partBones.Select(x=>x.Name));
            model.Skeleton.reset();
            var skeleton=model.Skeleton.bones;
            var bones=skeleton.Where(b=>!partNames.Contains(b.Text)).ToList();
            var index=new Dictionary<string,int>();
            for(int i=0;i<bones.Count;i++)if(!index.ContainsKey(bones[i].Text))index[bones[i].Text]=i;
            var clips=ChosenClips(SkeletalClips()).ToList();
            var check=new ToolCheck();
            using(var w=new BinaryWriter(File.Create(dst))) {
                w.Write(Encoding.ASCII.GetBytes("PJVCURV1"));
                w.Write((uint)bones.Count);
                foreach(var bone in bones) {
                    Text(w,bone.Text);
                    int parent=bone.parentIndex>=0&&index.ContainsKey(skeleton[bone.parentIndex].Text)?index[skeleton[bone.parentIndex].Text]:-1;
                    w.Write(parent);
                    w.Write((byte)(bone.UseSegmentScaleCompensate&&bone.parentIndex>=0?1:0));
                    var p=bone.GetPosition();var q=bone.rot;var s=bone.GetScale();
                    foreach(var v in new[]{p.X,p.Y,p.Z,q.X,q.Y,q.Z,q.W,s.X,s.Y,s.Z})w.Write(v);
                }
                w.Write((uint)clips.Count);
                foreach(var clip in clips) {
                    clip.OpenAnimationData();
                    var records=new List<KeyValuePair<int,Animation.KeyNode>>();
                    foreach(var node in clip.Bones.Where(n=>index.ContainsKey(n.Text)&&clip.SkeletalAnim.BoneAnims.Any(b=>b.Name==n.Text)))
                        records.Add(new KeyValuePair<int,Animation.KeyNode>(index[node.Text],node));
                    Text(w,clip.Text);w.Write((float)clip.FrameCount);w.Write((uint)records.Count);
                    foreach(var record in records) {
                        var node=record.Value;
                        var boneAnim=clip.SkeletalAnim.BoneAnims.First(b=>b.Name==node.Text);
                        w.Write((uint)record.Key);
                        w.Write((byte)(node.RotType==Animation.RotationType.QUATERNION?1:0));
                        var groups=Channels(node);
                        foreach(var group in groups) {
                            var curve=CurveOf(group,boneAnim);
                            if(curve!=null)check.Continuity(clip,node.Text,curve);
                            if(curve==null) {w.Write(0u);w.Write(group.Keys.Count>0?group.Keys[0].Value:0f);continue;}
                            w.Write((uint)curve.Frames.Length);
                            for(int k=0;k<curve.Frames.Length;k++) {
                                w.Write(curve.Frames[k]);
                                for(int j=0;j<4;j++)w.Write(Coefficient(curve,k,j));
                            }
                        }
                        check.Compare(clip,node,boneAnim,groups);
                    }
                }
            }
            Console.WriteLine("Curves: "+bones.Count+" bones, "+clips.Count+" clips");
            check.Report();
        }

        // Compares the curves' own evaluation with the installed tool's, on whole frames and between them.
        sealed class ToolCheck {
            readonly Dictionary<string,int> over=new Dictionary<string,int>();readonly float[] worst=new float[6];readonly string[] at=new string[6];
            static string[] labels={"translation","scale","rotation"};
            long segments;float worstJump;string jumpAt="";
            // A cubic segment must end where the next key starts (c0+c1+c2+c3 = next c0); the
            // 16 and 8 bit keys leave a few quanta of rounding.
            public void Continuity(FSKA clip, string bone, NSW.AnimCurve curve) {
                for(int k=0;k+1<curve.Frames.Length;k++) {
                    float end=Coefficient(curve,k,0)+Coefficient(curve,k,1)+Coefficient(curve,k,2)+Coefficient(curve,k,3);
                    float quanta=Math.Abs(end-Coefficient(curve,k+1,0))/(curve.KeyType==NSW.AnimCurveKeyType.Single?1e-6f:Math.Abs(curve.Scale));
                    segments++;
                    if(quanta>worstJump){worstJump=quanta;jumpAt=clip.Text+"/"+bone+" key "+k;}
                }
            }
            public void Compare(FSKA clip, Animation.KeyNode node, NSW.BoneAnim boneAnim, Animation.KeyGroup[] groups) {
                Func<float,int,float> value=(frame,c)=>{var curve=CurveOf(groups[c],boneAnim);return curve==null?(groups[c].Keys.Count>0?groups[c].Keys[0].Value:0f):Evaluate(curve,frame);};
                for(int whole=0;whole<2;whole++)
                    for(int frame=0;frame<=clip.FrameCount;frame++) {
                        float time=frame+(whole==0?0f:0.37f);
                        if(time>clip.FrameCount)continue;
                        var position=node.GetPosition(time);var scale=node.GetScale(time);
                        float dp=Math.Max(Math.Abs(position.X-value(time,7)),Math.Max(Math.Abs(position.Y-value(time,8)),Math.Abs(position.Z-value(time,9))));
                        float ds=Math.Max(Math.Abs(scale.X-value(time,0)),Math.Max(Math.Abs(scale.Y-value(time,1)),Math.Abs(scale.Z-value(time,2))));
                        float dr=0;
                        if(node.RotType!=Animation.RotationType.QUATERNION) {
                            // (The tool keeps the Euler angles of such a node in the xyz of its "rotation".)
                            var tool=node.GetRotation(time);
                            dr=Math.Max(Math.Abs(tool.X-value(time,3)),Math.Max(Math.Abs(tool.Y-value(time,4)),Math.Abs(tool.Z-value(time,5))));
                        }
                        if(dp>0.001f)Count(whole+" translation "+node.Text,clip.Text+"@"+time);
                        if(ds>0.0001f)Count(whole+" scale "+node.Text,clip.Text+"@"+time);
                        if(dr>0.0001f)Count(whole+" rotation "+node.Text,clip.Text+"@"+time);
                        Note(whole*3,dp,clip,node,time);Note(whole*3+1,ds,clip,node,time);Note(whole*3+2,dr,clip,node,time);
                    }
            }
            readonly Dictionary<string,string> firsts=new Dictionary<string,string>();
            void Count(string key, string example) {int n;over.TryGetValue(key,out n);over[key]=n+1;if(n==0)firsts[key]=example;}
            void Note(int slot, float difference, FSKA clip, Animation.KeyNode node, float time) {
                if(difference>worst[slot]){worst[slot]=difference;at[slot]=clip.Text+"/"+node.Text+"@"+time;}
            }
            public void Report() {
                Console.WriteLine("Segments: "+segments+", largest jump between a segment's end and the next key: "+worstJump+" quanta ("+jumpAt+")");
                for(int whole=0;whole<2;whole++)
                    Console.WriteLine("Largest difference from the installed tool's evaluation, "+(whole==0?"whole frames":"between frames")+": "+
                        string.Join(", ",Enumerable.Range(0,3).Select(i=>labels[i]+" "+worst[whole*3+i]+" ("+at[whole*3+i]+")")));
            }
        }
    }
}
