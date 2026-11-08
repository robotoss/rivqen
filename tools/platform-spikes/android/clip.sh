#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Transcodes a raw emulator screen recording into the recording format of WP-04 S3 (sprint record
# §2, CONTRACT.md §9): MP4, H.264, no audio, at most 540 px wide, at most 30 s, at most 1,000,000
# bytes, metadata removed. run.sh calls it in the recording mode (SPIKE_RECORD=1).
# Usage: sh clip.sh <raw.mp4> <out.mp4> <start-ms> <duration-ms>
#   start-ms     the part before it is cut away (the launcher before the activity is on screen)
#   duration-ms  the length to keep from start-ms (cut to 30000)
# Needs ffmpeg and ffprobe (a lab tool on the Mac, not a dependency of the repository): FFMPEG,
# FFPROBE, else the ones on PATH, else /opt/homebrew/bin. Exit 1 when no setting meets the caps.
set -eu
die() { echo "clip.sh: $*" >&2; exit 1; }
[ $# -eq 4 ] || { echo "usage: clip.sh <raw.mp4> <out.mp4> <start-ms> <duration-ms>" >&2; exit 2; }
raw="$1"
out="$2"
start_ms="$3"
dur_ms="$4"
case "$start_ms" in "" | *[!0-9]*) die "start-ms must be digits" ;; esac
case "$dur_ms" in "" | *[!0-9]*) die "duration-ms must be digits" ;; esac
[ -f "$raw" ] || die "no raw recording $raw"
tool() { command -v "$1" 2>/dev/null || echo "/opt/homebrew/bin/$1"; }
FFMPEG="${FFMPEG:-$(tool ffmpeg)}"
FFPROBE="${FFPROBE:-$(tool ffprobe)}"
[ -x "$FFMPEG" ] && [ -x "$FFPROBE" ] || die "ffmpeg or ffprobe not found (set FFMPEG and FFPROBE)"

MAX_BYTES=1000000
MAX_MS=30000
[ "$dur_ms" -le "$MAX_MS" ] || dur_ms="$MAX_MS"
[ "$dur_ms" -gt 0 ] || die "nothing to keep"
sec() { printf '%d.%03d' $(($1 / 1000)) $(($1 % 1000)); }

tmp="$out.tmp.mp4"
trap 'rm -f "$tmp"' EXIT
# Quality steps: a higher CRF (and then a lower frame rate) until the clip is within the byte cap.
for setting in "26 15" "30 15" "34 12" "38 10" "42 8"; do
  crf="${setting% *}"
  fps="${setting#* }"
  # -an: no audio; -map_metadata -1 (file and stream) and -map_chapters -1: no metadata; bitexact:
  # no version or time strings (the codec name "Lavc libx264" stays in the sample description);
  # filter_units=remove_types=6: no SEI units (x264 writes its settings there).
  "$FFMPEG" -nostdin -y -v error -ss "$(sec "$start_ms")" -i "$raw" -t "$(sec "$dur_ms")" \
    -an -sn -dn -map 0:v:0 -map_metadata -1 -map_metadata:s:v -1 -map_chapters -1 \
    -vf "scale='min(540,iw)':-2:flags=bicubic,fps=$fps" \
    -c:v libx264 -preset veryslow -crf "$crf" -pix_fmt yuv420p -profile:v high \
    -bsf:v filter_units=remove_types=6 -fflags +bitexact -flags:v +bitexact \
    -movflags +faststart "$tmp"
  size="$(wc -c < "$tmp" | tr -d ' ')"
  if [ "$size" -le "$MAX_BYTES" ]; then
    break
  fi
done
[ "$size" -le "$MAX_BYTES" ] || die "clip is $size bytes at the last quality step (limit $MAX_BYTES)"

# Verify the caps on the file itself.
probe="$("$FFPROBE" -v error -show_entries stream=codec_type,codec_name,width:format=duration:format_tags -of default=nw=1 "$tmp")"
echo "$probe" | grep -q '^codec_type=audio' && die "clip has an audio stream"
[ "$(echo "$probe" | grep -c '^codec_type=video')" -eq 1 ] || die "clip must have one video stream"
echo "$probe" | grep -q '^codec_name=h264' || die "clip is not H.264"
width="$(echo "$probe" | sed -n 's/^width=//p')"
[ "$width" -le 540 ] || die "clip is $width px wide"
duration="$(echo "$probe" | sed -n 's/^duration=//p')"
awk -v d="$duration" 'BEGIN { exit !(d > 0 && d <= 30.05) }' || die "clip is $duration s long"
# Only the MP4 brand tags may stay (no creation time, encoder, location or other metadata).
extra="$(echo "$probe" | grep '^TAG:' | grep -v -E '^TAG:(major_brand|minor_version|compatible_brands)=' || true)"
[ -z "$extra" ] || die "clip has metadata: $extra"
mv "$tmp" "$out"
echo "clip.sh: $out: $size bytes, ${duration} s, $width px wide, crf $crf, $fps fps"
