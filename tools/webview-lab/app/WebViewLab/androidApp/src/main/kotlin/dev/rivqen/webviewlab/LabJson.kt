// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.webviewlab

import org.json.JSONException
import org.json.JSONObject
import org.json.JSONTokener

/** A script result that is not what the lab script returns. */
internal class ScriptValueException(message: String, cause: Throwable? = null) : Exception(message, cause)

/** One result file (CONTRACT section 4). */
internal data class LabResult(
    val id: String,
    val view: View,
    val walker: Walker,
    val tree: String?,
    val characterSet: String?,
    val contentType: String?,
    val probe: String?,
    val error: String?,
) {
    enum class Walker(val wire: String) { OK("ok"), UNAVAILABLE("unavailable") }

    init {
        require(walker != Walker.OK || tree != null) { "walker ok needs a tree" }
    }

    companion object {
        fun unavailable(id: String, view: View, reason: String): LabResult =
            LabResult(id, view, Walker.UNAVAILABLE, null, null, null, null, reason)
    }
}

/** The page facts that the meta script reads next to the tree. */
internal data class PageMeta(val characterSet: String?, val contentType: String?, val probe: String?)

internal object LabJson {
    /** Bound for one script value (a tree of the deepest case is below 1 MiB). */
    const val MAX_SCRIPT_VALUE_CHARS: Int = 32 * 1024 * 1024

    /**
     * A JSON string literal with every character outside printable ASCII as `\uXXXX`. The output
     * is ASCII, so a lone surrogate in page text survives the file write without a change.
     */
    fun quote(s: String): String {
        val b = StringBuilder(s.length + 2)
        b.append('"')
        for (c in s) {
            when {
                c == '"' -> b.append("\\\"")
                c == '\\' -> b.append("\\\\")
                c == '\n' -> b.append("\\n")
                c == '\r' -> b.append("\\r")
                c == '\t' -> b.append("\\t")
                c < ' ' || c > '~' -> b.append("\\u").append(HEX[c.code shr 12 and 0xF])
                    .append(HEX[c.code shr 8 and 0xF]).append(HEX[c.code shr 4 and 0xF]).append(HEX[c.code and 0xF])
                else -> b.append(c)
            }
        }
        b.append('"')
        return b.toString()
    }

    private const val HEX = "0123456789abcdef"

    private fun quoteOrNull(s: String?): String = if (s == null) "null" else quote(s)

    /** The result file text, keys in the order of CONTRACT section 4, with a final newline. */
    fun encode(r: LabResult): String = buildString {
        append("{\n")
        append("  \"id\": ").append(quote(r.id)).append(",\n")
        append("  \"view\": ").append(quote(r.view.wire)).append(",\n")
        append("  \"walker\": ").append(quote(r.walker.wire)).append(",\n")
        append("  \"tree\": ").append(quoteOrNull(r.tree)).append(",\n")
        append("  \"characterSet\": ").append(quoteOrNull(r.characterSet)).append(",\n")
        append("  \"contentType\": ").append(quoteOrNull(r.contentType)).append(",\n")
        append("  \"probe\": ").append(quoteOrNull(r.probe)).append(",\n")
        append("  \"error\": ").append(quoteOrNull(r.error)).append("\n")
        append("}\n")
    }

    /**
     * Decodes the value that `evaluateJavascript` or `JavaScriptReplyProxy.executeJavaScript`
     * gives: the JSON text of the script value. A string gives the string; `null` (also the
     * value `undefined`, and a script that did not run) gives null. Every other value is an error.
     */
    fun decodeScriptString(json: String?): String? {
        if (json == null) return null
        val t = json.trim()
        if (t == "null") return null
        if (t.length > MAX_SCRIPT_VALUE_CHARS + 2) throw ScriptValueException("script value is too large")
        if (!t.startsWith("\"") || !t.endsWith("\"") || t.length < 2) {
            throw ScriptValueException("script value is not a string: ${t.take(SNIPPET)}")
        }
        try {
            val tok = JSONTokener(t)
            // The first character is a quote, so nextValue reads one string and nothing nested.
            val v = tok.nextValue() as? String ?: throw ScriptValueException("script value is not a string")
            if (tok.nextClean() != 0.toChar()) throw ScriptValueException("text after the script value")
            return v
        } catch (e: JSONException) {
            throw ScriptValueException("script value is not a JSON string", e)
        }
    }

