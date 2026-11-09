// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import java.net.URI;
import okhttp3.HttpUrl;

/** JVM runner: `okhttp` (HttpUrl.toString()) or `uri` (URI.toASCIIString(), toString() in extra). */
public final class JvmMain {
    public static void main(String[] args) throws Exception {
        String jdk = "JDK " + System.getProperty("java.version") + " (" + System.getProperty("java.vendor") + ")";
        if (args[1].equals("okhttp")) {
            Common.run(args[0], "okhttp", "OkHttp 5.5.1 (okhttp-android aar classes, the artifact of the Android spike); API: HttpUrl.get(input) or HttpUrl.get(base).resolve(input), toString()",
                jdk + ", Okio 3.18.1", (input, base) -> {
                    HttpUrl u;
                    if (base == null) {
                        u = HttpUrl.get(input);
                    } else {
                        u = HttpUrl.get(base).resolve(input);
                        if (u == null) throw new IllegalArgumentException("resolve returned null");
                    }
                    return new String[] {u.toString(), u.host(), u.encodedPath(), u.encodedQuery()};
                });
        } else {
            Common.Parser p = new Common.Parser() {
                URI parseUri(String input, String base) throws Exception {
                    return base == null ? new URI(input) : new URI(base).resolve(new URI(input));
                }

                public String[] parse(String input, String base) throws Exception {
                    URI u = parseUri(input, base);
                    return new String[] {u.toASCIIString(), u.getHost(), u.getRawPath(), u.getRawQuery()};
                }

                public String extra(String input, String base) throws Exception {
                    URI u = parseUri(input, base);
                    return "{\"toString\":" + Common.jsonString(u.toString()) + ",\"rawAuthority\":"
                        + (u.getRawAuthority() == null ? "null" : Common.jsonString(u.getRawAuthority())) + "}";
                }
            };
            Common.run(args[0], "java-net-uri", jdk + "; API: new URI(input) or new URI(base).resolve(new URI(input)), toASCIIString()", jdk, p);
        }
    }
}
