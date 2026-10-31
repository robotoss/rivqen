// Test harness: serves fixture pages; the UNMODIFIED upstream SonicFilter (jar) wraps it.
//
// Two routes (web.xml):
//  - /p/<page>: a stored fixture page (traces S01-S21).
//  - every other path that ControlFilter did not take: the case that the runner stored through
//    the control endpoint (server/conformance/CONTRACT.md section 2).
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import javax.servlet.http.*;

public class PageServlet extends HttpServlet {
  private static boolean isPageRoute(HttpServletRequest req) {
    return "/p".equals(req.getServletPath());
  }

  @Override protected void doGet(HttpServletRequest req, HttpServletResponse resp) throws IOException {
    if (!isPageRoute(req)) { serveCase(resp); return; }
    String name = String.valueOf(req.getPathInfo()).replaceAll("[^a-z]", "");
    Path f = Paths.get("/pages/" + name + ".html");
    if (!Files.isRegularFile(f)) { resp.setStatus(404); return; }
    resp.setContentType("text/html;charset=utf-8");
    resp.getOutputStream().write(Files.readAllBytes(f));
  }

  @Override protected void doPost(HttpServletRequest req, HttpServletResponse resp) throws IOException, javax.servlet.ServletException {
    if (isPageRoute(req)) { super.doPost(req, resp); return; }
    serveCase(resp);
  }

  // What an application does: status, field lines in order, then the body bytes.
  // The upstream filter acts on it.
  private static void serveCase(HttpServletResponse resp) throws IOException {
    ControlFilter.StoredCase c = ControlFilter.current();
    if (c == null) {
      byte[] body = "{\"error\":\"NO_CASE\"}".getBytes(StandardCharsets.UTF_8);
      resp.setStatus(503);
      resp.setContentType("application/json");
      resp.getOutputStream().write(body);
      return;
    }
    resp.setStatus(c.status);
    for (String[] h : c.headers) resp.addHeader(h[0], h[1]);
    resp.getOutputStream().write(c.page);
  }
}