    /** The meta script returns `JSON.stringify({characterSet, contentType, probe})`. */
    fun decodeMeta(json: String?): PageMeta {
        val text = decodeScriptString(json) ?: throw ScriptValueException("meta script gave no value")
        if (LabInput.jsonDepth(text) != 1) throw ScriptValueException("meta value is not a flat object")
        val o = try {
            JSONTokener(text).nextValue() as? JSONObject
        } catch (e: JSONException) {
            throw ScriptValueException("meta value is not JSON", e)
        } ?: throw ScriptValueException("meta value is not an object")
        fun field(name: String): String? {
            val v: Any? = o.opt(name)
            if (v == null || v == JSONObject.NULL) return null
            return v as? String ?: throw ScriptValueException("meta $name is not a string")
        }
        return PageMeta(field("characterSet"), field("contentType"), field("probe"))
    }

    /**
     * The meta script. It runs in the lab world, reads three page facts and returns them as one
     * JSON string. The probe selector enters as a JSON literal, never as script text.
     */
    fun metaScript(probe: String?): String =
        "(function () {\n" +
            "  var p = ${quoteOrNull(probe)};\n" +
            "  var e = p === null ? null : document.querySelector(p);\n" +
            "  return JSON.stringify({ characterSet: document.characterSet, contentType: document.contentType," +
            " probe: e === null ? null : e.textContent });\n" +
            "})();"

    /**
     * Runs `serialize.js` and returns its error text as the value when it throws, so the app can
     * tell a walker failure from a script that did not run (value null). The value is
     * `["ok", tree]` or `["error", message]` as JSON text.
     */
    fun guardedWalker(walker: String): String =
        "(function () {\n" +
            "  try {\n" +
            "    var tree =\n" + walker + "\n" +
            "    return JSON.stringify(typeof tree === \"string\" ? [\"ok\", tree] : [\"error\", \"no string\"]);\n" +
            "  } catch (x) {\n" +
            "    return JSON.stringify([\"error\", String(x && x.message ? x.message : x)]);\n" +
            "  }\n" +
            "})();"

    /** Decodes the value of [guardedWalker]: the tree, or a [ScriptValueException] with the walker error. */
    fun decodeGuarded(json: String?): String {
        val text = decodeScriptString(json) ?: throw ScriptValueException("serialize.js did not run")
        if (LabInput.jsonDepth(text) != 1) throw ScriptValueException("walker value is not a flat array")
        val arr = try {
            JSONTokener(text).nextValue() as? org.json.JSONArray
        } catch (e: JSONException) {
            throw ScriptValueException("walker value is not JSON", e)
        } ?: throw ScriptValueException("walker value is not an array")
        val kind = arr.opt(0) as? String
        val value = arr.opt(1) as? String
        if (arr.length() != 2 || kind == null || value == null) throw ScriptValueException("walker value has a bad shape")
        return when (kind) {
            "ok" -> value
            "error" -> throw ScriptValueException("serialize.js failed: $value")
            else -> throw ScriptValueException("walker value has a bad kind")
        }
    }

