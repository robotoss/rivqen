// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Screen recordings of the iOS prototype (CONTRACT.md section 9, WP-04 S3 recording format).
//
// node record.mjs stable --recorded <run.jsonl> --stored <run-1.jsonl>
//   Exit 0 when the recorded run ended ok and its normalized log (CONTRACT.md section 8, the rule of
//   check/) equals the stored run; 1 when not; 2 on bad use.
// node record.mjs clip --in <raw video> --out <clip.mp4> [--ffmpeg <path>] [--ffprobe <path>]
//   Transcodes the raw simctl recording into the format (H.264, no audio, at most 540 px wide, at
//   most 30 s, at most 1,000,000 bytes, metadata removed) and checks the result with ffprobe. The
//   clip is written to --out only when every rule holds; exit 1 otherwise.
//
// ffmpeg and ffprobe are lab tools on the Mac (not dependencies of the repository). They run without
// a shell, each with a time limit.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { normalizeRunLog, parseRunLog } from "../check/src/runs.mjs";

export const MAX_WIDTH = 540;
export const MAX_SECONDS = 30;
export const MAX_BYTES = 1_000_000;
/**
 * The last frame is shown this long at the end of the clip. simctl io recordVideo writes a frame only
 * when the screen changes, so a still screen at the end of the run has no frames of its own.
 */
export const END_HOLD_SECONDS = 1.5;
/** CRF steps, from the best quality down; the first clip within MAX_BYTES is kept. */
export const CRF_STEPS = [28, 32, 36, 40];
const TOOL_TIMEOUT_MS = 180_000;
/**
 * Tag names that can carry a date, a place, a device or the recording software. The stream tag `encoder`
 * (the codec name of the mp4 sample description, "Lavc libx264") is allowed: it names the codec only.
 */
const FORBIDDEN_TAG = /creation_time|date|location|make|model|software|com\.apple/i;

/**
 * Compares a recorded run with a stored run by the normalized form of CONTRACT.md section 8.
 * @param {string} recordedText @param {string} storedText
 * @returns {string[]} problems; empty when the recorded run is a stable, ok run
 */
export function stabilityProblems(recordedText, storedText) {
  const recorded = parseRunLog(recordedText);
  const stored = parseRunLog(storedText);
  const problems = [
    ...recorded.problems.map((p) => `recorded: ${p}`),
    ...stored.problems.map((p) => `stored: ${p}`),
  ];
  if (problems.length > 0) return problems;
  if (recorded.events.at(-1).ok !== true) problems.push("recorded: run.end is not ok");
  const a = normalizeRunLog(recorded.events);
  const b = normalizeRunLog(stored.events);
  for (const key of ["script", "app", "host"]) {
    const n = Math.max(a[key].length, b[key].length);
    for (let i = 0; i < n; i++) {
      const x = JSON.stringify(a[key][i] ?? null);
      const y = JSON.stringify(b[key][i] ?? null);
      if (x !== y) {
        problems.push(`${key} event ${i} differs: recorded ${x.slice(0, 200)} / stored ${y.slice(0, 200)}`);
        break;
      }
    }
  }
  if (a.stream_bytes !== b.stream_bytes) {
    problems.push(`stream bytes differ: recorded ${a.stream_bytes} / stored ${b.stream_bytes}`);
  }
  return problems;
}

/** ffmpeg arguments for one transcode step. Input and output are file paths, never shell text. */
export function ffmpegArgs(input, output, crf) {
  if (!CRF_STEPS.includes(crf)) throw new Error(`crf ${crf} is not one of ${CRF_STEPS.join(", ")}`);
  return [
    "-nostdin", "-v", "error", "-y", "-i", input,
    "-map", "0:v:0", "-an", "-sn", "-dn", "-map_metadata", "-1", "-map_chapters", "-1",
    "-vf", `scale=${MAX_WIDTH}:-2:flags=lanczos,fps=30,tpad=stop_mode=clone:stop_duration=${END_HOLD_SECONDS}`,
    "-c:v", "libx264", "-preset", "slow", "-crf", String(crf), "-pix_fmt", "yuv420p",
    "-fflags", "+bitexact", "-flags:v", "+bitexact", "-movflags", "+faststart",
    "-t", String(MAX_SECONDS), "-f", "mp4", output,
  ];
}

/** ffprobe arguments that print format and streams as JSON. */
export function ffprobeArgs(file) {
  return ["-v", "error", "-show_format", "-show_streams", "-of", "json", file];
}

/** Duration in seconds from ffprobe JSON, or NaN. */
export function durationOf(probe) {
  return Number.parseFloat(probe?.format?.duration);
}

/**
 * Checks a transcoded clip against the recording format.
 * @param {{ format?: object, streams?: object[] }} probe ffprobe JSON @param {number} size bytes
 * @returns {string[]} problems; empty when the clip is within the format
 */
