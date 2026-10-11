// Original CLI adapter to a separately installed Switch Toolbox / AssimpNet.
// No tool binaries or game data are distributed with this source.
using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Windows.Forms;
using FirstPlugin;
using Bfres.Structs;
using Toolbox.Library;
using Toolbox.Library.Animations;
using Assimp;

class ConvertOutfit {
    static TextureWrapMode Wrap(STTextureWrapMode mode) {
        return mode==STTextureWrapMode.Clamp?TextureWrapMode.Clamp:mode==STTextureWrapMode.Mirror?TextureWrapMode.Mirror:TextureWrapMode.Wrap;
    }
    static byte NormalBlue(byte red,byte green) {
        double x=red/127.5-1,y=green/127.5-1;
        return (byte)Math.Round(127.5*(1+Math.Sqrt(Math.Max(0,1-x*x-y*y))));
    }
    static bool IsRoot(string name) {return name=="JointRoot"||name=="AllRoot"||name=="nw4f_root";}
    // A few outfits (the voxel figure) bind the whole body rigidly to the
    // root in the original data. Keep such a figure in one piece: its head and
    // hands follow the root too, instead of floating off on their own bones.
    static bool RigidFigure(FMDL model) {
        var bones=model.Skeleton.bones;
        return model.Objects.Any()&&model.Objects.All(shape=>shape.vertices.All(v=>{
            if(v.boneIds.Count>1)return false;
            int i=v.boneIds.Count==0||v.boneIds[0]<0?shape.BoneIndex:model.Skeleton.Node_Array[v.boneIds[0]];
            return i>=0&&IsRoot(bones[i].Text);
        }));
    }
    class PartBone {public string Name,Set;public FMDL Part;public STBone Bone;public bool Chained;public OpenTK.Matrix4 Fix;}
    static string PartSet(string file) {
        return file.Contains("HandL")?"L":file.Contains("HandR")?"R":file.EndsWith("Eye")||file.EndsWith("EyeRsv")?"Eye":file.EndsWith("Face")||file.EndsWith("FaceRsv")?"Face":file.EndsWith("Head")||file.EndsWith("HeadRsv")?"Head":null;
    }
    static string PartName(string set,string name) {return set=="L"||set=="R"?name:set+"_"+name;}
    static IEnumerable<T> Animations<T>(TreeNode node) where T:TreeNode {
        var animation = node as T;
        if (animation != null) yield return animation;
        foreach (TreeNode child in node.Nodes)
            foreach (var a in Animations<T>(child)) yield return a;
    }
    [STAThread] static int Main(string[] args) {
        try {
            if(args.Contains("--self-check")) {
                if(Wrap(STTextureWrapMode.Clamp)!=TextureWrapMode.Clamp||Wrap(STTextureWrapMode.Mirror)!=TextureWrapMode.Mirror||Wrap(STTextureWrapMode.Repeat)!=TextureWrapMode.Wrap)
                    throw new Exception("Texture wrapping failed");
                if(NormalBlue(128,128)!=255||NormalBlue(255,255)!=128)throw new Exception("Normal reconstruction failed");
                if(PartSet("MarioEye")!="Eye"||PartSet("MarioFaceRsv")!="Face"||PartSet("MarioAlohaHead")!="Head"
                    ||PartName("Eye","Eyeball")==PartName("Face","Eyeball")||PartName("L","HandL")!="HandL")
                    throw new Exception("Part rig naming failed");
                Console.WriteLine("Normal map and part rig checks passed");return 0;
            }
            if (args.Length < 3) { Console.Error.WriteLine("Usage: convert.exe model.bfres output.glb animations.bfres [more-animation-files]"); return 2; }
            var modelPath=Path.GetFullPath(args[0]);
            // The library expects an MDI host even for CPU-only parsing; never show it.
            Toolbox.Library.Runtime.MainForm=new Form {IsMdiContainer=true,ShowInTaskbar=false};
            Toolbox.Library.Runtime.ExecutableDir=AppDomain.CurrentDomain.BaseDirectory;
            Toolbox.Library.Runtime.UseDirectXTexDecoder=false;
            GenericPluginLoader.LoadPlugin();
            var dst=Path.GetFullPath(args[1]);
            Directory.CreateDirectory(Path.GetDirectoryName(dst));
            var resource=new BFRES {FileName=Path.GetFileName(modelPath),FilePath=modelPath};
            using (var f=File.OpenRead(modelPath)) resource.Load(f);
            var models=resource.ExportableModels.OfType<FMDL>().ToArray();
            if(models.Length==0) throw new InvalidDataException("BFRES has no exportable model");
            // Export the model through the installed tool's standard interchange exporter.
            var model=models[0];var dae=Path.ChangeExtension(dst,"dae");
            bool rigidFigure=RigidFigure(model);
            if(rigidFigure)Console.WriteLine("Rigid figure: parts follow the root");
            var meshList=model.Objects.ToList();var materials=model.Materials.ToList();var textures=resource.GetTextures();
            // Materials per part file, so a part's texture archive binds only to that part.
            var partMaterials=new Dictionary<string,List<FMAT>>();
            // Keep the independent finger, face, eye and head rigs. Each action
            // samples its matching part clips alongside the body clip.
            var partBones=new List<PartBone>();
            var partClips=new Dictionary<string,Dictionary<string,FSKA>>();
            var visibility=new Dictionary<string,Dictionary<string,bool[]>>();
            var visibilityLoops=new Dictionary<string,bool>();
            var pupilUv=new Dictionary<string,Dictionary<string,float[][]>>();
            foreach(var arg in args.Where(a=>a.StartsWith("--hand-l=")||a.StartsWith("--hand-r=")||a.StartsWith("--eye=")||a.StartsWith("--face=")||a.StartsWith("--head="))) {
                int separator=arg.IndexOf('=');var path=arg.Substring(separator+1);
                string set=arg.StartsWith("--hand-")?arg.Substring(7,1).ToUpper():arg.Substring(2,separator-2);
                set=char.ToUpper(set[0])+set.Substring(1);
                var r=new BFRES {FileName=Path.GetFileName(path),FilePath=Path.GetFullPath(path)};
                using(var f=File.OpenRead(path))r.Load(f);
                partClips[set]=Animations<FSKA>(r).GroupBy(x=>x.Text).ToDictionary(g=>g.Key,g=>g.First());
                foreach(var v in Animations<FVIS>(r)) {
                    v.OpenAnimationData();
                    var raw=v.VisibilityAnim;
                    if(!visibility.ContainsKey(v.Text))visibility[v.Text]=new Dictionary<string,bool[]>();
                    visibilityLoops[v.Text]=raw.Loop;
                    for(int i=0;i<raw.Names.Count;i++) {
                        var curve=v.Values.FirstOrDefault(g=>g.AnimDataOffset==i);
                        visibility[v.Text][raw.Names[i]]=Enumerable.Range(0,raw.FrameCount+1)
                            .Select(frame=>curve==null?raw.BaseDataList[i]:curve.GetValue(frame)).ToArray();
                    }
                }
                // Eye-pupil SRT tracks use mode 0, rotation and UV offsets;
                // their scale stays at one. Keep them alongside mesh visibility.
                foreach(var a in Animations<FMAA>(r).Where(a=>set=="Eye"&&a.Text.EndsWith("_fts"))) {
                    a.OpenAnimationData();var name=a.Text.Substring(0,a.Text.Length-4);
                    if(!visibility.ContainsKey(name))visibility[name]=new Dictionary<string,bool[]>();
                    visibilityLoops[name]=a.CanLoop;
                    pupilUv[name]=new Dictionary<string,float[][]>();
                    foreach(var material in a.Materials.Where(m=>m.Text.StartsWith("EyePupil"))) {
                        var param=material.Params.FirstOrDefault(p=>p.Text=="tex_mtx0");
                        if(param==null)continue;
                        if(param.Values.Any(v=>v.AnimDataOffset!=0&&v.AnimDataOffset!=12&&v.AnimDataOffset!=16&&v.AnimDataOffset!=20))
                            throw new InvalidDataException("Unsupported pupil SRT scale in "+a.Text);
                        pupilUv[name][material.Text]=Enumerable.Range(0,a.FrameCount+1).Select(frame=>
                            new uint[]{12,16,20}.Select(offset=>{
                                var curve=param.Values.FirstOrDefault(v=>v.AnimDataOffset==offset);
                                return curve==null?0:curve.GetValue(frame);
                            }).ToArray()).ToArray();
                    }
                }
            }
            var expressions=visibility.ToDictionary(k=>k.Key,k=>(object)new {looping=visibilityLoops[k.Key],tracks=k.Value,
                uv=pupilUv.ContainsKey(k.Key)?pupilUv[k.Key]:new Dictionary<string,float[][]>()});
            File.WriteAllText(Path.ChangeExtension(dst,"expressions.json"),new System.Web.Script.Serialization.JavaScriptSerializer {MaxJsonLength=int.MaxValue}.Serialize(expressions));
            if(args.Contains("--expressions-only"))return 0;
            foreach(var part in args.Skip(2).Where(a=>a.StartsWith("--part="))) {
                // --part=model.bfres[|Joint|rotX|rotY|rotZ|transX|transY|transZ], from the
                // costume's own placement table (joint plus local rotation in degrees).
                var spec=part.Substring(7).Split('|');var path=spec[0];
                var r=new BFRES {FileName=Path.GetFileName(path),FilePath=Path.GetFullPath(path)};
                using(var f=File.OpenRead(path))r.Load(f);
                var file=Path.GetFileNameWithoutExtension(path);
                var own=r.GetTextures();
                if(!r.ExportableModels.OfType<FMDL>().Any()&&file.EndsWith("Texture")) {
                    // A texture archive (e.g. the 64 gloves) may reuse a body texture's name; the
                    // original binds it to its own part, so keep it apart under a new name.
                    var owner=file.Substring(0,file.Length-"Texture".Length);
                    foreach(var t in own.Where(t=>textures.Any(x=>x.Text==t.Text))) {
                        var renamed=t.Text+"_"+owner;
                        foreach(var m in partMaterials.Where(k=>k.Key.StartsWith(owner)).SelectMany(k=>k.Value))
                            foreach(var map in m.TextureMaps.Where(x=>x.Name==t.Text))map.Name=renamed;
                        Console.WriteLine("Texture "+t.Text+" of "+file+" renamed to "+renamed);
                        t.Text=renamed;
                    }
                }
                textures.AddRange(own);
                // Fallback for parts without a table: the installed tool's placement and a name guess.
                string anchor=file.Contains("HandL")?"HandL":file.Contains("HandR")?"HandR":file.Contains("Under")||file.Contains("Skirt")?"Hip":file.Contains("Shell")||file.Contains("Ruck")?"Spine2":"Head";
                var place=r.BFRESRender.ModelTransform;
                var joint=spec.Length>=8?model.Skeleton.bones.FirstOrDefault(b=>b.Text==spec[1]):null;
                if(joint!=null) {
                    Func<int,float> number=i=>float.Parse(spec[i],System.Globalization.CultureInfo.InvariantCulture);
                    Func<int,float> radians=i=>number(i)*(float)Math.PI/180;
                    model.Skeleton.reset(true);
                    // Row vectors: local rotation X, then Y, then Z, then offset, then the joint's rest matrix.
                    place=OpenTK.Matrix4.CreateRotationX(radians(2))*OpenTK.Matrix4.CreateRotationY(radians(3))*OpenTK.Matrix4.CreateRotationZ(radians(4))
                        *OpenTK.Matrix4.CreateTranslation(number(5),number(6),number(7))*model.Skeleton.GetBoneTransform(joint);
                    anchor=joint.Text;
                }
                foreach(var sub in r.ExportableModels.OfType<FMDL>()) {
                    string set=PartSet(file);bool hand=set=="L"||set=="R";
                    if(set!=null&&!rigidFigure) {
                        sub.Skeleton.reset(true);model.Skeleton.reset(true);
                        foreach(var pb in sub.Skeleton.bones) {
                            string name=PartName(set,pb.Text);
                            if(model.Skeleton.boneIndex(name)>=0||hand&&pb.parentIndex<0)continue;
                            var parentName=pb.parentIndex>=0?PartName(set,sub.Skeleton.bones[pb.parentIndex].Text):null;
                            int parent=parentName==null?-1:model.Skeleton.boneIndex(parentName);
                            bool chained=partBones.Any(x=>x.Name==parentName);
                            if(!hand&&parent<0)parent=model.Skeleton.boneIndex(anchor);
                            if(parent<0)continue;
                            // Independent face/eye/head roots attach to the body head; their
                            // namespaces keep duplicate names (notably Eyeball) separate.
                            var fix=place*model.Skeleton.GetBoneTransform(parent).Inverted();
                            var local=chained?pb.GetTransform():sub.Skeleton.GetBoneTransform(pb)*fix;
                            var nb=new STBone(model.Skeleton) {Text=name,parentIndex=parent,RotationType=STBone.BoneRotationType.Quaternion};
                            nb.FromTransform(local);
                            model.Skeleton.bones.Add(nb);
                            partBones.Add(new PartBone {Name=name,Set=set,Part=sub,Bone=pb,Chained=chained,Fix=fix});
                        }
                        model.Skeleton.reset(true);
                        Console.WriteLine("Part bones from "+file+": "+partBones.Count(x=>x.Part==sub));
                    }
                    // Parts have independent local skeletons. Move their bind geometry
                    // into the body's rest space before sharing its animation skeleton.
                    foreach(var shape in sub.Objects) {
                        if(shape.MaterialIndex>=0)shape.MaterialIndex+=materials.Count;
                        foreach(var v in shape.vertices) {
                            if(v.boneIds.Count==0){v.boneIds.Add(-1);v.boneWeights.Add(1);}
                            while(v.boneWeights.Count<v.boneIds.Count)v.boneWeights.Add(1);
                            var position=OpenTK.Vector3.Zero;var normal=OpenTK.Vector3.Zero;
                            for(int i=0;i<v.boneIds.Count;i++) {
                                int source=v.boneIds[i]<0?shape.BoneIndex:sub.Skeleton.Node_Array[v.boneIds[i]];
                                source=Math.Max(0,source);string name=sub.Skeleton.bones[source].Text;
                                var ownBone=partBones.FirstOrDefault(x=>x.Part==sub&&x.Bone==sub.Skeleton.bones[source]);
                                int destBone=model.Skeleton.bones.FindIndex(b=>b.Text==(ownBone==null?name:ownBone.Name));
                                // Part roots share names with body roots; rigid face pieces belong to the anchor.
                                // A bone the body lacks (coat flap, finger, cape segment) hangs from its
                                // nearest ancestor that the body has, as the original part does.
                                for(int up=sub.Skeleton.bones[source].parentIndex;destBone<0&&up>=0;up=sub.Skeleton.bones[up].parentIndex) {
                                    var parentName=sub.Skeleton.bones[up].Text;
                                    if(parentName=="nw4f_root"||parentName=="AllRoot"||parentName=="JointRoot")break;
                                    destBone=model.Skeleton.bones.FindIndex(b=>b.Text==parentName);
                                }
                                if(destBone<0||ownBone==null&&IsRoot(name))destBone=model.Skeleton.bones.FindIndex(b=>b.Text==anchor);
                                if(rigidFigure)destBone=model.Skeleton.bones.FindIndex(b=>b.Text=="JointRoot");
                                if(destBone<0)throw new InvalidDataException("No attachment bone for "+file);
                                int dest=Array.IndexOf(model.Skeleton.Node_Array,destBone);
                                if(dest<0){dest=model.Skeleton.Node_Array.Length;model.Skeleton.Node_Array=model.Skeleton.Node_Array.Concat(new[]{destBone}).ToArray();}
                                position+=OpenTK.Vector3.TransformPosition(v.pos,place)*v.boneWeights[i];
                                normal+=OpenTK.Vector3.TransformNormal(v.nrm,place)*v.boneWeights[i];
                                v.boneIds[i]=dest;
                            }
                            v.pos=position;v.nrm=normal.LengthSquared>0?normal.Normalized():normal;
                        }
                        shape.VertexSkinCount=(byte)Math.Max(1,(int)shape.VertexSkinCount);
                        if(shape.BoneIndex>=0){var name=sub.Skeleton.bones[shape.BoneIndex].Text;shape.BoneIndex=model.Skeleton.bones.FindIndex(b=>b.Text==name);}
                    }
                    meshList.AddRange(sub.Objects);materials.AddRange(sub.Materials);
                    if(!partMaterials.ContainsKey(file))partMaterials[file]=new List<FMAT>();
                    partMaterials[file].AddRange(sub.Materials.OfType<FMAT>());
                }
            }
            textures=textures.GroupBy(t=>t.Text).Select(g=>g.First()).ToList();
            for(int i=0;i<materials.Count;i++)materials[i].Text="material"+i+"_"+materials[i].Text;
            foreach(var texture in textures) {
                try {using(var bitmap=texture.GetBitmap()) {
                    // BC5 stores only tangent-space X/Y; PNG/glTF needs the reconstructed Z.
                    if(texture.Format.ToString().StartsWith("BC5")&&texture.Text.Contains("_nrm")) {
                        var pixels=bitmap.LockBits(new System.Drawing.Rectangle(0,0,bitmap.Width,bitmap.Height),System.Drawing.Imaging.ImageLockMode.ReadWrite,System.Drawing.Imaging.PixelFormat.Format32bppArgb);
                        var bytes=new byte[Math.Abs(pixels.Stride)*bitmap.Height];System.Runtime.InteropServices.Marshal.Copy(pixels.Scan0,bytes,0,bytes.Length);
                        for(int i=0;i<bytes.Length;i+=4){bytes[i]=NormalBlue(bytes[i+2],bytes[i+1]);bytes[i+3]=255;}
                        System.Runtime.InteropServices.Marshal.Copy(bytes,0,pixels.Scan0,bytes.Length);bitmap.UnlockBits(pixels);
                    }
                    bitmap.Save(Path.Combine(Path.GetDirectoryName(dst),texture.Text+".png"));
                }}
                catch(Exception e) {Console.Error.WriteLine("Texture "+texture.Text+": "+e.GetType().Name+" "+e.Message);}
            }
            var settings=new DAE.ExportSettings {ExportTextures=false,SuppressConfirmDialog=true,OnlyExportRiggedBones=false};
            DAE.Export(dae,settings,meshList,materials,textures,model.Skeleton,model.Skeleton.Node_Array.ToList());
            using(var importer=new AssimpContext()) {
                var scene=importer.ImportFile(dae,PostProcessSteps.Triangulate|PostProcessSteps.GenerateSmoothNormals);
                // The Collada importer drops sampler wrapping. Restore it from
                // each original material before exporting glTF (notably the 64 cap).
                foreach(var material in scene.Materials) {
                    var original=materials.FirstOrDefault(m=>m.Text==material.Name);
                    if(original==null)continue;
                    foreach(var map in original.TextureMaps) {
                        bool diffuse=map.Type==STGenericMatTexture.TextureType.Diffuse;
                        bool normal=map.Type==STGenericMatTexture.TextureType.Normal;
                        if(!(diffuse&&material.HasTextureDiffuse||normal&&material.HasTextureNormal))continue;
                        var slot=diffuse?material.TextureDiffuse:material.TextureNormal;
                        slot.WrapModeU=Wrap(map.WrapModeS);slot.WrapModeV=Wrap(map.WrapModeT);
                        if(diffuse)material.TextureDiffuse=slot;else material.TextureNormal=slot;
                    }
                }
                var all=new List<FSKA>();all.AddRange(Animations<FSKA>(resource));
                foreach(var path in args.Skip(2).Where(a=>!a.StartsWith("--"))) {var r=new BFRES {FileName=Path.GetFileName(path),FilePath=Path.GetFullPath(path)};using(var f=File.OpenRead(path))r.Load(f);all.AddRange(Animations<FSKA>(r));}
                if(args.Contains("--geometry-only"))all.Clear();
                Console.WriteLine("Segment scale compensated bones: "+model.Skeleton.bones.Count(b=>b.UseSegmentScaleCompensate));
                File.WriteAllLines(Path.ChangeExtension(dst,"animations.txt"),all.Select(a=>a.Text+" "+a.FrameCount));
                var prefixes=new[]{"Wait","Run","Dash","Walk","Jump","Fall","Rolling","Roll","Cap","Throw","HipDrop","Squat","Dive","Swim","Wall","Spin","Pole","Climb","Slide","Land","Brake","Turn","HeadSliding"};
                foreach(var clip in all.GroupBy(a=>a.Text).Select(g=>g.First()).Where(a=>args.Contains("--all-animations")||a.FrameCount<=600&&prefixes.Any(p=>a.Text.StartsWith(p,StringComparison.OrdinalIgnoreCase)))) {
                    var anim=new Assimp.Animation {Name=clip.Text,DurationInTicks=clip.FrameCount,TicksPerSecond=60.0};
                    var channels=new Dictionary<string,NodeAnimationChannel>();
                    foreach(var bone in model.Skeleton.bones) {var node=scene.RootNode.FindNode(bone.Text)??scene.RootNode.FindNode("Armature_"+bone.Text);if(node==null)continue;var ch=new NodeAnimationChannel {NodeName=node.Name};channels[bone.Text]=ch;anim.NodeAnimationChannels.Add(ch);}
                    model.Skeleton.reset();
                    foreach(var part in partBones.Select(x=>x.Part).Distinct())part.Skeleton.reset();
                    for(int frame=0;frame<=clip.FrameCount;frame++) {
                        clip.SetFrame(frame);clip.NextFrame(model.Skeleton,false,false);
                        var posed=new Dictionary<string,OpenTK.Matrix4>();
                        foreach(var group in partBones.GroupBy(x=>x.Part)) {
                            var set=group.First().Set;FSKA partClip=null;
                            if(partClips.ContainsKey(set)) {
                                if(!partClips[set].TryGetValue(clip.Text,out partClip))partClips[set].TryGetValue("Wait",out partClip);
                            }
                            if(partClip!=null) {
                                float at=partClip.CanLoop&&partClip.FrameCount>0?frame%partClip.FrameCount:Math.Min(frame,partClip.FrameCount);
                                partClip.SetFrame(at);partClip.NextFrame(group.Key.Skeleton,false,false);
                            }
                            foreach(var x in group) {
                                var b=x.Bone;
                                var localPose=OpenTK.Matrix4.CreateScale(b.sca)*OpenTK.Matrix4.CreateFromQuaternion(b.rot)*OpenTK.Matrix4.CreateTranslation(b.pos);
                                posed[x.Name]=x.Chained?localPose:localPose*(b.parentIndex>=0?group.Key.Skeleton.bones[b.parentIndex].Transform:OpenTK.Matrix4.Identity)*x.Fix;
                            }
                        }
                        foreach(var bone in model.Skeleton.bones) {
                            NodeAnimationChannel ch;if(!channels.TryGetValue(bone.Text,out ch))continue;
                            var p=bone.GetPosition();var s=bone.GetScale();var q=bone.rot;
                            OpenTK.Matrix4 partPose;
                            if(posed.TryGetValue(bone.Text,out partPose)) {
                                p=partPose.ExtractTranslation();s=partPose.ExtractScale();q=partPose.ExtractRotation();
                                ch.PositionKeys.Add(new VectorKey(frame,new Vector3D(p.X,p.Y,p.Z)));
                                ch.ScalingKeys.Add(new VectorKey(frame,new Vector3D(s.X,s.Y,s.Z)));
                                ch.RotationKeys.Add(new QuaternionKey(frame,new Assimp.Quaternion(q.W,q.X,q.Y,q.Z)));
                                continue;
                            }
                            // Odyssey rigs use segment scale compensation: a stretched arm
                            // must not inflate its hand. glTF always inherits parent scale,
                            // so divide it back out (exact for uniform parent scale).
                            var parent=bone.parentIndex>=0?model.Skeleton.bones[bone.parentIndex]:null;
                            if(bone.UseSegmentScaleCompensate&&parent!=null) {
                                var ps=parent.GetScale();
                                if(ps.X!=0&&ps.Y!=0&&ps.Z!=0)s=new OpenTK.Vector3(s.X/ps.X,s.Y/ps.Y,s.Z/ps.Z);
                            }
                            ch.PositionKeys.Add(new VectorKey(frame,new Vector3D(p.X,p.Y,p.Z)));
                            ch.ScalingKeys.Add(new VectorKey(frame,new Vector3D(s.X,s.Y,s.Z)));
                            ch.RotationKeys.Add(new QuaternionKey(frame,new Assimp.Quaternion(q.W,q.X,q.Y,q.Z)));
                        }
                    }
                    scene.Animations.Add(anim);
                }
                if(!importer.ExportFile(scene,Path.ChangeExtension(dst,"gltf"),"gltf2")) throw new IOException("Installed Assimp cannot export glTF 2.0");
                // Assimp 4.1 exports each bone as an unnamed glTF animation.
                // Preserve original clip boundaries so the packer can join those tracks.
                File.WriteAllLines(Path.ChangeExtension(dst,"clips.txt"),scene.Animations.Select(a=>a.Name+"\t"+a.NodeAnimationChannels.Count));
                Console.WriteLine(Path.GetFileName(dst)+": "+scene.MeshCount+" meshes, "+scene.AnimationCount+" animations");
            }
            return 0;
        } catch(Exception e) {Console.Error.WriteLine(e.ToString());return 1;}
    }
}
