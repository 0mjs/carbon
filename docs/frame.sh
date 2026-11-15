#!/usr/bin/env bash
# Frames a render of the window as a window: round corners, its three buttons, a shadow, on a dark backdrop
set -euo pipefail
in="$1"; out="$2"; dir=$(dirname "$in")
w=$(magick identify -format %w "$in"); h=$(magick identify -format %h "$in"); m=150
magick "$in" \
  -fill '#ff5f57' -draw 'circle 52,52 64,52' -fill '#febc2e' -draw 'circle 92,52 104,52' -fill '#28c840' -draw 'circle 132,52 144,52' \
  \( -size "${w}x${h}" xc:black -fill white -draw "roundrectangle 0,0 $((w-1)),$((h-1)) 26,26" \) \
  -alpha off -compose CopyOpacity -composite "$dir/win.png"
magick -size "$((w+2*m))x$((h+2*m))" radial-gradient:'#24242c-#070709' \
  \( "$dir/win.png" -background black -shadow 60x40+0+24 \) -gravity center -compose Over -composite \
  "$dir/win.png" -gravity center -composite \
  -resize 2200x -strip -define png:compression-level=9 "$out"
rm "$dir/win.png"
