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
    static byte NormalBlue(byte red,byte green) {
        double x=red/127.5-1,y=green/127.5-1;
        return (byte)Math.Round(127.5*(1+Math.Sqrt(Math.Max(0,1-x*x-y*y))));
    }
    static IEnumerable<FSKA> Animations(TreeNode node) {
        var animation = node as FSKA;
        if (animation != null) yield return animation;
        foreach (TreeNode child in node.Nodes)
            foreach (var a in Animations(child)) yield return a;
    }
    [STAThread] static int Main(string[] args) {
        try {
            if(args.Contains("--self-check")) {
                if(NormalBlue(128,128)!=255||NormalBlue(255,255)!=128)throw new Exception("Normal reconstruction failed");
                Console.WriteLine("Normal map check passed");return 0;
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
            var meshList=model.Objects.ToList();var materials=model.Materials.ToList();var textures=resource.GetTextures();
            foreach(var part in args.Skip(2).Where(a=>a.StartsWith("--part="))) {
                var path=part.Substring(7);var r=new BFRES {FileName=Path.GetFileName(path),FilePath=Path.GetFullPath(path)};
                using(var f=File.OpenRead(path))r.Load(f);textures.AddRange(r.GetTextures());
                foreach(var sub in r.ExportableModels.OfType<FMDL>()) {
                    // Parts have independent local skeletons. Move their bind geometry
                    // into the body's rest space before sharing its animation skeleton.
                    var file=Path.GetFileNameWithoutExtension(path);
                    string anchor=file.Contains("HandL")?"HandL":file.Contains("HandR")?"HandR":file.Contains("Under")||file.Contains("Skirt")?"Hip":file.Contains("Shell")||file.Contains("Ruck")?"Spine2":"Head";
                    foreach(var shape in sub.Objects) {
                        if(shape.MaterialIndex>=0)shape.MaterialIndex+=materials.Count;
                        foreach(var v in shape.vertices) {
                            if(v.boneIds.Count==0){v.boneIds.Add(-1);v.boneWeights.Add(1);}
                            while(v.boneWeights.Count<v.boneIds.Count)v.boneWeights.Add(1);
                            var position=OpenTK.Vector3.Zero;var normal=OpenTK.Vector3.Zero;
                            for(int i=0;i<v.boneIds.Count;i++) {
                                int source=v.boneIds[i]<0?shape.BoneIndex:sub.Skeleton.Node_Array[v.boneIds[i]];
                                source=Math.Max(0,source);string name=sub.Skeleton.bones[source].Text;
                                int destBone=model.Skeleton.bones.FindIndex(b=>b.Text==name);
                                if(destBone<0||name=="nw4f_root"||name=="AllRoot")destBone=model.Skeleton.bones.FindIndex(b=>b.Text==anchor);
                                if(destBone<0)throw new InvalidDataException("No attachment bone for "+file);
                                int dest=Array.IndexOf(model.Skeleton.Node_Array,destBone);
                                if(dest<0){dest=model.Skeleton.Node_Array.Length;model.Skeleton.Node_Array=model.Skeleton.Node_Array.Concat(new[]{destBone}).ToArray();}
                                // The installed tool supplies Odyssey's part bind placement.
                                var transform=r.BFRESRender.ModelTransform;
                                position+=OpenTK.Vector3.TransformPosition(v.pos,transform)*v.boneWeights[i];
                                normal+=OpenTK.Vector3.TransformNormal(v.nrm,transform)*v.boneWeights[i];
                                v.boneIds[i]=dest;
                            }
                            v.pos=position;v.nrm=normal.LengthSquared>0?normal.Normalized():normal;
                        }
                        shape.VertexSkinCount=(byte)Math.Max(1,(int)shape.VertexSkinCount);
                        if(shape.BoneIndex>=0){var name=sub.Skeleton.bones[shape.BoneIndex].Text;shape.BoneIndex=model.Skeleton.bones.FindIndex(b=>b.Text==name);}
                    }
                    meshList.AddRange(sub.Objects);materials.AddRange(sub.Materials);
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
                var all=new List<FSKA>();all.AddRange(Animations(resource));
                foreach(var path in args.Skip(2).Where(a=>!a.StartsWith("--"))) {var r=new BFRES {FileName=Path.GetFileName(path),FilePath=Path.GetFullPath(path)};using(var f=File.OpenRead(path))r.Load(f);all.AddRange(Animations(r));}
                if(args.Contains("--geometry-only"))all.Clear();
                File.WriteAllLines(Path.ChangeExtension(dst,"animations.txt"),all.Select(a=>a.Text+" "+a.FrameCount));
                var prefixes=new[]{"Wait","Run","Walk","Jump","Fall","Rolling","Roll","Cap","Throw","HipDrop","Squat","Dive","Swim","Wall","Spin","Pole","Climb","Slide"};
                foreach(var clip in all.GroupBy(a=>a.Text).Select(g=>g.First()).Where(a=>args.Contains("--all-animations")||a.FrameCount<=600&&prefixes.Any(p=>a.Text.StartsWith(p,StringComparison.OrdinalIgnoreCase)))) {
                    var anim=new Assimp.Animation {Name=clip.Text,DurationInTicks=clip.FrameCount,TicksPerSecond=60.0};
                    var channels=new Dictionary<string,NodeAnimationChannel>();
                    foreach(var bone in model.Skeleton.bones) {var node=scene.RootNode.FindNode(bone.Text)??scene.RootNode.FindNode("Armature_"+bone.Text);if(node==null)continue;var ch=new NodeAnimationChannel {NodeName=node.Name};channels[bone.Text]=ch;anim.NodeAnimationChannels.Add(ch);}
                    model.Skeleton.reset();
                    for(int frame=0;frame<=clip.FrameCount;frame++) {
                        clip.SetFrame(frame);clip.NextFrame(model.Skeleton,false,false);
                        foreach(var bone in model.Skeleton.bones) {
                            NodeAnimationChannel ch;if(!channels.TryGetValue(bone.Text,out ch))continue;
                            var p=bone.GetPosition();var s=bone.GetScale();var q=bone.rot;
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
