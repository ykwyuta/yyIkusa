# Pack generated transparent 5x5 sheets without altering the artwork.
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$root=Split-Path $PSScriptRoot -Parent
$assets=Join-Path $root 'assets/units'
$manifest=Get-Content -LiteralPath (Join-Path $assets 'generation.json') -Raw | ConvertFrom-Json
if($manifest.units.Count -ne 8){throw 'Eight troop sheets required'}
$output=[IO.File]::Create((Join-Path $assets 'walk_cells.rgba'))
$preview=[Drawing.Bitmap]::new(640,1280)
$canvas=[Drawing.Graphics]::FromImage($preview)
$canvas.Clear([Drawing.Color]::FromArgb(27,40,47))
$font=[Drawing.Font]::new('Yu Gothic UI',11)
try {
    for($unit=0;$unit -lt 8;$unit++) {
        $item=$manifest.units[$unit]
        $source=[Drawing.Bitmap]::FromFile((Join-Path $assets ('movement/'+$item.id+'.png')))
        try {
            if([Math]::Abs($source.Width/$source.Height-1.0) -gt 0.01){throw "Non-square 5x5 sheet: $($item.id)"}
            $canvas.DrawString($item.name,$font,[Drawing.Brushes]::White,4,($unit*160))
            for($row=0;$row -lt 5;$row++){for($frame=0;$frame -lt 5;$frame++) {
                $x0=[Math]::Floor($frame*$source.Width/5);$x1=[Math]::Floor(($frame+1)*$source.Width/5)
                $y0=[Math]::Floor($row*$source.Height/5);$y1=[Math]::Floor(($row+1)*$source.Height/5)
                $tile=[Drawing.Bitmap]::new(128,128,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
                $g=[Drawing.Graphics]::FromImage($tile)
                try {
                    $g.InterpolationMode=[Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                    # One fixed transform across every frame; no per-frame recentering or resizing.
                    $g.DrawImage($source,[Drawing.Rectangle]::new(4,4,120,120),[Drawing.Rectangle]::new($x0,$y0,$x1-$x0,$y1-$y0),[Drawing.GraphicsUnit]::Pixel)
                }finally{$g.Dispose()}
                try {
                    $bits=$tile.LockBits([Drawing.Rectangle]::new(0,0,128,128),[Drawing.Imaging.ImageLockMode]::ReadOnly,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
                    try {
                        $pixels=[byte[]]::new(128*128*4)
                        if($bits.Stride -ne 512){throw 'Unexpected row stride'}
                        [Runtime.InteropServices.Marshal]::Copy($bits.Scan0,$pixels,0,$pixels.Length)
                        for($i=0;$i -lt $pixels.Length;$i+=4){$b=$pixels[$i];$pixels[$i]=$pixels[$i+2];$pixels[$i+2]=$b}
                        if($pixels[3] -ne 0){throw "Sheet must have transparent margins: $($item.id)"}
                        $output.Write($pixels,0,$pixels.Length)
                    }finally{$tile.UnlockBits($bits)}
                    if($row -eq 1){$canvas.DrawImage($tile,($frame*128),($unit*160+26),128,128)}
                }finally{$tile.Dispose()}
            }}
        }finally{$source.Dispose()}
    }
    $preview.Save((Join-Path $assets 'movement-preview.jpg'),[Drawing.Imaging.ImageFormat]::Jpeg)
}finally{$output.Dispose();$canvas.Dispose();$font.Dispose();$preview.Dispose()}
Push-Location $root
try {python tools/pack_ikusa_art.py;if($LASTEXITCODE -ne 0){throw 'Atlas packing failed'}}finally{Pop-Location}
Write-Output 'Packed 200 transparent direction/animation cells.'
