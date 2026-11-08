// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;

/** Shared by the JVM and the Android runner: input lines in, raw record lines out (see lib.mjs). */
final class Common {
    private Common() {}

    static final class Input {
        final String id;
        final String input;
        final String base;

        Input(String id, String input, String base) {
            this.id = id;
            this.input = input;
            this.base = base;
        }
    }

    interface Parser {
        /** Returns {href, host, path, query}; throws when the stack refuses the input. */
        String[] parse(String input, String base) throws Exception;

        /** Extra members as a JSON object text, or null. */
        default String extra(String input, String base) throws Exception {
            return null;
        }
    }

    static String enc(String s) {
        return s == null ? "~" : Base64.getEncoder().encodeToString(s.getBytes(StandardCharsets.UTF_8));
    }

    static String dec(String s) {
        return s.equals("~") ? null : new String(Base64.getDecoder().decode(s), StandardCharsets.UTF_8);
    }

    static List<Input> readInputs(String file) throws IOException {
        List<Input> out = new ArrayList<>();
        for (String line : Files.readAllLines(Paths.get(file), StandardCharsets.UTF_8)) {
            if (line.isEmpty()) continue;
            String[] p = line.split("\t", -1);
            out.add(new Input(p[0], dec(p[1]), dec(p[2])));
        }
        return out;
    }

    static String jsonString(String s) {
        StringBuilder b = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"' || c == '\\') b.append('\\').append(c);
            else if (c < 0x20 || c > 0x7e) b.append(String.format("\\u%04x", (int) c));
            else b.append(c);
        }
        return b.append('"').toString();
    }

    static void run(String file, String stack, String version, String runtime, Parser parser) throws IOException {
        StringBuilder out = new StringBuilder();
        out.append("H\tstack\t").append(enc(stack)).append('\n');
        out.append("H\tversion\t").append(enc(version)).append('\n');
        out.append("H\truntime\t").append(enc(runtime)).append('\n');
        for (Input in : readInputs(file)) {
            String[] r = null;
            String error = null;
            try {
                r = parser.parse(in.input, in.base);
            } catch (Throwable t) {
                error = t.getClass().getName();
            }
            out.append("R\t").append(in.id).append('\t').append(r != null ? "1" : "0");
            for (int i = 0; i < 4; i++) out.append('\t').append(enc(r != null ? r[i] : null));
            out.append('\t').append(enc(error));
            String extra = null;
            if (r != null) {
                try {
                    extra = parser.extra(in.input, in.base);
                } catch (Throwable t) {
                    extra = null;
                }
            }
            out.append('\t').append(enc(extra)).append('\n');
        }
        System.out.write(out.toString().getBytes(StandardCharsets.UTF_8), 0, out.toString().getBytes(StandardCharsets.UTF_8).length);
        System.out.flush();
    }
}
