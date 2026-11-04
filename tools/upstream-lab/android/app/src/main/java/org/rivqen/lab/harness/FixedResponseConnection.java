// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.content.Intent;

import com.tencent.sonic.sdk.SonicSession;
import com.tencent.sonic.sdk.SonicSessionConnection;
import com.tencent.sonic.sdk.SonicSessionConnectionInterceptor;

import java.io.BufferedInputStream;
import java.io.ByteArrayInputStream;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A connection with a fixed answer (status 200, the given fields, empty body) for the function
 * cases that need a server object: no network. It is given to the SDK through the public
 * connection interceptor of the session configuration.
 */
final class FixedResponseConnection extends SonicSessionConnection {

    private final Map<String, List<String>> fields;

    FixedResponseConnection(SonicSession session, Intent intent, Map<String, List<String>> fields) {
        super(session, intent);
        this.fields = Collections.unmodifiableMap(new LinkedHashMap<>(fields));
    }

    static SonicSessionConnectionInterceptor interceptor(final Map<String, List<String>> fields) {
        return new SonicSessionConnectionInterceptor() {
            @Override
            public SonicSessionConnection getConnection(SonicSession session, Intent intent) {
                return new FixedResponseConnection(session, intent, fields);
            }
        };
    }

    @Override
    protected int internalConnect() {
        return 0;
    }

    @Override
    protected BufferedInputStream internalGetResponseStream() {
        return new BufferedInputStream(new ByteArrayInputStream(new byte[0]));
    }

    /** The plain ETag field name: the custom ETag key of a response is not covered by the lab. */
    @Override
    protected String internalGetCustomHeadFieldEtag() {
        return CUSTOM_HEAD_FILED_ETAG;
    }

    @Override
    public void disconnect() {
        // No connection to close.
    }

    @Override
    public int getResponseCode() {
        return 200;
    }

    @Override
    public Map<String, List<String>> getResponseHeaderFields() {
        return fields;
    }

    @Override
    public String getResponseHeaderField(String key) {
        for (Map.Entry<String, List<String>> e : fields.entrySet()) {
            if (e.getKey().equalsIgnoreCase(key) && !e.getValue().isEmpty()) {
                return e.getValue().get(0);
            }
        }
        return null;
    }
}
