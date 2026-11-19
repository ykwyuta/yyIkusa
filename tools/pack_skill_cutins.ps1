# Pack generated PNG originals into compact 768x512 indexed cut-in assets. No external dependencies.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$drawingReferences = @('System.Drawing.Common', 'System.Drawing.Primitives', 'System.Runtime', 'System.Collections', 'System.Runtime.InteropServices')
# Recent PowerShell/.NET versions split GDI+ interfaces into additional assemblies.
foreach ($assembly in @('System.Private.Windows.GdiPlus', 'System.Private.Windows.Core')) {
    if (Test-Path -LiteralPath (Join-Path $PSHOME ($assembly + '.dll'))) { $drawingReferences += $assembly }
}
Add-Type -ReferencedAssemblies $drawingReferences -TypeDefinition @'
using System;
using System.IO;
using System.Drawing;
using System.Drawing.Imaging;
using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;
using System.Collections.Generic;

public static class SkillCutinPacker {
    class ColorBin { public int R,G,B,N; }
    class Box {
        public List<ColorBin> Colors; public int Axis, Weight; public double Score;
        public Box(List<ColorBin> colors) {
            Colors=colors; int[] low={255,255,255}, high={0,0,0};
            foreach(var c in colors) { int[] v={c.R,c.G,c.B}; Weight+=c.N;
                for(int k=0;k<3;k++){low[k]=Math.Min(low[k],v[k]);high[k]=Math.Max(high[k],v[k]);}
            }
            int range=0; for(int k=0;k<3;k++)if(high[k]-low[k]>range){range=high[k]-low[k];Axis=k;}
            Score=colors.Count>1 ? (range+1)*Math.Sqrt(Weight) : -1;
        }
    }
    public static int Pack(string source, string output) {
        const int W=768,H=512;
        byte[] rgba;
        using(var image=Image.FromFile(source))
        using(var bitmap=new Bitmap(W,H,PixelFormat.Format32bppArgb)) {
            if(image.Width<1024 || image.Height<640)throw new Exception("Cut-in original is too small: "+source);
            if(Math.Abs((double)image.Width/image.Height-1.5)>0.05)throw new Exception("Expected 3:2 cut-in art: "+source);
            using(var g=Graphics.FromImage(bitmap)) {
                g.InterpolationMode=InterpolationMode.HighQualityBicubic;
                g.DrawImage(image,new Rectangle(0,0,W,H));
            }
            var bits=bitmap.LockBits(new Rectangle(0,0,W,H),ImageLockMode.ReadOnly,PixelFormat.Format32bppArgb);
            rgba=new byte[W*H*4];
            try {for(int y=0;y<H;y++)Marshal.Copy(IntPtr.Add(bits.Scan0,y*bits.Stride),rgba,y*W*4,W*4);}
            finally {bitmap.UnlockBits(bits);}
        }
        int[] count=new int[32768],rs=new int[32768],gs=new int[32768],bs=new int[32768];
        for(int i=0;i<rgba.Length;i+=4) {
            int r=rgba[i+2],g=rgba[i+1],b=rgba[i];
            int key=(r>>3)*1024+(g>>3)*32+(b>>3);
            count[key]++;rs[key]+=r;gs[key]+=g;bs[key]+=b;
        }
        var colors=new List<ColorBin>();
        for(int i=0;i<count.Length;i++)if(count[i]>0)colors.Add(new ColorBin{R=rs[i]/count[i],G=gs[i]/count[i],B=bs[i]/count[i],N=count[i]});
        var boxes=new List<Box>{new Box(colors)};
        while(boxes.Count<256) {
            int pick=0;for(int k=1;k<boxes.Count;k++)if(boxes[k].Score>boxes[pick].Score)pick=k;
            var box=boxes[pick];if(box.Score<0)break;int axis=box.Axis;
            box.Colors.Sort((a,b)=>(axis==0?a.R:axis==1?a.G:a.B).CompareTo(axis==0?b.R:axis==1?b.G:b.B));
            int sum=0,split=0;while(split<box.Colors.Count-1 && sum<box.Weight/2)sum+=box.Colors[split++].N;
            split=Math.Max(1,split);
            boxes[pick]=new Box(box.Colors.GetRange(0,split));
            boxes.Add(new Box(box.Colors.GetRange(split,box.Colors.Count-split)));
        }
        byte[] palette=new byte[768];
        for(int k=0;k<boxes.Count;k++) {
            long r=0,g=0,b=0,n=0;foreach(var c in boxes[k].Colors){r+=(long)c.R*c.N;g+=(long)c.G*c.N;b+=(long)c.B*c.N;n+=c.N;}
            palette[k*3]=(byte)(r/n);palette[k*3+1]=(byte)(g/n);palette[k*3+2]=(byte)(b/n);
        }
        byte[] lookup=new byte[32768];
        for(int key=0;key<count.Length;key++)if(count[key]>0) {
            int r=rs[key]/count[key],g=gs[key]/count[key],b=bs[key]/count[key],best=0,error=int.MaxValue;
            for(int k=0;k<boxes.Count;k++) {
                int dr=r-palette[k*3],dg=g-palette[k*3+1],db=b-palette[k*3+2];
                int e=dr*dr*2+dg*dg*3+db*db;if(e<error){error=e;best=k;}
            }
            lookup[key]=(byte)best;
        }
        byte[] indices=new byte[W*H];
        for(int i=0;i<indices.Length;i++){int p=i*4;indices[i]=lookup[(rgba[p+2]>>3)*1024+(rgba[p+1]>>3)*32+(rgba[p]>>3)];}
        using(var file=new BinaryWriter(File.Create(output))) {
            file.Write(new byte[]{89,89,67,73});file.Write((ushort)W);file.Write((ushort)H);file.Write(palette);
            int i=0;while(i<indices.Length) {
                int run=1;while(i+run<indices.Length && run<130 && indices[i+run]==indices[i])run++;
                if(run>=3){file.Write((byte)(run+125));file.Write(indices[i]);i+=run;}
                else {
                    int start=i;i++;
                    while(i<indices.Length && i-start<128) {
                        if(i+2<indices.Length && indices[i]==indices[i+1] && indices[i]==indices[i+2])break;
                        i++;
                    }
                    file.Write((byte)(i-start-1));file.Write(indices,start,i-start);
                }
            }
        }
        return (int)new FileInfo(output).Length;
    }
}
'@

$assetRoot = Join-Path (Split-Path $PSScriptRoot -Parent) 'assets/generals/cutins'
$manifest = Get-Content -LiteralPath (Join-Path $assetRoot 'manifest.json') -Raw | ConvertFrom-Json
$total = 0
foreach ($character in $manifest.characters) {
    if ($character.id -notmatch '^[a-z_]+$') { throw 'Invalid asset identifier' }
    $source = Join-Path $assetRoot ($character.id + '.png')
    $output = Join-Path $assetRoot ($character.id + '.yyci')
    $bytes = [SkillCutinPacker]::Pack($source, $output)
    $total += $bytes
    Write-Output ($character.id + ': ' + $bytes + ' bytes')
}
Write-Output ('Total cut-in payload: ' + $total + ' bytes')



