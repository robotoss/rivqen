// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// parse5 is not a dependency of the lab. The lab takes the copy that tools/rqp-markup/crosscheck
// installs (`npm ci` there), resolved by path. WEBVIEW_LAB_PARSE5 can name another parse5 package directory.

import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { LabError, REPO_ROOT } from "./lab.mjs";

export const PARSE5_VERSION = "8.0.1";

/** @returns {string} the parse5 package directory */
export function parse5Dir() {
  return process.env.WEBVIEW_LAB_PARSE5 ?? join(REPO_ROOT, "tools", "rqp-markup", "crosscheck", "node_modules", "parse5");
}

/**
 * Load parse5 and check its version.
 * @returns {Promise<{ parse: (html: string, options?: object) => any }>}
 */
export async function loadParse5() {
  const dir = parse5Dir();
  const entry = join(dir, "dist", "index.js");
  if (!existsSync(entry)) {
    throw new LabError("PARSE5_MISSING", `parse5 not found at ${dir}. Run "npm ci" in tools/rqp-markup/crosscheck.`);
  }
  const pkg = JSON.parse(await readFile(join(dir, "package.json"), "utf8"));
  if (pkg.version !== PARSE5_VERSION) {
    throw new LabError("PARSE5_VERSION", `parse5 ${String(pkg.version)} found at ${dir}; the lab needs ${PARSE5_VERSION}`);
  }
  const mod = await import(pathToFileURL(entry).href);
  return { parse: mod.parse };
}