export function clipProblems(probe, size) {
  const problems = [];
  const streams = Array.isArray(probe?.streams) ? probe.streams : [];
  if (streams.length !== 1) problems.push(`${streams.length} streams, exactly 1 (video) is allowed`);
  const v = streams.find((s) => s.codec_type === "video");
  if (!v) {
    problems.push("no video stream");
  } else {
    if (v.codec_name !== "h264") problems.push(`codec ${v.codec_name}, h264 is required`);
    if (!(v.width > 0 && v.width <= MAX_WIDTH)) problems.push(`width ${v.width}, at most ${MAX_WIDTH}`);
  }
  for (const s of streams) if (s.codec_type !== "video") problems.push(`${s.codec_type} stream present`);
  const d = durationOf(probe);
  if (!(d > 0 && d <= MAX_SECONDS)) problems.push(`duration ${probe?.format?.duration}, at most ${MAX_SECONDS} s`);
  if (!String(probe?.format?.format_name ?? "").split(",").includes("mp4")) {
    problems.push(`container ${probe?.format?.format_name}, mp4 is required`);
  }
  if (!(Number.isInteger(size) && size > 0 && size <= MAX_BYTES)) problems.push(`size ${size} bytes, at most ${MAX_BYTES}`);
  const tagSets = [["format", probe?.format?.tags], ...streams.map((s, i) => [`stream ${i}`, s.tags])];
  for (const [where, tags] of tagSets) {
    for (const key of Object.keys(tags ?? {})) {
      if (FORBIDDEN_TAG.test(key)) problems.push(`${where} tag ${key} is not removed`);
    }
  }
  return problems;
}

function probeFile(ffprobe, file) {
  const out = execFileSync(ffprobe, ffprobeArgs(file), { timeout: TOOL_TIMEOUT_MS, maxBuffer: 4 << 20 });
  return JSON.parse(out.toString("utf8"));
}

/**
 * Transcodes `input` into the format and copies the clip to `output` when it is valid.
 * @returns {{ problems: string[], crf?: number, size?: number, seconds?: number, width?: number }}
 */
export function makeClip({ input, output, ffmpeg, ffprobe }) {
  const raw = probeFile(ffprobe, input);
  const rawSeconds = durationOf(raw);
  // The clip shows the whole run: a raw recording that is too long is refused, not cut.
  const rawMax = MAX_SECONDS - END_HOLD_SECONDS;
  if (!(rawSeconds > 0 && rawSeconds <= rawMax)) {
    return { problems: [`raw recording is ${raw?.format?.duration} s, at most ${rawMax} s`] };
  }
  const work = mkdtempSync(path.join(tmpdir(), "ios-clip-"));
  try {
    let last = [];
    for (const crf of CRF_STEPS) {
      const tmp = path.join(work, `clip-${crf}.mp4`);
      execFileSync(ffmpeg, ffmpegArgs(input, tmp, crf), { timeout: TOOL_TIMEOUT_MS, stdio: ["ignore", "ignore", "pipe"] });
      const size = statSync(tmp).size;
      const probe = probeFile(ffprobe, tmp);
      last = clipProblems(probe, size);
      if (last.length === 0) {
        copyFileSync(tmp, output);
        const v = probe.streams[0];
        return { problems: [], crf, size, seconds: durationOf(probe), width: v.width };
      }
      // Only the size can get better with a higher CRF; any other problem stops here.
      if (!last.every((p) => p.startsWith("size "))) break;
    }
    return { problems: last };
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

function parseArgs(argv) {
  const [command, ...rest] = argv;
  const opts = {};
  for (let i = 0; i < rest.length; i += 2) {
    const key = rest[i];
    const value = rest[i + 1];
    if (!key?.startsWith("--") || value === undefined) throw new Error(`bad argument ${key}`);
    opts[key.slice(2)] = value;
  }
  return { command, opts };
}

function main(argv) {
  let parsed;
  try {
    parsed = parseArgs(argv);
  } catch (e) {
    console.error(`record.mjs: ${e.message}`);
    return 2;
  }
  const { command, opts } = parsed;
  if (command === "stable" && opts.recorded && opts.stored) {
    const problems = stabilityProblems(readFileSync(opts.recorded, "utf8"), readFileSync(opts.stored, "utf8"));
    for (const p of problems) console.error(`record.mjs: ${p}`);
    if (problems.length === 0) console.log(`stable: ${path.basename(opts.recorded)} equals ${opts.stored}`);
    return problems.length === 0 ? 0 : 1;
  }
  if (command === "clip" && opts.in && opts.out) {
    const result = makeClip({ input: opts.in, output: opts.out, ffmpeg: opts.ffmpeg ?? "ffmpeg", ffprobe: opts.ffprobe ?? "ffprobe" });
    for (const p of result.problems) console.error(`record.mjs: ${p}`);
    if (result.problems.length > 0) return 1;
    console.log(`clip: ${opts.out} ${result.size} bytes, ${result.seconds} s, ${result.width} px, crf ${result.crf}`);
    return 0;
  }
  console.error("usage: record.mjs stable --recorded <run.jsonl> --stored <run-1.jsonl> | clip --in <raw> --out <clip.mp4> [--ffmpeg <path>] [--ffprobe <path>]");
  return 2;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    console.error(`record.mjs: ${e.message}`);
    process.exitCode = 1;
  }
}
