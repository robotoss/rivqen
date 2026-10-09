// Test harness: serves fixture pages; the UNMODIFIED upstream SonicFilter (jar) wraps it.
import java.io.IOException;
import java.nio.file.*;
import javax.servlet.http.*;
public class PageServlet extends HttpServlet {
  @Override protected void doGet(HttpServletRequest req, HttpServletResponse resp) throws IOException {
    String name = String.valueOf(req.getPathInfo()).replaceAll("[^a-z]", "");
    Path f = Paths.get("/pages/" + name + ".html");
    if (!Files.isRegularFile(f)) { resp.setStatus(404); return; }
    resp.setContentType("text/html;charset=utf-8");
    resp.getOutputStream().write(Files.readAllBytes(f));
  }
}
