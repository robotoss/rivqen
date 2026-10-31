// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Control endpoint of the upstream lab (server/conformance/CONTRACT.md section 2, with the lab
// exception of DL-081). TEST CODE ONLY: it runs in a lab container whose port is published on
// 127.0.0.1 of the host, and it is off unless RIVQEN_CONFORMANCE=1.
//
// This filter is listed BEFORE the upstream SonicFilter in web.xml and does not call the chain
// for a control path, so the upstream code never sees a control request. Case requests go on to
// PageServlet, which serves the stored case.
import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import com.google.gson.stream.JsonReader;
import com.google.gson.stream.JsonToken;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.StringReader;
import java.math.BigDecimal;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Base64;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import javax.servlet.Filter;
import javax.servlet.FilterChain;
import javax.servlet.FilterConfig;
import javax.servlet.ServletException;
import javax.servlet.ServletRequest;
import javax.servlet.ServletResponse;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;

public class ControlFilter implements Filter {
  static final String PROFILE = "legacy-java";
  static final String CONTROL_PREFIX = "/__rivqen/conformance/";
  static final int MAX_CASE_BYTES = 8 * 1024 * 1024;
  private static final boolean CONFORMANCE = "1".equals(System.getenv("RIVQEN_CONFORMANCE"));
  private static final String HOST_PORT = System.getenv("LAB_HOST_PORT") == null ? "" : System.getenv("LAB_HOST_PORT");

  /** The stored case: immutable once built. */
  static final class StoredCase {
    final String caseId;
    final byte[] page;
    final int status;
    final List<String[]> headers;

    StoredCase(String caseId, byte[] page, int status, List<String[]> headers) {
      this.caseId = caseId;
      this.page = page;
      this.status = status;
      this.headers = Collections.unmodifiableList(headers);
    }
  }

  /** Result of parseCase: exactly one of value and error is set. */
  static final class Parsed {
    final StoredCase value;
    final String error; // BAD_CASE or UNSUPPORTED

    Parsed(StoredCase value, String error) {
      this.value = value;
      this.error = error;
    }
  }

  private static final Object LOCK = new Object();
  private static StoredCase stored; // guarded by LOCK; one case at a time

  static StoredCase current() {
    synchronized (LOCK) {
      return stored;
    }
  }

  private static void store(StoredCase c) {
    synchronized (LOCK) {
      stored = c;
    }
  }

  @Override
  public void init(FilterConfig config) {}

  @Override
  public void destroy() {}

  @Override
  public void doFilter(ServletRequest request, ServletResponse response, FilterChain chain) throws IOException, ServletException {
    if (!(request instanceof HttpServletRequest) || !(response instanceof HttpServletResponse)) {
      chain.doFilter(request, response);
      return;
    }
    HttpServletRequest req = (HttpServletRequest) request;
    HttpServletResponse resp = (HttpServletResponse) response;
    String path = req.getRequestURI();
    if (path == null || !path.startsWith(CONTROL_PREFIX)) {
      chain.doFilter(request, response);
      return;
    }
    try {
      control(req, resp, path);
    } catch (IOException | RuntimeException e) {
      System.err.println("control error: " + e.getClass().getSimpleName());
      if (!resp.isCommitted()) {
        sendJson(resp, 500, "{\"error\":\"HARNESS_FAILED\"}");
      }
    }
  }

