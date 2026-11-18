# Repack generated 5x2 expression sheets; Windows/.NET only, no external packages.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$assetRoot = Join-Path (Split-Path $PSScriptRoot -Parent) 'assets/generals'
$manifest = Get-Content -LiteralPath (Join-Path $assetRoot 'generation.json') -Raw | ConvertFrom-Json
if ($manifest.characters.Count -ne 20) { throw 'Expected twenty joint command slots' }
if (($manifest.characters.id | Sort-Object -Unique).Count -ne 20) { throw 'Duplicate character id' }

function Read-Cell($sheet, [int]$index, [int]$size) {
    $col = $index % 5; $row = [Math]::Floor($index / 5)
    # Trim the tiny generated seam between panels, preserving both sisters together.
    $x0 = [Math]::Floor($col * $sheet.Width / 5)
    $x1 = [Math]::Floor(($col + 1) * $sheet.Width / 5)
    $y0 = [Math]::Floor($row * $sheet.Height / 2)
    $y1 = [Math]::Floor(($row + 1) * $sheet.Height / 2)
    $inset = [Math]::Max(2, [Math]::Floor(($x1 - $x0) * 0.01))
    $rect = [Drawing.Rectangle]::new($x0+$inset,$y0+$inset,$x1-$x0-2*$inset,$y1-$y0-2*$inset)
    $bitmap = [Drawing.Bitmap]::new($size,$size,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.DrawImage($sheet,[Drawing.Rectangle]::new(0,0,$size,$size),$rect,[Drawing.GraphicsUnit]::Pixel)
    } finally { $graphics.Dispose() }
    $bits = $bitmap.LockBits([Drawing.Rectangle]::new(0,0,$size,$size),[Drawing.Imaging.ImageLockMode]::ReadOnly,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        if ($bits.Stride -ne $size*4) { throw 'Unexpected BGRA row stride' }
        $pixels = [byte[]]::new($size*$size*4)
        [Runtime.InteropServices.Marshal]::Copy($bits.Scan0,$pixels,0,$pixels.Length)
    } finally { $bitmap.UnlockBits($bits) }
    return [pscustomobject]@{ Bitmap=$bitmap; Pixels=$pixels }
}

$portraits = [IO.File]::Create((Join-Path $assetRoot 'portraits.bgra'))
$expressions = [IO.File]::Create((Join-Path $assetRoot 'expressions/expressions.bgra'))
$preview = [Drawing.Bitmap]::new(800,720)
$canvas = [Drawing.Graphics]::FromImage($preview)
$canvas.Clear([Drawing.Color]::FromArgb(24,31,39))
$font = [Drawing.Font]::new('Yu Gothic UI',10)
try {
    for ($id=0; $id -lt 20; $id++) {
        $character = $manifest.characters[$id]
        $sheet = [Drawing.Bitmap]::FromFile((Join-Path $assetRoot ('expressions/'+$character.id+'.png')))
        try {
            if ([Math]::Abs($sheet.Width/$sheet.Height - 2.5) -gt 0.08) { throw "Invalid 5x2 sheet: $($character.id)" }
            $portrait = Read-Cell $sheet 0 128
            try {
                $portraits.Write($portrait.Pixels,0,$portrait.Pixels.Length)
                $portrait.Bitmap.Save((Join-Path $assetRoot ($character.id+'.png')),[Drawing.Imaging.ImageFormat]::Png)
                $x=($id%5)*160; $y=[Math]::Floor($id/5)*180
                $canvas.DrawImage($portrait.Bitmap,$x+16,$y+5,128,128)
                $canvas.DrawString(('{0:00} {1}' -f ($id+1),$character.name),$font,[Drawing.Brushes]::White,[single]($x+4),[single]($y+138))
            } finally { $portrait.Bitmap.Dispose() }
            for ($cue=0; $cue -lt 10; $cue++) {
                $cell = Read-Cell $sheet $cue 96
                try { $expressions.Write($cell.Pixels,0,$cell.Pixels.Length) }
                finally { $cell.Bitmap.Dispose() }
            }
        } finally { $sheet.Dispose() }
    }
    $preview.Save((Join-Path $assetRoot 'catalog.jpg'),[Drawing.Imaging.ImageFormat]::Jpeg)
} finally {
    $portraits.Dispose(); $expressions.Dispose(); $canvas.Dispose(); $font.Dispose(); $preview.Dispose()
}
Write-Output 'Packed 20 portraits and 200 expressions.'
