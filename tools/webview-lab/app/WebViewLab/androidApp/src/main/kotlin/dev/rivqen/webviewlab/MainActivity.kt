// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.webviewlab

import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.WindowManager
import android.webkit.WebSettings
import android.widget.FrameLayout
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.security.MessageDigest

/**
 * The lab activity. `android-run.sh` starts it with `adb shell am start` and these extras:
 *
 * - `port` (int, required): the port of the lab server on the host (`10.0.2.2:<port>`).
 * - `settleMs` (int, 0 to 2000, default 1000): the wait after `onPageFinished` (CONTRACT section 4 step 2).
 * - `selftest` (boolean): run the self-test of the input and output code only.
 *
 * Input: `files/in/` (`index.json`, `<id>.json`, `serialize.js`). Output: `files/out/results/<id>.<view>.json`,
 * `files/out/device.json`, and last `files/out/status.json`.
 */
class MainActivity : Activity() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val container = FrameLayout(this)
        setContentView(container)
        // One run per launch: a re-created activity does not start a second run.
        if (savedInstanceState != null) return
        val inDir = File(filesDir, "in")
        val outDir = File(filesDir, "out")
        val selftest = intent.getBooleanExtra(EXTRA_SELFTEST, false)
        val port = intent.getIntExtra(EXTRA_PORT, -1)
        val settleMs = intent.getIntExtra(EXTRA_SETTLE_MS, DEFAULT_SETTLE_MS).toLong()
        scope.launch {
            try {
                if (selftest) runSelfTest(outDir) else runLab(container, inDir, outDir, port, settleMs)
            } catch (e: CancellationException) {
                throw e
            } catch (e: LabInputException) {
                writeStatus(outDir, failed("input: ${e.message}"))
            } catch (e: IOException) {
                writeStatus(outDir, failed("output: ${e.message}"))
            } catch (e: IllegalArgumentException) {
                writeStatus(outDir, failed("arguments: ${e.message}"))
            } catch (e: RuntimeException) {
                // A defect or a WebView failure: record it in the status file instead of a crash.
                Log.e(LabRunner.TAG, "run failed", e)
                writeStatus(outDir, failed("internal: ${e.javaClass.simpleName}: ${e.message}"))
            } finally {
                finishAndRemoveTask()
            }
        }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private suspend fun runSelfTest(outDir: File) {
        val failures = SelfTest.run()
        val o = JSONObject()
            .put("state", if (failures.isEmpty()) "passed" else "failed")
            .put("checks", SelfTest.count())
            .put("failures", JSONArray(failures))
        withContext(Dispatchers.IO) {
            resetDir(outDir)
            writeText(File(outDir, "selftest.json"), o.toString(2) + "\n")
        }
        Log.i(LabRunner.TAG, "selftest: ${SelfTest.count()} checks, ${failures.size} failed")
    }

    private suspend fun runLab(container: FrameLayout, inDir: File, outDir: File, port: Int, settleMs: Long) {
        require(LabNet.isValidPort(port)) { "port must be ${LabNet.MIN_PORT}..${LabNet.MAX_PORT}" }
        require(settleMs in 0..LabRunner.MAX_SETTLE_MS) { "settleMs must be 0..${LabRunner.MAX_SETTLE_MS}" }
        if (Build.VERSION.SDK_INT >= LOCAL_NETWORK_SDK &&
            checkSelfPermission(PERMISSION_LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED
        ) {
            throw IllegalArgumentException("$PERMISSION_LOCAL_NETWORK is not granted (adb shell pm grant)")
        }
        val plan = withContext(Dispatchers.IO) { LabInput.load(inDir) }
        val resultsDir = File(outDir, "results")
        withContext(Dispatchers.IO) {
            resetDir(outDir)
            if (!resultsDir.mkdirs()) throw IOException("cannot create the results directory")
        }
        val runner = LabRunner(this, container, plan, port, settleMs)
        val notes = ArrayList<RowNote>()
        for (c in plan.cases) {
            for (view in c.views) {
                val row = runner.run(c, view)
                notes.add(row.note)
                withContext(Dispatchers.IO) {
                    writeText(File(resultsDir, "${c.id}.${view.wire}.json"), LabJson.encode(row.result))
                }
                Log.i(LabRunner.TAG, "${c.id} ${view.wire}: walker ${row.result.walker.wire}")
            }
        }
        val device = deviceFacts(plan, port, settleMs, runner.worldSupported, notes)
        withContext(Dispatchers.IO) { writeText(File(outDir, "device.json"), device.toString(2) + "\n") }
        val problems = notes.filter { it.loadProblem != null }
        val status = JSONObject()
            .put("state", if (problems.isEmpty()) "done" else "failed")
            .put("rows", notes.size)
            .put("ok", notes.count { it.walker == LabResult.Walker.OK })
            .put("unavailable", notes.count { it.walker == LabResult.Walker.UNAVAILABLE })
            .put("loadProblems", JSONArray(problems.map { "${it.id}.${it.view.wire}: ${it.loadProblem}" }))
        writeStatus(outDir, status)
    }

    private fun deviceFacts(
        plan: LabPlan,
        port: Int,
        settleMs: Long,
        worldSupported: Boolean,
        notes: List<RowNote>,
    ): JSONObject {
        val pkg = WebViewCompat.getCurrentWebViewPackage(this)
        val features = JSONObject()
        for (f in FEATURES) features.put(f, WebViewFeature.isFeatureSupported(f))
        val rows = JSONArray()
        for (n in notes) {
            rows.put(
                JSONObject()
                    .put("id", n.id)
                    .put("view", n.view.wire)
                    .put("walker", n.walker.wire)
                    .put("channel", n.channel)
                    .put("bridge", n.bridge ?: JSONObject.NULL)
                    .put("consoleCheck", n.consoleCheck)
                    .put("mainWorld", n.mainWorld)
                    .put("blocked", n.blocked)
                    .put("blockedOrigins", JSONArray(n.blockedOrigins))
                    .put("loadProblem", n.loadProblem ?: JSONObject.NULL),
            )
        }
        return JSONObject()
            .put("androidRelease", Build.VERSION.RELEASE)
            .put("sdkInt", Build.VERSION.SDK_INT)
            .put("fingerprint", Build.FINGERPRINT)
            .put("model", Build.MODEL)
            .put("webViewPackage", pkg?.packageName ?: JSONObject.NULL)
            .put("webViewVersion", pkg?.versionName ?: JSONObject.NULL)
            .put("userAgent", WebSettings.getDefaultUserAgent(this))
            .put("features", features)
            .put("labWorldSupported", worldSupported)
            .put("walkerSha256", sha256(plan.walker))
            .put("labOrigin", LabNet.origin(port))
            .put("settleMs", settleMs)
            .put("rows", rows)
    }

    private suspend fun writeStatus(outDir: File, status: JSONObject) {
        try {
            withContext(Dispatchers.IO) {
                if (!outDir.isDirectory && !outDir.mkdirs()) throw IOException("cannot create the output directory")
                writeText(File(outDir, "status.json"), status.toString(2) + "\n")
            }
            Log.i(LabRunner.TAG, "status: ${status.optString("state")}")
        } catch (e: IOException) {
            // Nothing more to do: the run script sees no status file and stops at its time limit.
            Log.e(LabRunner.TAG, "cannot write status.json", e)
        }
    }

    private fun failed(reason: String): JSONObject = JSONObject().put("state", "failed").put("error", reason)

    companion object {
        const val EXTRA_PORT: String = "port"
        const val EXTRA_SETTLE_MS: String = "settleMs"
        const val EXTRA_SELFTEST: String = "selftest"
        const val DEFAULT_SETTLE_MS: Int = 1000

        /** Android 17 (API 37) enforces the local network permission for apps that target it. */
        private const val LOCAL_NETWORK_SDK = 37
        private const val PERMISSION_LOCAL_NETWORK = "android.permission.ACCESS_LOCAL_NETWORK"

        private val FEATURES = listOf(
            WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD,
            WebViewFeature.WEB_MESSAGE_LISTENER,
            WebViewFeature.DOCUMENT_START_SCRIPT,
        )

        private fun sha256(s: String): String {
            val hex = "0123456789abcdef"
            val b = StringBuilder()
            for (byte in MessageDigest.getInstance("SHA-256").digest(s.toByteArray(Charsets.UTF_8))) {
                val v = byte.toInt() and 0xFF
                b.append(hex[v shr 4]).append(hex[v and 0xF])
            }
            return b.toString()
        }

        private fun resetDir(dir: File) {
            if (dir.exists() && !dir.deleteRecursively()) throw IOException("cannot delete ${dir.name}")
            if (!dir.mkdirs()) throw IOException("cannot create ${dir.name}")
        }

        /** Writes to a temporary file, then renames, so the run script never reads half a file. */
        private fun writeText(file: File, text: String) {
            val tmp = File(file.parentFile, file.name + ".tmp")
            tmp.writeText(text, Charsets.UTF_8)
            if (!tmp.renameTo(file)) throw IOException("cannot rename ${tmp.name}")
        }
    }
}
