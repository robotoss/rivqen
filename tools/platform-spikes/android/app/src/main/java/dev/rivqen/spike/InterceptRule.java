// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package dev.rivqen.spike;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.regex.Pattern;

/**
 * The interception rule of the prototype (android.md A-04) and the conversion of the fetched
 * response head into the parts of a {@code WebResourceResponse}. Java and {@code java.*} only, so
 * that a plain JVM check runs it ({@code check.sh}).
 *
 * <p>The WebView gets the early-fetch response only when all of these are true: main frame,
 * method {@code GET}, not a redirect, the URL is equal to the session URL (exact string), the
 * session origin is in the allow list, the early fetch has not been claimed yet (once only), and
 * the head can be expressed as a {@code WebResourceResponse}. Otherwise the request passes to the
 * WebView network stack.
 */
public final class InterceptRule {

    /** What the WebView gets for a request. */
    public enum Decision {
        /** The early-fetch response through the bridge pipe. */
        STREAM,
        /** No interception: the WebView loads the request itself. */
        PASS
    }

    /** A decision with the reason (for the metrics file). */
    public static final class Verdict {
        public final Decision decision;
        public final String reason;

        Verdict(Decision decision, String reason) {
            this.decision = decision;
            this.reason = reason;
        }
    }

    /** The parts of a {@code WebResourceResponse}. */
    public static final class Head {
        public final String mimeType;
        /** Charset name, or {@code null} when the Content-Type has none. */
        public final String encoding;
        public final int status;
        public final String reason;
        /** Fields for the WebView: one value per name, in the received order. */
        public final Map<String, String> fields;
        /** Names of fields that a single-value map cannot hold (for example a second Set-Cookie). */
        public final List<String> dropped;

        Head(String mimeType, String encoding, int status, String reason, Map<String, String> fields, List<String> dropped) {
            this.mimeType = mimeType;
            this.encoding = encoding;
            this.status = status;
            this.reason = reason;
            this.fields = fields;
            this.dropped = dropped;
        }
    }

    /** Fields that describe one connection, not the resource (RFC 9110 section 7.6.1), and the length of a re-framed body. */
    private static final Set<String> HOP_BY_HOP = new HashSet<String>(java.util.Arrays.asList(
            "connection", "keep-alive", "proxy-connection", "transfer-encoding", "te", "trailer", "upgrade",
            "content-length"));

    private static final Pattern TOKEN = Pattern.compile("[!#$%&'*+.^_`|~0-9A-Za-z-]+");
    private static final Pattern SYNTHETIC_PAIR = Pattern.compile("[a-z0-9-]{1,64}=[a-z0-9-]{0,64}");

    private final String sessionUrl;
    private final boolean originAllowed;
    private final AtomicBoolean claimed = new AtomicBoolean();

    /**
     * @param sessionUrl the URL of the early fetch, as given to {@code loadUrl}
     * @param sessionOrigin the origin of that URL from the parser of the HTTP stack ({@link #origin})
     * @param allowedOrigins origins that the prototype may intercept
     */
    public InterceptRule(String sessionUrl, String sessionOrigin, Set<String> allowedOrigins) {
        if (sessionUrl == null || sessionOrigin == null || allowedOrigins == null) {
            throw new NullPointerException("sessionUrl, sessionOrigin and allowedOrigins are required");
        }
        this.sessionUrl = sessionUrl;
        this.originAllowed = allowedOrigins.contains(sessionOrigin);
    }

    /**
     * Decides for one request. A STREAM verdict claims the early fetch: a later request gets PASS.
     *
     * @param method request method
     * @param url request URL as the WebView gives it
     * @param mainFrame the request is for the main frame
     * @param redirect the request is the result of a redirect
     */
    public Verdict decide(String method, String url, boolean mainFrame, boolean redirect) {
        if (!mainFrame) {
            return new Verdict(Decision.PASS, "not-main-frame");
        }
        if (!"GET".equals(method)) {
            return new Verdict(Decision.PASS, "method");
        }
        if (redirect) {
            return new Verdict(Decision.PASS, "redirect");
        }
        if (!sessionUrl.equals(url)) {
            return new Verdict(Decision.PASS, "url");
        }
        if (!originAllowed) {
            return new Verdict(Decision.PASS, "origin");
        }
        if (!claimed.compareAndSet(false, true)) {
            return new Verdict(Decision.PASS, "claimed");
        }
        return new Verdict(Decision.STREAM, "match");
    }

