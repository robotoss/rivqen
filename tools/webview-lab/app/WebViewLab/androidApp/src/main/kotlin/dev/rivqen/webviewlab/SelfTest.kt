// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.webviewlab

import android.net.Uri
import org.json.JSONObject
import org.json.JSONTokener
import java.io.File

/**
 * Behavior checks of the input, URL and JSON code. They run on the device (`--ez selftest true`)
 * because the lab has no JVM test framework: JUnit 4 is EPL-1.0 (third-party.md, WebView lab).
 * `android-run.sh` runs them before every lab run and stops when one fails.
 */
internal object SelfTest {
    private class Check(val name: String, val body: () -> Boolean)

    private const val PORT = 18092
    private const val AT = "@"

    private fun throwsInput(body: () -> Unit): Boolean = try {
        body()
        false
    } catch (e: LabInputException) {
        true
    }

    private fun throwsValue(body: () -> Unit): Boolean = try {
        body()
        false
    } catch (e: ScriptValueException) {
        true
    }

    private fun lab(url: String): Boolean {
        val u = Uri.parse(url)
        return LabNet.isLabUrl(u.scheme, u.host, u.port, u.userInfo, PORT)
    }

    private fun caseJson(views: String, probe: String = "null", id: String = "NOSCRIPT"): String =
        """{"id":"$id","question":"Q-19","source":"x","headers":[],"views":$views,"probe":$probe}"""

    private fun roundTrip(s: String): Boolean {
        val q = LabJson.quote(s)
        return q.all { it in ' '..'~' } && JSONTokener(q).nextValue() == s
    }

    private fun tempFile(bytes: ByteArray): File {
        val f = File.createTempFile("selftest", ".json")
        f.writeBytes(bytes)
        return f
    }

    private fun readOk(bytes: ByteArray, max: Int): Boolean {
        val f = tempFile(bytes)
        try {
            LabInput.readBounded(f, max)
            return true
        } catch (e: LabInputException) {
            return false
        } finally {
            f.delete()
        }
    }

