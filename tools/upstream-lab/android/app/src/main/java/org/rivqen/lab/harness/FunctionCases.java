// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.content.Context;

import com.tencent.sonic.sdk.RivqenLabAccess;
import com.tencent.sonic.sdk.SonicConfig;
import com.tencent.sonic.sdk.SonicConstants;
import com.tencent.sonic.sdk.SonicEngine;
import com.tencent.sonic.sdk.SonicSession;
import com.tencent.sonic.sdk.SonicSessionConfig;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Function cases of CONTRACT section 7 against the unmodified SDK: no session flow, no network.
 * A function that the SDK has no callable member for is {@code unsupported} with the reason.
 */
final class FunctionCases {

    static final String PLATFORM = "android";
    private static final String CASE_URL_PREFIX = Lab.ORIGIN_PREFIX + "fn/";

    private final Context context;
    private LabRuntime runtime;
    private int sessionNumber;

    FunctionCases(Context context) {
        this.context = context;
    }

    /** Runs every case of {@code input}; returns {cases, unsupported, errors}. */
    int[] run(File input, File output) throws Exception {
        JSONArray cases = new JSONArray(new String(readBounded(input), StandardCharsets.UTF_8));
        if (cases.length() > Lab.MAX_FUNCTION_CASES) {
            throw new IOException("over " + Lab.MAX_FUNCTION_CASES + " cases");
        }
        runtime = new LabRuntime(context, Events.NONE, RunConfig.DEFAULT_ACCOUNT, null, true);
        Threads.onMain(() -> SonicEngine.createInstance(runtime, new SonicConfig.Builder().build()), 10_000);
        int unsupported = 0;
        int errors = 0;
        File tmp = new File(output.getPath() + ".tmp");
        try (Writer out = new OutputStreamWriter(new FileOutputStream(tmp), StandardCharsets.UTF_8)) {
            for (int i = 0; i < cases.length(); i++) {
                JSONObject c = cases.getJSONObject(i);
                JSONObject line = new JSONObject();
                line.put("fn", c.optString("fn"));
                line.put("id", c.optString("id"));
                line.put("platform", PLATFORM);
                String reason = unsupportedReason(c.optString("fn"));
                if (reason != null) {
                    line.put("unsupported", reason);
                    unsupported++;
                } else {
                    try {
                        line.put("result", call(c.optString("fn"), c.getJSONObject("input")));
                    } catch (Exception e) {
                        line.put("error", Lab.describe(e));
                        errors++;
                    }
                }
                out.write(Json.write(sorted(line)));
                out.write('\n');
            }
        }
        if (!tmp.renameTo(output)) {
            throw new IOException("cannot write " + output);
        }
        return new int[] {cases.length(), unsupported, errors};
    }

    /** Functions with no callable member in the SDK (no reflection; CONTRACT section 7). */
    private static String unsupportedReason(String fn) {
        switch (fn) {
            case "is_legacy_response":
                return "private method SonicServer.isSonicResponse (upstream:sonic-android/sdk/src/main/java/"
                        + "com/tencent/sonic/sdk/SonicServer.java:189); not callable without reflection";
            case "template_change":
                return "inline code of SonicSession.handleFlow_Connection (upstream:sonic-android/sdk/src/main/java/"
                        + "com/tencent/sonic/sdk/SonicSession.java:772-776); no callable function";
            default:
                return null;
        }
    }

    private JSONObject call(String fn, JSONObject in) throws Exception {
        switch (fn) {
            case "session_id": {
                runtime.setAccount(in.isNull("account") ? null : in.getString("account"));
                String id = runtime.makeSessionId(in.getString("url"), in.getBoolean("account_related"));
                return Json.object("id", id);
            }
            case "split": {
                String[] r = RivqenLabAccess.split("rq-fn-split", in.getString("html"));
                if (!"true".equals(r[2])) {
                    throw new IllegalStateException("separateTemplateAndData returned false");
                }
                return Json.object("data", new JSONObject(r[1]), "template", r[0]);
            }
            case "diff": {
                JSONObject local = in.isNull("local") ? null : in.getJSONObject("local");
                return Json.object("diff", RivqenLabAccess.diff(local, in.getJSONObject("server")));
            }
            case "message":
                return message(in);
            case "etag": {
                Map<String, List<String>> fields = new LinkedHashMap<>();
                if (!in.isNull("value")) {
                    fields.put("eTag", Collections.singletonList(in.getString("value")));
                }
                SonicSession s = probeSession(fields);
                try {
                    return Json.object("value", RivqenLabAccess.etagAfterConnect(s));
                } finally {
                    s.destroy();
                }
            }
            case "charset":
                return Json.object("value", RivqenLabAccess.charset(in.isNull("content_type") ? null : in.getString("content_type")));
            default:
                throw new IllegalArgumentException("unknown fn: " + fn);
        }
    }