    /**
     * The lab-world script. It is added for DOMContentLoaded in the main frame, in the
     * androidx.webkit world, and it runs also when content JavaScript is off. It:
     *
     * 1. sends the ready message when the message object exists (view `on` only, see [ConsoleCollector]);
     * 2. at the `load` event of the window (the end of the main-frame load, CONTRACT section 4
     *    step 2), reads the page facts and runs `serialize.js`;
     * 3. sends the result as ASCII JSON in console chunks (see [ConsoleCollector]).
     *
     * FACT (WebView 155, view off): in this world the `load` listener, microtasks and
     * `requestAnimationFrame` run; `setTimeout` callbacks do not. So the script waits for `load`
     * and uses no timer.
     *
     * `serialize.js` is a script whose value is the tree (CONTRACT section 4): an expression
     * statement after comments. It enters as `var tree =` and the script text, so no `eval` is
     * needed. The nonce and the probe enter as JSON literals.
     */
    fun worldScript(nonce: String, walker: String, probe: String?): String {
        require(nonce.all { it in '0'..'9' || it in 'a'..'f' } && nonce.isNotEmpty()) { "bad nonce" }
        return "(function () {\n" +
            "  if (window !== window.top) return;\n" +
            "  var N = ${quote("rivqen-lab:$nonce:")};\n" +
            "  var bridge = typeof ${LabRunner.BRIDGE} === \"object\" && ${LabRunner.BRIDGE} !== null;\n" +
            "  if (bridge) ${LabRunner.BRIDGE}.postMessage(${quote(LabRunner.READY)});\n" +
            "  var run = function () {\n" +
            "  var out = { bridge: bridge, tree: null, characterSet: null, contentType: null, probe: null, error: null };\n" +
            "  var fail = function (where, x) {\n" +
            "    out.error = (out.error === null ? \"\" : out.error + \"; \") + where + \": \" + String(x && x.message ? x.message : x);\n" +
            "  };\n" +
            // The page facts first: they stay in the result when the walker fails.
            "  try {\n" +
            "    var p = ${quoteOrNull(probe)};\n" +
            "    var e = p === null ? null : document.querySelector(p);\n" +
            "    out.characterSet = document.characterSet;\n" +
            "    out.contentType = document.contentType;\n" +
            "    out.probe = e === null ? null : e.textContent;\n" +
            "  } catch (x) {\n" +
            "    fail(\"page facts\", x);\n" +
            "  }\n" +
            "  try {\n" +
            "    var tree =\n" + walker + "\n" +
            "    if (typeof tree === \"string\") out.tree = tree; else fail(\"serialize.js\", \"no string\");\n" +
            "  } catch (x) {\n" +
            "    fail(\"serialize.js\", x);\n" +
            "  }\n" +
            "  var s = JSON.stringify(out).replace(/[\\u007f-\\uffff]/g, function (c) {\n" +
            "    return \"\\\\u\" + (\"000\" + c.charCodeAt(0).toString(16)).slice(-4);\n" +
            "  });\n" +
            "  var size = ${ConsoleCollector.CHUNK_CHARS};\n" +
            "  var n = Math.max(1, Math.ceil(s.length / size));\n" +
            "  for (var i = 0; i < n; i++) console.info(N + i + \"/\" + n + \"/\" + s.length + \":\" + s.slice(i * size, (i + 1) * size));\n" +
            "  };\n" +
            "  if (document.readyState === \"complete\") run(); else window.addEventListener(\"load\", run, { once: true });\n" +
            "})();\n"
    }

    /** The result of the lab-world script. */
    data class WorldPayload(
        val bridge: Boolean,
        val tree: String?,
        val characterSet: String?,
        val contentType: String?,
        val probe: String?,
        val error: String?,
    )

    /** Decodes the payload of [worldScript]: a flat JSON object. */
    fun decodeWorldPayload(text: String): WorldPayload {
        if (LabInput.jsonDepth(text) != 1) throw ScriptValueException("payload is not a flat object")
        val o = try {
            val t = JSONTokener(text)
            val v = t.nextValue()
            if (t.nextClean() != 0.toChar()) throw ScriptValueException("text after the payload")
            v as? JSONObject
        } catch (e: JSONException) {
            throw ScriptValueException("payload is not JSON", e)
        } ?: throw ScriptValueException("payload is not an object")
        fun field(name: String): String? {
            val v: Any? = o.opt(name)
            if (v == null || v == JSONObject.NULL) return null
            return v as? String ?: throw ScriptValueException("payload $name is not a string")
        }
        val bridge = o.opt("bridge") as? Boolean ?: throw ScriptValueException("payload bridge is not a boolean")
        return WorldPayload(bridge, field("tree"), field("characterSet"), field("contentType"), field("probe"), field("error"))
    }

    private const val SNIPPET = 80
}
