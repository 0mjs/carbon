#!/usr/bin/env bash
# Draws Carbon's app icon (icon-1024.png) and packs it for macOS (Carbon.icns). Needs ImageMagick.
#
# The tile is shaded at 16 bits a channel with a grain too fine to see, then brought down to 8, so the black has
# no steps in it: a browser's rendering of icon.svg puts visible bands across a gradient this dark.
# Everything is drawn at twice the size and scaled down. The numbers are icon.svg's, doubled.
set -euo pipefail
cd "$(dirname "$0")"
INK='#f4f4f5'
magick -depth 16 \
  \( -size 1648x742 gradient:'#161616-#0a0a0a' \) \( -size 1648x906 gradient:'#0a0a0a-#030303' \) -append \
  -colorspace Gray -attenuate 0.12 +noise Gaussian -colorspace sRGB \
  \( -size 1648x1648 xc:black -fill white -draw 'roundrectangle 0,0 1647,1647 372,372' \) \
  -alpha off -compose CopyOpacity -composite tile.miff
magick -depth 16 -size 2048x2048 xc:none \
  \( tile.miff -background black -shadow 30x24+0+0 +repage \) -geometry +152+170 -compose Over -composite \
  tile.miff -geometry +200+200 -composite \
  -fill none -stroke "$INK" -strokewidth 163.2 -draw 'stroke-linecap butt ellipse 1024,1024 360,360 35,325' \
  -stroke none -fill "$INK" -draw 'circle 1388.8,1024 1480,1024' \
  -resize 1024x1024 -depth 8 icon-1024.png
rm tile.miff

rm -rf Carbon.iconset && mkdir Carbon.iconset
for size in 16 32 128 256 512; do
  sips -z $size $size icon-1024.png --out Carbon.iconset/icon_${size}x${size}.png >/dev/null
  sips -z $((size * 2)) $((size * 2)) icon-1024.png --out Carbon.iconset/icon_${size}x${size}@2x.png >/dev/null
done
iconutil -c icns Carbon.iconset -o Carbon.icns
rm -rf Carbon.iconset
echo "brand/icon-1024.png and brand/Carbon.icns"
