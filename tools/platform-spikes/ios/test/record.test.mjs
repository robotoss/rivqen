// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { joinRun } from "../join.mjs";
import { clipProblems, CRF_STEPS, END_HOLD_SECONDS, ffmpegArgs, MAX_BYTES, MAX_SECONDS, MAX_WIDTH, stabilityProblems } from "../record.mjs";

const meta = (k, ok = true) => ({
  run: `CAP-I-05-ios-27.0-${k}`, capability: "CAP-I-05", os: "27.0", k, device: "iPhone 16 Pro",
  prototype: "IOS-P3", ok, error: ok ? null : "error: x", t_end: 900 + k,
});
const app = (k, value = "0", reads = [100, 50]) =>
  [
    { ev: "app.probe", name: "chunk", seq: 0, src: "app", t: 10 + k, value },
    ...reads.map((bytes, i) => ({ bytes, ev: "app.stream.read", seq: 1 + i, src: "app", t: 20 + i })),
    { ev: "app.probe", name: "metric.first_paint", seq: 9, src: "app", t: 30, value: 40 + k },
  ]
    .map((e) => JSON.stringify(e))
    .join("\n") + "\n";
const host = (k, extra = "") =>
  `{"ev":"host.request","method":"GET","seq":0,"site":"A","src":"host","t":${k},"target":"/s/stream?run=CAP-I-05-ios-27.0-${k}${extra}"}\n`;

test("stable_recorded_run_equals_stored_after_normalization", () => {
  // Different k, run id, times, metric value and read split; same normalized log.
  const stored = joinRun(meta(1), app(1), host(1));
  const recorded = joinRun(meta(4), app(4, "0", [150]), host(4));
  assert.deepEqual(stabilityProblems(recorded, stored), []);
});

test("stable_refuses_a_run_that_is_not_ok", () => {
  const stored = joinRun(meta(1), app(1), host(1));
  const recorded = joinRun(meta(4, false), app(4), host(4));
  const problems = stabilityProblems(recorded, stored);
  assert.ok(problems.includes("recorded: run.end is not ok"), problems.join("; "));
});

test("stable_names_the_first_differing_event_per_source", () => {
  const stored = joinRun(meta(1), app(1), host(1));
  const recorded = joinRun(meta(4), app(4, "1"), host(4, "&x=1"));
  const problems = stabilityProblems(recorded, stored);
  assert.equal(problems.length, 2, problems.join("; "));
  assert.match(problems[0], /^app event 0 differs/);
  assert.match(problems[1], /^host event 0 differs/);
});

test("stable_detects_a_missing_event_and_a_byte_count_change", () => {
  const stored = joinRun(meta(1), app(1), host(1) + host(1, "&k=1"));
  const recorded = joinRun(meta(4), app(4, "0", [100]), host(4));
  const problems = stabilityProblems(recorded, stored);
  assert.ok(problems.some((p) => /^host event 1 differs: recorded null/.test(p)), problems.join("; "));
  assert.ok(problems.includes("stream bytes differ: recorded 100 / stored 150"), problems.join("; "));
});

test("stable_reports_parse_problems_instead_of_comparing", () => {
  const stored = joinRun(meta(1), app(1), host(1));
  const problems = stabilityProblems("not json\n", stored);
  assert.ok(problems.length > 0 && problems.every((p) => p.startsWith("recorded: ")), problems.join("; "));
});