  private void control(HttpServletRequest req, HttpServletResponse resp, String path) throws IOException {
    if (!CONFORMANCE) {
      sendJson(resp, 404, "{\"error\":\"NOT_FOUND\"}");
      return;
    }
    // Lab exception (DL-081): the peer is the bridge address, so check Host and Content-Type only.
    if (!isControlHost(req.getHeader("Host"), HOST_PORT)) {
      sendJson(resp, 403, "{\"error\":\"FORBIDDEN\"}");
      return;
    }
    if (path.equals(CONTROL_PREFIX + "info")) {
      if (!"GET".equals(req.getMethod())) {
        sendJson(resp, 405, "{\"error\":\"METHOD_NOT_ALLOWED\"}");
        return;
      }
      sendJson(resp, 200, "{\"contract\":1,\"suites\":[\"legacy\"],\"profiles\":[\"" + PROFILE + "\"]}");
      return;
    }
    if (path.equals(CONTROL_PREFIX + "case")) {
      if (!"POST".equals(req.getMethod())) {
        sendJson(resp, 405, "{\"error\":\"METHOD_NOT_ALLOWED\"}");
        return;
      }
      if (!isJsonContentType(req.getHeader("Content-Type"))) {
        sendJson(resp, 415, "{\"error\":\"UNSUPPORTED_MEDIA_TYPE\"}");
        return;
      }
      byte[] body = readBody(req.getInputStream(), MAX_CASE_BYTES);
      if (body == null) {
        sendJson(resp, 413, "{\"error\":\"TOO_LARGE\"}");
        return;
      }
      Parsed parsed = parseCase(new String(body, StandardCharsets.UTF_8), PROFILE);
      if ("BAD_CASE".equals(parsed.error)) {
        sendJson(resp, 400, "{\"error\":\"BAD_CASE\"}");
        return;
      }
      if (parsed.value == null) {
        sendJson(resp, 422, "{\"error\":\"UNSUPPORTED\"}");
        return;
      }
      store(parsed.value);
      resp.setStatus(204);
      return;
    }
    sendJson(resp, 404, "{\"error\":\"NOT_FOUND\"}");
  }

  private static void sendJson(HttpServletResponse resp, int status, String json) throws IOException {
    byte[] bytes = json.getBytes(StandardCharsets.UTF_8);
    resp.setStatus(status);
    resp.setContentType("application/json");
    resp.setContentLength(bytes.length);
    resp.getOutputStream().write(bytes);
  }

  /** Reads the whole body; null when it is larger than limit (the rest is read and dropped). */
  static byte[] readBody(InputStream in, int limit) throws IOException {
    ByteArrayOutputStream out = new ByteArrayOutputStream();
    byte[] buf = new byte[8192];
    long size = 0;
    int n;
    while ((n = in.read(buf)) >= 0) {
      size += n;
      if (size <= limit) {
        out.write(buf, 0, n);
      }
    }
    return size > limit ? null : out.toByteArray();
  }

  /** Host of a control request: a loopback name with the PUBLISHED port of the lab. Fails closed. */
  static boolean isControlHost(String host, String port) {
    if (host == null || port == null || !port.matches("[0-9]{1,5}")) {
      return false;
    }
    String h = host.toLowerCase(Locale.ROOT);
    return h.equals("127.0.0.1:" + port) || h.equals("localhost:" + port) || h.equals("[::1]:" + port);
  }

  /** Media type application/json (ASCII case-insensitive), parameters allowed. */
  static boolean isJsonContentType(String value) {
    if (value == null) {
      return false;
    }
    int semi = value.indexOf(';');
    String type = (semi < 0 ? value : value.substring(0, semi)).trim();
    return type.toLowerCase(Locale.ROOT).equals("application/json");
  }

  private static boolean isString(JsonElement e) {
    return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isString();
  }

  private static boolean isToken(String s) {
    if (s.isEmpty()) {
      return false;
    }
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      boolean ok = (c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || "!#$%&'*+-.^_`|~".indexOf(c) >= 0;
      if (!ok) {
        return false;
      }
    }
    return true;
  }