    /**
     * The origin string {@code scheme://host[:port]} with a lower-case scheme and host, brackets
     * around an IPv6 host, and no port when it is the default port of the scheme.
     */
    public static String origin(String scheme, String host, int port) {
        String s = scheme.toLowerCase(Locale.ROOT);
        String h = host.toLowerCase(Locale.ROOT);
        if (h.indexOf(':') >= 0 && !h.startsWith("[")) {
            h = "[" + h + "]";
        }
        boolean defaultPort = ("https".equals(s) && port == 443) || ("http".equals(s) && port == 80);
        return s + "://" + h + (defaultPort ? "" : ":" + port);
    }

    /**
     * Converts a fetched head. Returns {@code null} when the WebView cannot take it as a response
     * (the request then passes): a status outside [100, 299] and [400, 599] (a 3xx cannot be
     * returned), an empty reason phrase, or no usable Content-Type.
     *
     * @param status the status code
     * @param reason the reason phrase
     * @param pairs the fields as [name, value] pairs in the received order
     */
    public static Head head(int status, String reason, List<String[]> pairs) {
        if (!((status >= 100 && status <= 299) || (status >= 400 && status <= 599))) {
            return null;
        }
        if (reason == null || reason.trim().isEmpty()) {
            return null;
        }
        Set<String> connectionNames = new HashSet<String>();
        for (String[] p : pairs) {
            if ("connection".equalsIgnoreCase(p[0])) {
                for (String n : p[1].split(",")) {
                    connectionNames.add(n.trim().toLowerCase(Locale.ROOT));
                }
            }
        }
        Map<String, String> fields = new LinkedHashMap<String, String>();
        Map<String, String> lowerToName = new LinkedHashMap<String, String>();
        List<String> dropped = new ArrayList<String>();
        String contentType = null;
        for (String[] p : pairs) {
            String lower = p[0].toLowerCase(Locale.ROOT);
            if (HOP_BY_HOP.contains(lower) || connectionNames.contains(lower)) {
                continue;
            }
            if ("content-type".equals(lower) && contentType == null) {
                contentType = p[1];
            }
            String name = lowerToName.get(lower);
            if (name == null) {
                lowerToName.put(lower, p[0]);
                fields.put(p[0], p[1]);
            } else if ("set-cookie".equals(lower)) {
                // A Set-Cookie value can hold a comma (Expires), so two of them cannot be joined.
                dropped.add(p[0]);
            } else {
                fields.put(name, fields.get(name) + ", " + p[1]);
            }
        }
        String[] media = mediaType(contentType);
        if (media == null) {
            return null;
        }
        return new Head(media[0], media[1], status, reason, Collections.unmodifiableMap(fields),
                Collections.unmodifiableList(dropped));
    }

    /**
     * Splits a Content-Type value into the lower-case media type and the lower-case charset
     * (or {@code null}). Returns {@code null} when there is no valid {@code type/subtype}.
     */
    public static String[] mediaType(String value) {
        if (value == null) {
            return null;
        }
        String[] parts = value.split(";");
        String type = parts[0].trim().toLowerCase(Locale.ROOT);
        int slash = type.indexOf('/');
        if (slash <= 0 || slash == type.length() - 1
                || !TOKEN.matcher(type.substring(0, slash)).matches()
                || !TOKEN.matcher(type.substring(slash + 1)).matches()) {
            return null;
        }
        String charset = null;
        for (int i = 1; i < parts.length; i++) {
            String param = parts[i].trim();
            int eq = param.indexOf('=');
            if (eq > 0 && "charset".equalsIgnoreCase(param.substring(0, eq).trim())) {
                String v = param.substring(eq + 1).trim();
                if (v.length() >= 2 && v.startsWith("\"") && v.endsWith("\"")) {
                    v = v.substring(1, v.length() - 1);
                }
                if (!v.isEmpty() && TOKEN.matcher(v).matches()) {
                    charset = v.toLowerCase(Locale.ROOT);
                }
                break;
            }
        }
        return new String[] {type, charset};
    }

    /**
     * The value of a field as the log may hold it: a cookie or authorization value only when it is
     * synthetic (every cookie pair has a {@code [a-z0-9-]} name and value), otherwise
     * {@code [redacted]}.
     */
    public static String loggable(String name, String value) {
        String lower = name.toLowerCase(Locale.ROOT);
        if ("set-cookie".equals(lower)) {
            // The first pair is the cookie; the attributes after it hold no cookie value.
            int semi = value.indexOf(';');
            String pair = semi < 0 ? value : value.substring(0, semi);
            return SYNTHETIC_PAIR.matcher(pair.trim()).matches() ? value : "[redacted]";
        }
        if ("cookie".equals(lower)) {
            for (String pair : value.split(";", -1)) {
                if (!SYNTHETIC_PAIR.matcher(pair.trim()).matches()) {
                    return "[redacted]";
                }
            }
            return value;
        }
        if ("authorization".equals(lower) || "proxy-authorization".equals(lower)) {
            return "[redacted]";
        }
        return value;
    }
}