    /**
     * The page message of {@code SonicSession.setResult}. A diff gets {@code local_refresh_time} =
     * now when it has none, as the data update flow of the SDK adds it; the SDK then reports the
     * age in milliseconds (0 is left out), so that member can differ between runs.
     */
    private JSONObject message(JSONObject in) throws Exception {
        String pending = null;
        if (in.has("diff") && !in.isNull("diff")) {
            JSONObject diff = new JSONObject(in.getJSONObject("diff").toString());
            if (!diff.has("local_refresh_time")) {
                diff.put("local_refresh_time", System.currentTimeMillis());
            }
            pending = diff.toString();
        }
        SonicSession s = probeSession(fields(in.opt("fields")));
        try {
            return Json.object("json", RivqenLabAccess.message(s, in.getInt("src_code"), in.getInt("final_code"), pending));
        } finally {
            s.destroy();
        }
    }

    /** A session that never starts, with a fixed-answer connection; one URL per probe. */
    private SonicSession probeSession(Map<String, List<String>> fields) {
        SonicSessionConfig config = new SonicSessionConfig.Builder()
                .setSessionMode(SonicConstants.SESSION_MODE_QUICK)
                .setAutoStartWhenCreate(false)
                .setConnectionInterceptor(FixedResponseConnection.interceptor(fields))
                .build();
        SonicSession s = SonicEngine.getInstance().createSession(CASE_URL_PREFIX + (++sessionNumber), config);
        if (s == null) {
            throw new IllegalStateException("createSession returned null");
        }
        return s;
    }

    /** {@code fields}: an object (string or string array values) or an array of [name, value] pairs. */
    private static Map<String, List<String>> fields(Object value) throws JSONException {
        Map<String, List<String>> out = new LinkedHashMap<>();
        if (value instanceof JSONObject) {
            JSONObject o = (JSONObject) value;
            for (Iterator<String> it = o.keys(); it.hasNext(); ) {
                String k = it.next();
                Object v = o.get(k);
                List<String> values = new ArrayList<>();
                if (v instanceof JSONArray) {
                    for (int i = 0; i < ((JSONArray) v).length(); i++) {
                        values.add(((JSONArray) v).getString(i));
                    }
                } else {
                    values.add(String.valueOf(v));
                }
                out.put(k, values);
            }
        } else if (value instanceof JSONArray) {
            JSONArray a = (JSONArray) value;
            for (int i = 0; i < a.length(); i++) {
                JSONArray pair = a.getJSONArray(i);
                String k = pair.getString(0);
                List<String> values = out.get(k);
                if (values == null) {
                    values = new ArrayList<>();
                    out.put(k, values);
                }
                values.add(pair.getString(1));
            }
        } else if (value != null && value != JSONObject.NULL) {
            throw new IllegalArgumentException("fields must be an object or an array of pairs");
        }
        return out;
    }

    /** Top-level members in name order; nested objects keep the order the SDK returned. */
    private static JSONObject sorted(JSONObject o) throws JSONException {
        List<String> keys = new ArrayList<>();
        for (Iterator<String> it = o.keys(); it.hasNext(); ) {
            keys.add(it.next());
        }
        Collections.sort(keys);
        JSONObject out = new JSONObject();
        for (String k : keys) {
            out.put(k, o.get(k));
        }
        return out;
    }

    private static byte[] readBounded(File f) throws IOException {
        if (f.length() > Lab.MAX_FUNCTION_FILE) {
            throw new IOException(f.getName() + " over " + Lab.MAX_FUNCTION_FILE + " bytes");
        }
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buf = new byte[8192];
        try (InputStream in = new FileInputStream(f)) {
            int n;
            while ((n = in.read(buf)) != -1) {
                if (out.size() + n > Lab.MAX_FUNCTION_FILE) {
                    throw new IOException(f.getName() + " over " + Lab.MAX_FUNCTION_FILE + " bytes");
                }
                out.write(buf, 0, n);
            }
        }
        return out.toByteArray();
    }
}