    private val checks: List<Check> = listOf(
        // Request rule (CONTRACT section 6).
        Check("lab url: case page") { lab("http://10.0.2.2:18092/c/NOSCRIPT") },
        Check("lab url: scheme and host case-insensitive") { lab("HTTP://10.0.2.2:18092/") },
        Check("lab url: https refused") { !lab("https://10.0.2.2:18092/") },
        Check("lab url: no port refused") { !lab("http://10.0.2.2/") },
        Check("lab url: other port refused") { !lab("http://10.0.2.2:18091/") },
        Check("lab url: other host refused") { !lab("http://example.com:18092/") },
        Check("lab url: localhost refused") { !lab("http://127.0.0.1:18092/") },
        // URLs with user information. AT is "@", split from the text so lint does not read a credential.
        Check("lab url: user info refused") { !lab("http://u" + AT + "10.0.2.2:18092/") },
        Check("lab url: host in user info refused") { !lab("http://10.0.2.2:18092" + AT + "example.com/") },
        Check("lab url: data refused") { !lab("data:text/html,x") },
        Check("lab url: trailing dot refused") { !lab("http://10.0.2.2.:18092/") },
        Check("lab origin") { LabNet.origin(PORT) == "http://10.0.2.2:18092" },
        Check("port bounds") { LabNet.isValidPort(1024) && LabNet.isValidPort(65535) && !LabNet.isValidPort(80) },

        // index.json.
        Check("index: two ids") { LabInput.parseIndex("""["A","B-1"]""") == listOf("A", "B-1") },
        Check("index: empty refused") { throwsInput { LabInput.parseIndex("[]") } },
        Check("index: object refused") { throwsInput { LabInput.parseIndex("{}") } },
        Check("index: duplicate refused") { throwsInput { LabInput.parseIndex("""["A","A"]""") } },
        Check("index: lower case refused") { throwsInput { LabInput.parseIndex("""["a"]""") } },
        Check("index: path refused") { throwsInput { LabInput.parseIndex("""["../X"]""") } },
        Check("index: long id refused") { throwsInput { LabInput.parseIndex("[\"" + "A".repeat(65) + "\"]") } },
        Check("index: number refused") { throwsInput { LabInput.parseIndex("[1]") } },
        Check("index: text after refused") { throwsInput { LabInput.parseIndex("""["A"] x""") } },
        Check("index: not JSON refused") { throwsInput { LabInput.parseIndex("[\"A\"") } },
        Check("index: deep nesting refused") { throwsInput { LabInput.parseIndex("[".repeat(5000)) } },
        Check("index: too many refused") {
            throwsInput { LabInput.parseIndex((0..LabInput.MAX_CASES).joinToString(",", "[", "]") { "\"A$it\"" }) }
        },

        // Case files.
        Check("case: two views") {
            LabInput.parseCase(caseJson("""["on","off"]"""), "NOSCRIPT") == LabCase("NOSCRIPT", listOf(View.ON, View.OFF), null)
        },
        Check("case: probe") {
            LabInput.parseCase(caseJson("""["on"]""", "\"#m\""), "NOSCRIPT").probe == "#m"
        },
        Check("case: id mismatch refused") { throwsInput { LabInput.parseCase(caseJson("""["on"]"""), "OTHER") } },
        Check("case: no views refused") { throwsInput { LabInput.parseCase(caseJson("[]"), "NOSCRIPT") } },
        Check("case: bad view refused") { throwsInput { LabInput.parseCase(caseJson("""["maybe"]"""), "NOSCRIPT") } },
        Check("case: twice refused") { throwsInput { LabInput.parseCase(caseJson("""["on","on"]"""), "NOSCRIPT") } },
        Check("case: three views refused") {
            throwsInput { LabInput.parseCase(caseJson("""["on","off","on"]"""), "NOSCRIPT") }
        },
        Check("case: empty probe refused") { throwsInput { LabInput.parseCase(caseJson("""["on"]""", "\"\""), "NOSCRIPT") } },
        Check("case: number probe refused") { throwsInput { LabInput.parseCase(caseJson("""["on"]""", "1"), "NOSCRIPT") } },
        Check("case: long probe refused") {
            throwsInput { LabInput.parseCase(caseJson("""["on"]""", "\"" + "p".repeat(257) + "\""), "NOSCRIPT") }
        },
        Check("case: array refused") { throwsInput { LabInput.parseCase("[]", "NOSCRIPT") } },

        // Bounded strict reads.
        Check("read: at the bound") { readOk(ByteArray(16) { 'a'.code.toByte() }, 16) },
        Check("read: above the bound refused") { !readOk(ByteArray(17) { 'a'.code.toByte() }, 16) },
        Check("read: malformed UTF-8 refused") { !readOk(byteArrayOf(0x61, 0xC3.toByte()), 16) },
        Check("read: missing file refused") { throwsInput { LabInput.readBounded(File("/nonexistent/x")) } },
        Check("json depth: brackets in strings ignored") { LabInput.jsonDepth("""["[[{\"]"]""") == 1 },

        // Result encoding.
        Check("quote: plain") { roundTrip("abc") },
        Check("quote: escapes") { roundTrip("q\"b\\n\n\r\t\u0000\u001f\u007f") },
        Check("quote: non-ASCII") { roundTrip("Grüße – 日本") },
        Check("quote: lone surrogate") { roundTrip("a\uD800b") },
        Check("encode: fields") {
            val r = LabResult("CT-UTF8", View.ON, LabResult.Walker.OK, "| <html>", "UTF-8", "text/html", "Grüße", null)
            val o = JSONObject(LabJson.encode(r))
            o.getString("id") == "CT-UTF8" && o.getString("view") == "on" && o.getString("walker") == "ok" &&
                o.getString("tree") == "| <html>" && o.getString("probe") == "Grüße" && o.isNull("error") &&
                o.length() == 8
        },
        Check("encode: unavailable") {
            val o = JSONObject(LabJson.encode(LabResult.unavailable("X", View.OFF, "why")))
            o.getString("walker") == "unavailable" && o.isNull("tree") && o.getString("error") == "why"
        },
        Check("result: ok needs a tree") {
            try {
                LabResult("X", View.ON, LabResult.Walker.OK, null, null, null, null, null)
                false
            } catch (e: IllegalArgumentException) {
                true
            }
        },

        // Script values.
        Check("value: null") { LabJson.decodeScriptString("null") == null && LabJson.decodeScriptString(null) == null },
        Check("value: string") { LabJson.decodeScriptString("\"a\\u00e9\\n\"") == "aé\n" },
        Check("value: number refused") { throwsValue { LabJson.decodeScriptString("42") } },
        Check("value: object refused") { throwsValue { LabJson.decodeScriptString("{}") } },
        Check("value: text after refused") { throwsValue { LabJson.decodeScriptString("\"a\" \"b\"") } },
        Check("meta: fields") {
            LabJson.decodeMeta(LabJson.quote("""{"characterSet":"UTF-8","contentType":"text/html","probe":null}""")) ==
                PageMeta("UTF-8", "text/html", null)
        },
        Check("meta: nested refused") {
            throwsValue { LabJson.decodeMeta(LabJson.quote("""{"characterSet":{"a":1}}""")) }
        },
        Check("meta: number refused") { throwsValue { LabJson.decodeMeta(LabJson.quote("""{"probe":1}""")) } },
        Check("meta script: probe is a literal") {
            val s = LabJson.metaScript("p\";alert(1);\"")
            val line = s.lines().first { it.trimStart().startsWith("var p = ") }
            val literal = line.trim().removePrefix("var p = ").removeSuffix(";")
            JSONTokener(literal).nextValue() == "p\";alert(1);\""
        },
        Check("meta script: no probe") { LabJson.metaScript(null).contains("var p = null;") },

        // Console channel of the lab world.
        Check("console: one chunk") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/1/3:xyz") && c.complete && c.payload() == "xyz"
        },
        Check("console: chunks in any order") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:1/2/5:de") && !c.complete && c.offer("rivqen-lab:ab:0/2/5:abc") &&
                c.payload() == "abcde"
        },
        Check("console: chunk text may hold colons") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/1/3:a:b") && c.payload() == "a:b"
        },
        Check("console: other text ignored") {
            val c = ConsoleCollector("ab")
            !c.offer("hello") && !c.offer("rivqen-lab:cd:0/1/1:x") && !c.complete && c.error == null
        },
        Check("console: length mismatch is an error") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/1/4:xyz") && c.error != null && c.payload() == null
        },
        Check("console: too much text is an error") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/2/2:xyz") && c.error != null
        },
        Check("console: duplicate chunk is an error") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/2/2:x") && c.offer("rivqen-lab:ab:0/2/2:x") && c.error != null
        },
        Check("console: changed header is an error") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/2/2:x") && c.offer("rivqen-lab:ab:1/3/2:y") && c.error != null
        },
        Check("console: index out of range is an error") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:2/2/2:x") && c.error != null
        },
        Check("console: bad numbers are an error") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:a/1/1:x") && c.error != null
        },
        Check("console: chunk count bound") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/${ConsoleCollector.MAX_CHUNKS + 1}/1:x") && c.error != null
        },
        Check("console: long chunk is an error") {
            val c = ConsoleCollector("ab")
            val n = ConsoleCollector.CHUNK_CHARS + 1
            c.offer("rivqen-lab:ab:0/1/$n:" + "x".repeat(n)) && c.error != null
        },
        Check("console: empty payload") {
            val c = ConsoleCollector("ab")
            c.offer("rivqen-lab:ab:0/1/0:") && c.complete && c.payload() == ""
        },
        Check("payload: fields") {
            LabJson.decodeWorldPayload(
                """{"bridge":false,"tree":"| <html>","characterSet":"UTF-8","contentType":"text/html","probe":null,"error":null}""",
            ) == LabJson.WorldPayload(false, "| <html>", "UTF-8", "text/html", null, null)
        },
        Check("payload: bridge must be a boolean") {
            throwsValue { LabJson.decodeWorldPayload("""{"bridge":"no","tree":null}""") }
        },
        Check("payload: nested refused") {
            throwsValue { LabJson.decodeWorldPayload("""{"bridge":true,"tree":["x"]}""") }
        },
        Check("payload: text after refused") {
            throwsValue { LabJson.decodeWorldPayload("""{"bridge":true} {}""") }
        },
        Check("world script: nonce and probe are literals") {
            val s = LabJson.worldScript("0f", "(function () { return \"t\"; })();", "a\"b")
            s.contains("var N = \"rivqen-lab:0f:\";") && s.contains("var p = \"a\\\"b\";") &&
                s.contains("var tree =\n(function () { return \"t\"; })();")
        },
        Check("guarded walker: tree") { LabJson.decodeGuarded(LabJson.quote("""["ok","| <html>"]""")) == "| <html>" },
        Check("guarded walker: error is reported") {
            try {
                LabJson.decodeGuarded(LabJson.quote("""["error","boom"]"""))
                false
            } catch (e: ScriptValueException) {
                e.message == "serialize.js failed: boom"
            }
        },
        Check("guarded walker: not run") { throwsValue { LabJson.decodeGuarded("null") } },
        Check("guarded walker: bad shape refused") { throwsValue { LabJson.decodeGuarded(LabJson.quote("""["ok"]""")) } },
        Check("guarded walker: bad kind refused") { throwsValue { LabJson.decodeGuarded(LabJson.quote("""["maybe","x"]""")) } },
        Check("guarded walker: walker text kept") {
            LabJson.guardedWalker("W();").contains("var tree =\nW();\n")
        },
        Check("world script: page facts before the walker") {
            val s = LabJson.worldScript("0f", "W();", null)
            s.indexOf("out.contentType = document.contentType") in 0 until s.indexOf("var tree =\nW();")
        },
        Check("world script: waits for the load event, no timer") {
            val s = LabJson.worldScript("0f", "W();", null)
            s.contains("window.addEventListener(\"load\", run, { once: true })") && !s.contains("setTimeout")
        },
        Check("world script: main frame only") {
            LabJson.worldScript("0f", "W();", null).contains("if (window !== window.top) return;")
        },
        Check("world script: bad nonce refused") {
            try {
                LabJson.worldScript("0f\"", "x", null)
                false
            } catch (e: IllegalArgumentException) {
                true
            }
        },
    )

    fun count(): Int = checks.size

    /** Runs every check. Returns the names of the failed checks (an exception is a failure). */
    fun run(): List<String> = checks.mapNotNull { c ->
        val ok = try {
            c.body()
        } catch (e: Exception) {
            false
        }
        if (ok) null else c.name
    }
}