test("ffmpeg_args_follow_the_format", () => {
  const args = ffmpegArgs("in.mov", "out.mp4", 32);
  const after = (flag) => args[args.indexOf(flag) + 1];
  assert.equal(after("-i"), "in.mov");
  assert.equal(args.at(-1), "out.mp4");
  assert.ok(args.includes("-an"));
  assert.equal(after("-map_metadata"), "-1");
  assert.equal(after("-c:v"), "libx264");
  assert.equal(after("-crf"), "32");
  assert.equal(after("-t"), String(MAX_SECONDS));
  assert.match(after("-vf"), new RegExp(`^scale=${MAX_WIDTH}:-2`));
  assert.match(after("-vf"), new RegExp(`tpad=stop_mode=clone:stop_duration=${END_HOLD_SECONDS}$`));
  assert.ok(MAX_SECONDS - END_HOLD_SECONDS > 0);
  assert.equal(after("-f"), "mp4");
  assert.throws(() => ffmpegArgs("in.mov", "out.mp4", 23), /crf 23/);
  assert.deepEqual(CRF_STEPS, [...CRF_STEPS].sort((a, b) => a - b));
});

const goodProbe = () => ({
  format: { format_name: "mov,mp4,m4a,3gp,3g2,mj2", duration: "8.4", tags: { major_brand: "isom", minor_version: "512", compatible_brands: "isomiso2avc1mp41" } },
  streams: [{ codec_type: "video", codec_name: "h264", width: 540, height: 1174, tags: { language: "und", handler_name: "VideoHandler" } }],
});

test("clip_within_the_format_has_no_problems", () => {
  assert.deepEqual(clipProblems(goodProbe(), 600_000), []);
  assert.deepEqual(clipProblems(goodProbe(), MAX_BYTES), []);
  const p = goodProbe();
  p.format.duration = String(MAX_SECONDS);
  assert.deepEqual(clipProblems(p, 1), []);
});

test("clip_over_a_cap_is_refused", () => {
  assert.deepEqual(clipProblems(goodProbe(), MAX_BYTES + 1), [`size ${MAX_BYTES + 1} bytes, at most ${MAX_BYTES}`]);
  assert.equal(clipProblems(goodProbe(), 0).length, 1);
  const long = goodProbe();
  long.format.duration = "30.05";
  assert.deepEqual(clipProblems(long, 1000), [`duration 30.05, at most ${MAX_SECONDS} s`]);
  const wide = goodProbe();
  wide.streams[0].width = MAX_WIDTH + 2;
  assert.deepEqual(clipProblems(wide, 1000), [`width ${MAX_WIDTH + 2}, at most ${MAX_WIDTH}`]);
  const hevc = goodProbe();
  hevc.streams[0].codec_name = "hevc";
  assert.deepEqual(clipProblems(hevc, 1000), ["codec hevc, h264 is required"]);
  const mov = goodProbe();
  mov.format.format_name = "matroska,webm";
  assert.deepEqual(clipProblems(mov, 1000), ["container matroska,webm, mp4 is required"]);
});

test("clip_with_audio_or_no_video_is_refused", () => {
  const audio = goodProbe();
  audio.streams.push({ codec_type: "audio", codec_name: "aac" });
  assert.deepEqual(clipProblems(audio, 1000), ["2 streams, exactly 1 (video) is allowed", "audio stream present"]);
  assert.deepEqual(clipProblems({ format: goodProbe().format, streams: [] }, 1000), ["0 streams, exactly 1 (video) is allowed", "no video stream"]);
  assert.ok(clipProblems({}, 1000).length >= 3);
});

test("clip_with_identifying_metadata_is_refused", () => {
  const p = goodProbe();
  p.format.tags.creation_time = "2026-11-08T00:00:00Z";
  p.format.tags["com.apple.quicktime.model"] = "x";
  p.streams[0].tags.creation_time = "2026-11-08T00:00:00Z";
  assert.deepEqual(clipProblems(p, 1000), [
    "format tag creation_time is not removed",
    "format tag com.apple.quicktime.model is not removed",
    "stream 0 tag creation_time is not removed",
  ]);
});

test("clip_codec_name_tag_is_allowed", () => {
  // The mp4 sample description names the codec; it says nothing about the recording.
  const p = goodProbe();
  p.streams[0].tags.encoder = "Lavc libx264";
  assert.deepEqual(clipProblems(p, 1000), []);
});
