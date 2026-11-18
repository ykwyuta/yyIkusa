# Prepare replacement artwork for parallel additions without modifying the active roster.
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$root=Split-Path $PSScriptRoot -Parent
$assets=Join-Path $root 'assets/generals'
$manifest=Get-Content -LiteralPath (Join-Path $assets 'generation-additions.json') -Raw | ConvertFrom-Json
if($manifest.characters.Count -ne 6){throw 'Expected six replacement characters'}
$destination=Join-Path $assets 'additions'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$portraits=[IO.File]::Create((Join-Path $destination 'portraits.bgra'))
$expressions=[IO.File]::Create((Join-Path $destination 'expressions.bgra'))
$preview=[Drawing.Bitmap]::new(960,180)
$canvas=[Drawing.Graphics]::FromImage($preview)
$canvas.Clear([Drawing.Color]::FromArgb(24,31,39))
$font=[Drawing.Font]::new('Yu Gothic UI',11)
$hashes=[Collections.Generic.HashSet[string]]::new()
$sha=[Security.Cryptography.SHA256]::Create()
try {
    for($id=0;$id -lt $manifest.characters.Count;$id++) {
        $character=$manifest.characters[$id]
        $sheet=[Drawing.Bitmap]::FromFile((Join-Path $assets ('expressions/'+$character.id+'.png')))
        try {
            if([Math]::Abs($sheet.Width/$sheet.Height-2.5) -gt 0.08){throw "Invalid sheet: $($character.id)"}
            for($cue=0;$cue -lt 10;$cue++) {
                $col=$cue%5;$row=[Math]::Floor($cue/5)
                $x0=[Math]::Floor($col*$sheet.Width/5);$x1=[Math]::Floor(($col+1)*$sheet.Width/5)
                $y0=[Math]::Floor($row*$sheet.Height/2);$y1=[Math]::Floor(($row+1)*$sheet.Height/2)
                $inset=[Math]::Max(2,[Math]::Floor(($x1-$x0)*0.01))
                $source=[Drawing.Rectangle]::new($x0+$inset,$y0+$inset,$x1-$x0-2*$inset,$y1-$y0-2*$inset)
                $sizes=if($cue -eq 0){@(96,128)}else{@(96)}
                foreach($size in $sizes) {
                    $tile=[Drawing.Bitmap]::new($size,$size,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
                    $g=[Drawing.Graphics]::FromImage($tile)
                    try {
                        $g.InterpolationMode=[Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                        $g.DrawImage($sheet,[Drawing.Rectangle]::new(0,0,$size,$size),$source,[Drawing.GraphicsUnit]::Pixel)
                    }finally{$g.Dispose()}
                    try {
                        $bits=$tile.LockBits([Drawing.Rectangle]::new(0,0,$size,$size),[Drawing.Imaging.ImageLockMode]::ReadOnly,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
                        try {
                            if($bits.Stride -ne $size*4){throw 'Unexpected stride'}
                            $pixels=[byte[]]::new($size*$size*4)
                            [Runtime.InteropServices.Marshal]::Copy($bits.Scan0,$pixels,0,$pixels.Length)
                        }finally{$tile.UnlockBits($bits)}
                        if($size -eq 96){
                            for($i=3;$i -lt $pixels.Length;$i+=4){if($pixels[$i] -ne 255){throw 'Portrait background must be opaque'}}
                            if(-not $hashes.Add([BitConverter]::ToString($sha.ComputeHash($pixels)))){throw 'Duplicate expression'}
                            $expressions.Write($pixels,0,$pixels.Length)
                        }else{
                            $portraits.Write($pixels,0,$pixels.Length)
                            $tile.Save((Join-Path $assets ($character.id+'.png')),[Drawing.Imaging.ImageFormat]::Png)
                            $canvas.DrawImage($tile,($id*160+16),4,128,128)
                            $canvas.DrawString($character.name,$font,[Drawing.Brushes]::White,($id*160+6),142)
                        }
                    }finally{$tile.Dispose()}
                }
            }
        }finally{$sheet.Dispose()}
    }
    $preview.Save((Join-Path $assets 'catalog-additions.jpg'),[Drawing.Imaging.ImageFormat]::Jpeg)
}finally{$sha.Dispose();$font.Dispose();$canvas.Dispose();$preview.Dispose();$portraits.Dispose();$expressions.Dispose()}
if((Get-Item -LiteralPath (Join-Path $destination 'portraits.bgra')).Length -ne 393216){throw 'Portrait count mismatch'}
if((Get-Item -LiteralPath (Join-Path $destination 'expressions.bgra')).Length -ne 2211840){throw 'Expression count mismatch'}
Write-Output 'Prepared six portraits and sixty distinct expressions; active roster unchanged.'
