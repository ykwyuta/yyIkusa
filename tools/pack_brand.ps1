# Resize/package image_gen artwork; do not paint or alter the generated subjects.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot -Parent
$brand = Join-Path $root 'assets/branding'
function Resize-Art($image,[int]$width,[int]$height) {
    $bitmap=[Drawing.Bitmap]::new($width,$height,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g=[Drawing.Graphics]::FromImage($bitmap)
    try {
        $g.InterpolationMode=[Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.DrawImage($image,[Drawing.Rectangle]::new(0,0,$width,$height))
    } finally {$g.Dispose()}
    return $bitmap
}
function Write-Rgb($bitmap,$path) {
    $rect=[Drawing.Rectangle]::new(0,0,$bitmap.Width,$bitmap.Height)
    $bits=$bitmap.LockBits($rect,[Drawing.Imaging.ImageLockMode]::ReadOnly,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $bgra=[byte[]]::new($bits.Stride*$bitmap.Height)
        [Runtime.InteropServices.Marshal]::Copy($bits.Scan0,$bgra,0,$bgra.Length)
        $rgb=[byte[]]::new($bitmap.Width*$bitmap.Height*3)
        for($y=0;$y -lt $bitmap.Height;$y++) {for($x=0;$x -lt $bitmap.Width;$x++) {
            $i=$y*$bits.Stride+$x*4; $j=($y*$bitmap.Width+$x)*3
            $rgb[$j]=$bgra[$i+2];$rgb[$j+1]=$bgra[$i+1];$rgb[$j+2]=$bgra[$i]
        }}
        [IO.File]::WriteAllBytes($path,$rgb)
    } finally {$bitmap.UnlockBits($bits)}
}
$icon=[Drawing.Bitmap]::FromFile((Join-Path $brand 'icon-source.png'))
$title=[Drawing.Bitmap]::FromFile((Join-Path $brand 'title-source.png'))
try {
    foreach($item in @(@('ikusa-192.png',192),@('ikusa-512.png',512),@('ikusa-apple.png',180))) {
        $bitmap=Resize-Art $icon $item[1] $item[1]
        try {$bitmap.Save((Join-Path $root ('web/'+$item[0])),[Drawing.Imaging.ImageFormat]::Png)} finally {$bitmap.Dispose()}
    }
    $mark=Resize-Art $icon 128 128
    try {Write-Rgb $mark (Join-Path $brand 'mark.rgb')} finally {$mark.Dispose()}
    $background=Resize-Art $title 1280 720
    try {
        Write-Rgb $background (Join-Path $brand 'title.rgb')
        $background.Save((Join-Path $root 'web/ikusa-cover.jpg'),[Drawing.Imaging.ImageFormat]::Jpeg)
    } finally {$background.Dispose()}
    $sizes=@(16,24,32,48,64,128,256)
    $images=@()
    foreach($size in $sizes) {
        $bitmap=Resize-Art $icon $size $size
        $memory=[IO.MemoryStream]::new()
        try {$bitmap.Save($memory,[Drawing.Imaging.ImageFormat]::Png);$images+=,[byte[]]$memory.ToArray()}
        finally {$memory.Dispose();$bitmap.Dispose()}
    }
    $memory=[IO.MemoryStream]::new();$writer=[IO.BinaryWriter]::new($memory)
    try {
        $writer.Write([uint16]0);$writer.Write([uint16]1);$writer.Write([uint16]$sizes.Count)
        $offset=6+16*$sizes.Count
        for($i=0;$i -lt $sizes.Count;$i++) {
            $sizeByte=if($sizes[$i] -eq 256){0}else{$sizes[$i]}
            $writer.Write([byte]$sizeByte);$writer.Write([byte]$sizeByte)
            $writer.Write([byte]0);$writer.Write([byte]0);$writer.Write([uint16]1);$writer.Write([uint16]32)
            $writer.Write([uint32]$images[$i].Length);$writer.Write([uint32]$offset)
            $offset+=$images[$i].Length
        }
        foreach($image in $images){$writer.Write([byte[]]$image)}
        $writer.Flush();$bytes=$memory.ToArray()
        [IO.File]::WriteAllBytes((Join-Path $brand 'ikusa.ico'),$bytes)
        [IO.File]::WriteAllBytes((Join-Path $root 'assets/app_icon.ico'),$bytes)
    } finally {$writer.Dispose();$memory.Dispose()}
} finally {$title.Dispose();$icon.Dispose()}
Write-Output 'Packed dedicated yyIkusa icons, title background and loading cover.'