  private static boolean isFieldValue(String s) {
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      if (!(c == '\t' || (c >= 0x20 && c <= 0x7e) || (c >= 0x80 && c <= 0xff))) {
        return false;
      }
    }
    return true;
  }

  private static boolean isBase64Text(String s) {
    if (s.length() % 4 != 0) {
      return false;
    }
    int end = s.length();
    int pad = 0;
    while (end > 0 && s.charAt(end - 1) == '=' && pad < 3) {
      end--;
      pad++;
    }
    if (pad > 2) {
      return false;
    }
    for (int i = 0; i < end; i++) {
      char c = s.charAt(i);
      boolean ok = (c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '+' || c == '/';
      if (!ok) {
        return false;
      }
    }
    return true;
  }

  /** Strict JSON: no lenient syntax, no trailing text. Throws on any defect. */
  private static JsonElement parseStrict(String text) throws IOException {
    JsonReader reader = new JsonReader(new StringReader(text));
    reader.setLenient(false);
    JsonElement element = new Gson().getAdapter(JsonElement.class).read(reader);
    if (reader.peek() != JsonToken.END_DOCUMENT) {
      throw new IOException("trailing text");
    }
    return element;
  }

  private static final Parsed BAD = new Parsed(null, "BAD_CASE");
  private static final Parsed UNSUPPORTED = new Parsed(null, "UNSUPPORTED");

  /** Parse a case body. BAD_CASE (invalid JSON or member) is checked before UNSUPPORTED. */
  static Parsed parseCase(String text, String profile) {
    JsonElement root;
    try {
      root = parseStrict(text);
    } catch (IOException | RuntimeException | StackOverflowError e) {
      return BAD;
    }
    if (root == null || !root.isJsonObject()) {
      return BAD;
    }
    JsonObject json = root.getAsJsonObject();
    JsonElement suiteEl = json.get("suite");
    if (!isString(suiteEl)) {
      return BAD;
    }
    String suite = suiteEl.getAsString();
    if (!suite.equals("rqp") && !suite.equals("legacy")) {
      return BAD;
    }
    if (!isString(json.get("case_id"))) {
      return BAD;
    }
    if (suite.equals("legacy") && !isString(json.get("profile"))) {
      return BAD;
    }
    JsonElement pageEl = json.get("page_base64");
    if (!isString(pageEl) || !isBase64Text(pageEl.getAsString())) {
      return BAD;
    }
    byte[] page;
    try {
      page = Base64.getDecoder().decode(pageEl.getAsString());
    } catch (IllegalArgumentException e) {
      return BAD;
    }
    JsonElement appEl = json.get("app");
    if (appEl == null || !appEl.isJsonObject()) {
      return BAD;
    }
    JsonObject app = appEl.getAsJsonObject();
    JsonElement statusEl = app.get("status");
    if (statusEl == null || !statusEl.isJsonPrimitive() || !statusEl.getAsJsonPrimitive().isNumber()) {
      return BAD;
    }
    int status;
    try {
      BigDecimal n = new BigDecimal(statusEl.getAsJsonPrimitive().getAsString());
      if (n.stripTrailingZeros().scale() > 0 || n.compareTo(BigDecimal.valueOf(100)) < 0 || n.compareTo(BigDecimal.valueOf(599)) > 0) {
        return BAD;
      }
      status = n.intValueExact();
    } catch (NumberFormatException | ArithmeticException e) {
      return BAD;
    }
    JsonElement headersEl = app.get("headers");
    if (headersEl == null || !headersEl.isJsonArray()) {
      return BAD;
    }
    List<String[]> headers = new ArrayList<>();
    JsonArray pairs = headersEl.getAsJsonArray();
    for (JsonElement pairEl : pairs) {
      if (!pairEl.isJsonArray() || pairEl.getAsJsonArray().size() != 2) {
        return BAD;
      }
      JsonArray pair = pairEl.getAsJsonArray();
      if (!isString(pair.get(0)) || !isString(pair.get(1))) {
        return BAD;
      }
      String name = pair.get(0).getAsString();
      String value = pair.get(1).getAsString();
      if (!isToken(name) || !isFieldValue(value)) {
        return BAD;
      }
      headers.add(new String[] {name, value});
    }
    if (json.has("server_config") && !json.get("server_config").isJsonObject()) {
      return BAD;
    }
    if (!suite.equals("legacy") || !profile.equals(json.get("profile").getAsString())) {
      return UNSUPPORTED;
    }
    return new Parsed(new StoredCase(json.get("case_id").getAsString(), page, status, headers), null);
  }
}
