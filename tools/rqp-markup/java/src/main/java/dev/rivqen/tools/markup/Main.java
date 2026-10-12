// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.io.BufferedWriter;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStreamWriter;
import java.io.PrintStream;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryIteratorException;
import java.nio.file.DirectoryStream;
import java.nio.file.FileSystemException;
import java.nio.file.Files;
import java.nio.file.InvalidPathException;
import java.nio.file.Path;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.function.Function;

/**
 * Command line of the Java candidate ({@code tools/rqp-markup/CONTRACT.md} §2).
 *
 * <pre>
 * rqp-markup &lt;input.html&gt;
 * rqp-markup --batch &lt;fixtures-dir&gt;
 * </pre>
 *
 * <p>Exit codes: 0 when results are printed (valid and invalid documents), 2 for a usage or I/O
 * error, 3 for an internal error (a defect). For a defect in the analysis of one document, the
 * command still prints a fail-closed result for it ({@code RQP_MARKUP_STRUCTURE}, CONTRACT.md §5),
 * goes on with the other documents of a batch, and exits with code 3. Messages on stderr never
 * contain document content.
 */
public final class Main {
  static final int EXIT_OK = 0;
  static final int EXIT_USAGE_OR_IO = 2;
  static final int EXIT_INTERNAL = 3;

  private static final String USAGE =
      "usage: rqp-markup <input.html> | rqp-markup --batch <fixtures-dir>";

  private Main() {}

  /**
   * Entry point. No exception escapes: an unexpected one is reported by its class name only.
   *
   * @param args the arguments
   */
  public static void main(String[] args) {
    PrintStream err = System.err;
    int code;
    try {
      Writer out =
          new BufferedWriter(
              new OutputStreamWriter(
                  new FileOutputStream(FileDescriptor.out), StandardCharsets.UTF_8));
      code = run(args, out, err);
    } catch (Throwable t) {
      // The outermost boundary: never print a message, it could contain input data.
      err.println("rqp-markup: internal error (" + t.getClass().getName() + ")");
      code = EXIT_INTERNAL;
    }
    err.flush();
    System.exit(code);
  }

  /**
   * Runs one command.
   *
   * @param args the arguments
   * @param out result lines; flushed before return
   * @param err messages for the user
   * @return the exit code
   */
  static int run(String[] args, Writer out, PrintStream err) {
    return run(args, out, err, Analyzer::analyze);
  }

  /**
   * Runs one command with another analysis (tests give one with a defect).
   *
   * @param analyzer the analysis of one document; it never throws
   * @return the exit code
   */
  static int run(String[] args, Writer out, PrintStream err, Function<byte[], Result> analyzer) {
    try {
      int defects;
      if (args.length == 1 && !args[0].startsWith("--")) {
        Result r = analyzer.apply(read(Path.of(args[0])));
        out.write(ResultJson.format(r, null));
        out.write('\n');
        defects = r.internalError() ? 1 : 0;
      } else if (args.length == 2 && args[0].equals("--batch")) {
        defects = batch(Path.of(args[1]), out, analyzer);
      } else {
        err.println(USAGE);
        return EXIT_USAGE_OR_IO;
      }
      out.flush();
      if (defects > 0) {
        // The count only: the rule text has the exception class, never a message or content.
        err.println("rqp-markup: internal error in " + defects + " document(s)");
        return EXIT_INTERNAL;
      }
      return EXIT_OK;
    } catch (IOException | InvalidPathException | DirectoryIteratorException e) {
      // The class name only: a message can contain a path or input data.
      err.println("rqp-markup: I/O error (" + e.getClass().getSimpleName() + ")");
      return EXIT_USAGE_OR_IO;
    }
  }

  /** Prints one result line for each fixture; returns the number of internal errors. */
  private static int batch(Path dir, Writer out, Function<byte[], Result> analyzer)
      throws IOException {
    List<Path> fixtures = new ArrayList<>();
    try (DirectoryStream<Path> entries = Files.newDirectoryStream(dir)) {
      for (Path p : entries) {
        if (Files.isDirectory(p)) {
          fixtures.add(p);
        }
      }
    }
    fixtures.sort((a, b) -> Arrays.compareUnsigned(nameBytes(a), nameBytes(b)));
    int defects = 0;
    for (Path p : fixtures) {
      Path input = p.resolve("input.html");
      if (!Files.exists(input)) {
        // Only a missing input.html is skipped. Any other entry is checked by read().
        continue;
      }
      Result r = analyzer.apply(read(input));
      out.write(ResultJson.format(r, String.valueOf(p.getFileName())));
      out.write('\n');
      defects += r.internalError() ? 1 : 0;
    }
    return defects;
  }

  private static byte[] nameBytes(Path p) {
    return String.valueOf(p.getFileName()).getBytes(StandardCharsets.UTF_8);
  }

  /**
   * Reads at most {@link Analyzer#MAX_INPUT_BYTES} + 1 bytes: enough to decide M-01 without reading
   * a larger file into memory.
   */
  static byte[] read(Path path) throws IOException {
    // Check the type before the open: opening a FIFO waits for a writer (CONTRACT.md §2 item 7).
    // Symbolic links are followed. A narrow window remains between this check and the open.
    if (!Files.readAttributes(path, BasicFileAttributes.class).isRegularFile()) {
      throw new FileSystemException(null, null, "not a regular file");
    }
    try (InputStream s = Files.newInputStream(path)) {
      return s.readNBytes(Analyzer.MAX_INPUT_BYTES + 1);
    }
  }
}
