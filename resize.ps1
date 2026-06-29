Add-Type -AssemblyName System.Drawing
$sourcePath = "C:\Users\Noah\Documents\GitHub\GotiDesk\public\logo.png"
$targetPath = "C:\Users\Noah\Documents\GitHub\GotiDesk\public\logo_512.png"

$img = [System.Drawing.Image]::FromFile($sourcePath)
$bmp = New-Object System.Drawing.Bitmap 512, 512
$graph = [System.Drawing.Graphics]::FromImage($bmp)

# Set high-quality scaling algorithms
$graph.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$graph.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
$graph.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$graph.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality

$graph.DrawImage($img, 0, 0, 512, 512)
$bmp.Save($targetPath, [System.Drawing.Imaging.ImageFormat]::Png)

$graph.Dispose()
$bmp.Dispose()
$img.Dispose()
