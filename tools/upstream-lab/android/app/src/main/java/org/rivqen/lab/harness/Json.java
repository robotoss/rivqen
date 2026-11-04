// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.util.JsonWriter;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.IOException;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/** JSON and hash helpers of the harness. */
final class Json {

    private Json() {
    }

    /** Builds an object from name and value pairs; a {@code null} value is JSON {@code null}. */
    static JSONObject object(Object... pairs) {
        if (pairs.length % 2 != 0) {
            throw new IllegalArgumentException("pairs must be name and value");
        }
        JSONObject o = new JSONObject();
        try {
            for (int i = 0; i < pairs.length; i += 2) {
                Object value = pairs[i + 1];
                o.put((String) pairs[i], value == null ? JSONObject.NULL : value);
            }
        } catch (JSONException e) {
            throw new IllegalArgumentException(e);
        }
        return o;
    }

    /** Header map as a list of {@code [name, value]} pairs sorted by name, then value; {@code null} stays null. */
    static Object headerPairs(Map<String, String> headers) {
        if (headers == null) {
            return JSONObject.NULL;
        }
        List<String[]> pairs = new ArrayList<>();
        for (Map.Entry<String, String> e : headers.entrySet()) {
            pairs.add(new String[] {String.valueOf(e.getKey()), String.valueOf(e.getValue())});
        }
        Collections.sort(pairs, (a, b) -> {
            int c = a[0].compareTo(b[0]);
            return c != 0 ? c : a[1].compareTo(b[1]);
        });
        JSONArray out = new JSONArray();
        for (String[] p : pairs) {
            JSONArray pair = new JSONArray();
            pair.put(p[0]);
            pair.put(p[1]);
            out.put(pair);
        }
        return out;
    }

    static MessageDigest sha256() {
        try {
            return MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    static String hex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            sb.append(Character.forDigit((b >> 4) & 0xf, 16)).append(Character.forDigit(b & 0xf, 16));
        }
        return sb.toString();
    }

    static String sha256Utf8(String text) {
        return hex(sha256().digest(text.getBytes(StandardCharsets.UTF_8)));
    }

    /**
     * Writes a value as compact JSON in the key order of the value (org.json objects keep insertion
     * order on Android). No {@code \/} escape, unlike {@code JSONObject.toString()}.
     */
    static String write(Object value) {
        StringWriter out = new StringWriter();
        try (JsonWriter w = new JsonWriter(out)) {
            w.setLenient(true);
            writeValue(w, value);
        } catch (IOException | JSONException e) {
            throw new IllegalStateException(e);
        }
        return out.toString();
    }

    private static void writeValue(JsonWriter w, Object value) throws IOException, JSONException {
        if (value == null || value == JSONObject.NULL) {
            w.nullValue();
        } else if (value instanceof JSONObject) {
            JSONObject o = (JSONObject) value;
            w.beginObject();
            Iterator<String> keys = o.keys();
            while (keys.hasNext()) {
                String k = keys.next();
                w.name(k);
                writeValue(w, o.get(k));
            }
            w.endObject();
        } else if (value instanceof JSONArray) {
            JSONArray a = (JSONArray) value;
            w.beginArray();
            for (int i = 0; i < a.length(); i++) {
                writeValue(w, a.get(i));
            }
            w.endArray();
        } else if (value instanceof Boolean) {
            w.value((Boolean) value);
        } else if (value instanceof Integer || value instanceof Long) {
            w.value(((Number) value).longValue());
        } else if (value instanceof Number) {
            w.value((Number) value);
        } else {
            w.value(String.valueOf(value));
        }
    }
}
