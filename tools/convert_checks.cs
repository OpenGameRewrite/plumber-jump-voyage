using System;
using Assimp;
using Toolbox.Library;

partial class ConvertOutfit {
    static void SelfCheck() {
        var skeleton=new STSkeleton();
        var bone=new STBone(skeleton) {pos=new OpenTK.Vector3(1,2,3),sca=OpenTK.Vector3.One,rot=OpenTK.Quaternion.FromAxisAngle(OpenTK.Vector3.UnitZ,0.3f)};
        var parent=OpenTK.Matrix4.CreateTranslation(10,0,0);
        var fix=OpenTK.Matrix4.CreateTranslation(0,4,0);
        var chained=OpenTK.Vector3.TransformPosition(OpenTK.Vector3.Zero,PartPose(bone,true,parent,fix));
        var attached=OpenTK.Vector3.TransformPosition(OpenTK.Vector3.Zero,PartPose(bone,false,parent,fix));
        if((chained-new OpenTK.Vector3(1,2,3)).Length>0.0001f||(attached-new OpenTK.Vector3(11,6,3)).Length>0.0001f)
            throw new Exception("Independent part placement failed");

                if(Wrap(STTextureWrapMode.Clamp)!=TextureWrapMode.Clamp||Wrap(STTextureWrapMode.Mirror)!=TextureWrapMode.Mirror||Wrap(STTextureWrapMode.Repeat)!=TextureWrapMode.Wrap)
                    throw new Exception("Texture wrapping failed");
                if(PartAction("Face","Run")!="Move"||PartAction("Face","Walk")!="Move"||PartAction("Face","Jump")!="Jump"||PartAction("Eye","Run")!="Run")
                    throw new Exception("Facial locomotion action failed");
                if(NormalBlue(128,128)!=255||NormalBlue(255,255)!=128)throw new Exception("Normal reconstruction failed");
                if(PartSet("MarioEye")!="Eye"||PartSet("MarioFaceRsv")!="Face"||PartSet("MarioAlohaHead")!="Head"
                    ||PartName("Eye","Eyeball")==PartName("Face","Eyeball")||PartName("L","HandL")!="HandL")
                    throw new Exception("Part rig naming failed");
        Console.WriteLine("Converter checks passed");
    }
}
