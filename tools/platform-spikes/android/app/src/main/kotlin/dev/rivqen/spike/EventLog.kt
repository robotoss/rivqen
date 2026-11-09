// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spike

import android.os.SystemClock
import java.util.TreeMap

/**
 * The app part of the run log (CONTRACT.md section 5): one JSON object per event with sorted keys,
 * `src` `app`, `seq` from 0 and `t` in ms since the run start (app clock). Thread-safe; at most
 * [maxEvents] events, the rest are counted in [dropped] (the run script fails a run with drops).
 */
internal class EventLog(private val t0Nanos: Long, private val maxEvents: Int) {
    private val lock = Any()
    private val lines = ArrayList<String>()
    private val metrics = LinkedHashMap<String, Any?>()
    private var seq = 0
    private var droppedCount = 0

    /** Events that did not fit. */
    val dropped: Int
        get() = synchronized(lock) { droppedCount }

    /** Milliseconds since the run start. */
    fun now(): Long = (SystemClock.elapsedRealtimeNanos() - t0Nanos) / NANOS_PER_MS

    fun emit(ev: String, members: Map<String, Any?>) {
        synchronized(lock) { append(ev, members, now()) }
    }

    /**
     * Writes an event now with the time [t] of an earlier moment. For an event whose moment races
     * other app events (see `app.fetch.end` in [SpikeActivity]): the log order stays the same in
     * every run, and `t` still tells when it happened.
     */
    fun emitAt(ev: String, members: Map<String, Any?>, t: Long) {
        synchronized(lock) { append(ev, members, t) }
    }

    private fun append(ev: String, members: Map<String, Any?>, t: Long) {
        if (lines.size >= maxEvents) {
            droppedCount++
            return
        }
        val m = TreeMap<String, Any?>(members)
        m["ev"] = ev
        m["src"] = "app"
        m["seq"] = seq++
        m["t"] = t
        lines.add(Json.encode(m))
    }

    /** A timing number: an `app.probe` event with a `metric.` name and an entry of the metrics file. */
    fun metric(name: String, value: Number?) {
        require(name.startsWith("metric.")) { "a metric name starts with metric." }
        synchronized(lock) { metrics[name] = value }
        emit("app.probe", mapOf("name" to name, "value" to value))
    }

    fun lines(): List<String> = synchronized(lock) { ArrayList(lines) }

    fun metrics(): Map<String, Any?> = synchronized(lock) { LinkedHashMap(metrics) }

    private companion object {
        const val NANOS_PER_MS = 1_000_000L
    }
}

/** A small JSON writer with sorted object keys (the run script normalizes the form again). */
internal object Json {
    fun encode(value: Any?): String = StringBuilder().also { write(it, value) }.toString()

    private fun write(sb: StringBuilder, v: Any?) {
        when (v) {
            null, org.json.JSONObject.NULL -> sb.append("null")
            is String -> quote(sb, v)
            is Boolean -> sb.append(v)
            is Int, is Long, is Short, is Byte -> sb.append(v.toString())
            is Double -> number(sb, v)
            is Float -> number(sb, v.toDouble())
            is Map<*, *> -> {
                sb.append('{')
                var first = true
                for ((k, value) in v.entries.map { it.key.toString() to it.value }.sortedBy { it.first }) {
                    if (!first) sb.append(',')
                    first = false
                    quote(sb, k)
                    sb.append(':')
                    write(sb, value)
                }
                sb.append('}')
            }
            is Iterable<*> -> {
                sb.append('[')
                var first = true
                for (e in v) {
                    if (!first) sb.append(',')
                    first = false
                    write(sb, e)
                }
                sb.append(']')
            }
            is Array<*> -> write(sb, v.asList())
            else -> quote(sb, v.toString())
        }
    }

    private fun number(sb: StringBuilder, d: Double) {
        when {
            d.isNaN() || d.isInfinite() -> sb.append("null")
            d == Math.rint(d) && Math.abs(d) < 1e15 -> sb.append(d.toLong())
            else -> sb.append(d)
        }
    }

    private fun quote(sb: StringBuilder, s: String) {
        sb.append('"')
        for (c in s) {
            when (c) {
                '"' -> sb.append("\\\"")
                '\\' -> sb.append("\\\\")
                '\n' -> sb.append("\\n")
                '\r' -> sb.append("\\r")
                '\t' -> sb.append("\\t")
                else -> if (c < ' ' || c == ' ' || c == ' ') {
                    sb.append(String.format(java.util.Locale.ROOT, "\\u%04x", c.code))
                } else {
                    sb.append(c)
                }
            }
        }
        sb.append('"')
    }
}
