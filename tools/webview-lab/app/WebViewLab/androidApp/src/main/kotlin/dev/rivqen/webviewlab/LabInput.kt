// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.webviewlab

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import org.json.JSONTokener
import java.io.File
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction
import java.nio.charset.StandardCharsets

/** The scripting view of a run (CONTRACT section 2.1). */
internal enum class View(val wire: String) {
    ON("on"),
    OFF("off"),
    ;

    companion object {
        fun fromWire(s: String): View? = entries.firstOrNull { it.wire == s }
    }
}

/** The part of a case file that the app needs. The server sends the body and the headers. */
internal data class LabCase(val id: String, val views: List<View>, val probe: String?)

/** The cases in the order of `cases/index.json`, and the text of `host/serialize.js`. */
internal data class LabPlan(val cases: List<LabCase>, val walker: String)

/** The input files are missing, too large or malformed. The run stops; no result is written. */
internal class LabInputException(message: String, cause: Throwable? = null) : Exception(message, cause)

/**
 * Reads the lab input that `android-run.sh` copies into the app: `index.json`, `<id>.json`
 * (CONTRACT section 2.1) and `serialize.js`. The files come from outside the app, so every read
 * has a size bound and every value is checked.
 */
internal object LabInput {
    /** Bound for one input file. The largest case file is below 1 KiB, `serialize.js` below 4 KiB. */
    const val MAX_FILE_BYTES: Int = 64 * 1024

    /** Bound for the number of cases. */
    const val MAX_CASES: Int = 1000

    /** Bound for a probe selector. */
    const val MAX_PROBE_CHARS: Int = 256

    private val ID = Regex("^[A-Z0-9-]{1,64}$")

    fun isValidId(id: String): Boolean = ID.matches(id)

    /** Reads `dir/index.json`, each case file and `dir/serialize.js`. */
    fun load(dir: File): LabPlan {
        val ids = parseIndex(readBounded(File(dir, "index.json")))
        val cases = ids.map { id -> parseCase(readBounded(File(dir, "$id.json")), id) }
        val walker = readBounded(File(dir, "serialize.js"))
        if (walker.isBlank()) throw LabInputException("serialize.js is empty")
        return LabPlan(cases, walker)
    }

    /** Reads a file as strict UTF-8 with the bound [max]. */
    fun readBounded(file: File, max: Int = MAX_FILE_BYTES): String {
        val bytes = try {
            file.inputStream().use { input ->
                val buf = ByteArray(max + 1)
                var n = 0
                while (n < buf.size) {
                    val r = input.read(buf, n, buf.size - n)
                    if (r < 0) break
                    n += r
                }
                if (n > max) throw LabInputException("${file.name}: larger than $max bytes")
                buf.copyOf(n)
            }
        } catch (e: IOException) {
            throw LabInputException("${file.name}: cannot read", e)
        }
        return decodeUtf8(bytes, file.name)
    }

    /** Strict UTF-8: a malformed byte sequence is an error, not a replacement character. */
    fun decodeUtf8(bytes: ByteArray, where: String): String = try {
        StandardCharsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
            .decode(ByteBuffer.wrap(bytes))
            .toString()
    } catch (e: CharacterCodingException) {
        throw LabInputException("$where: not UTF-8", e)
    }

    /** `cases/index.json`: a JSON array of unique case ids (CONTRACT section 2.2). */
    fun parseIndex(text: String): List<String> {
        val arr = parseJson(text, "index.json") as? JSONArray
            ?: throw LabInputException("index.json: not a JSON array")
        if (arr.length() == 0) throw LabInputException("index.json: no cases")
        if (arr.length() > MAX_CASES) throw LabInputException("index.json: more than $MAX_CASES cases")
        val ids = ArrayList<String>(arr.length())
        for (i in 0 until arr.length()) {
            val id = arr.opt(i) as? String ?: throw LabInputException("index.json: item $i is not a string")
            if (!isValidId(id)) throw LabInputException("index.json: item $i is not a case id")
            ids.add(id)
        }
        if (ids.toSet().size != ids.size) throw LabInputException("index.json: an id occurs twice")
        return ids
    }

    /** `cases/<id>.json`: the id, the views and the probe (CONTRACT section 2.1). */
    fun parseCase(text: String, expectedId: String): LabCase {
        val where = "$expectedId.json"
        val o = parseJson(text, where) as? JSONObject ?: throw LabInputException("$where: not a JSON object")
        val id = o.opt("id") as? String ?: throw LabInputException("$where: no id")
        if (id != expectedId) throw LabInputException("$where: id does not match the file name")
        val rawViews = o.opt("views") as? JSONArray ?: throw LabInputException("$where: no views")
        if (rawViews.length() == 0 || rawViews.length() > View.entries.size) {
            throw LabInputException("$where: bad views")
        }
        val views = ArrayList<View>(rawViews.length())
        for (i in 0 until rawViews.length()) {
            val v = (rawViews.opt(i) as? String)?.let(View::fromWire)
                ?: throw LabInputException("$where: bad view")
            if (v in views) throw LabInputException("$where: a view occurs twice")
            views.add(v)
        }
        val p: Any? = o.opt("probe")
        val probe: String? = if (p == null || p == JSONObject.NULL) {
            null
        } else {
            if (p !is String || p.isEmpty() || p.length > MAX_PROBE_CHARS) throw LabInputException("$where: bad probe")
            p
        }
        return LabCase(id, views, probe)
    }

    /** Bound for the nesting of arrays and objects. A case file has depth 3. */
    const val MAX_JSON_DEPTH: Int = 8

    /**
     * The deepest nesting of `[` and `{` outside strings. The org.json parser recurses, so a
     * deep input must be refused before the parse.
     */
    fun jsonDepth(text: String): Int {
        var depth = 0
        var max = 0
        var inString = false
        var escaped = false
        for (c in text) {
            if (inString) {
                when {
                    escaped -> escaped = false
                    c == '\\' -> escaped = true
                    c == '"' -> inString = false
                }
                continue
            }
            when (c) {
                '"' -> inString = true
                '[', '{' -> {
                    depth++
                    if (depth > max) max = depth
                }
                ']', '}' -> depth--
            }
        }
        return max
    }

    private fun parseJson(text: String, where: String): Any {
        if (jsonDepth(text) > MAX_JSON_DEPTH) throw LabInputException("$where: nested deeper than $MAX_JSON_DEPTH")
        try {
            val t = JSONTokener(text)
            val v = t.nextValue()
            if (t.nextClean() != 0.toChar()) throw LabInputException("$where: text after the JSON value")
            return v
        } catch (e: JSONException) {
            throw LabInputException("$where: not JSON", e)
        }
    }
}
