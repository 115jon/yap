package dev.yap.qtproto.crash;

import java.util.function.Predicate;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Strips a crash report's free text (an exception's message and stack, a
 * Rust panic's message, an ANR trace) of what could identify the user or
 * open their account: tokens and keys, email addresses, phone numbers, IP
 * addresses, ids, URL paths and queries, file paths under the user's
 * folders, and long quoted text (where a message or a name would sit). The
 * code's own names and line numbers stay; they are what the report is for.
 *
 * Pattern-based, so it cannot know every secret: the prompt shows the
 * report before anything is sent, and nothing is sent without the user.
 * Plain Java (no Android classes), so its tests run on any JVM
 * (android/test, native-mobile.yml).
 */
public final class Redactor {
    /** The longest line kept, and the longest text. */
    static final int MAX_LINE = 400;
    static final int MAX_TEXT = 32 * 1024;

    private Redactor() {}

    private static final class Rule {
        final Pattern pattern;
        final String replacement;

        Rule(String regex, String replacement) {
            this.pattern = Pattern.compile(regex);
            this.replacement = replacement;
        }
    }

    // In order: paths and URLs first (they hold the other shapes), then
    // secrets, then personal data, then whatever still looks opaque.
    private static final Rule[] RULES = {
        // content: URIs name the user's own files.
        new Rule("content://[^\\s\"'<>)]+", "content://<redacted>"),
        // URLs keep their scheme and host (the app's own servers); a
        // user:password goes, and the path and query, which hold ids,
        // invite codes and signed parameters.
        new Rule("\\b((?:https?|wss?)://)[^@/\\s\"'<>]*@", "$1"),
        new Rule("\\b((?:https?|wss?)://[^/\\s\"'<>?#]+)[/?#][^\\s\"'<>)]*", "$1/<path>"),
        // The app's private data and installed APK folders (the package
        // name stays, the random install folder goes).
        new Rule("/data/(?:user(?:_de)?/\\d+|data)/[\\w.]+", "<app>"),
        new Rule("/data/app/(?:~~[^/\\s]+/)?[^/\\s]+", "<apk>"),
        // Shared storage: everything after it, to the line's end or the
        // next colon or quote, is the user's own file name (spaces too).
        new Rule("(?:/storage/emulated/\\d+|/sdcard|/storage/[0-9A-F]{4}-[0-9A-F]{4})(?:/[^\\n:\"'<>]*)?",
                "<storage>"),
        // A build machine's or user's home folder (Rust panic locations).
        new Rule("(?:/home|/Users)/[^/\\s]+", "<home>"),
        new Rule("(?i)\\b[a-z]:[\\\\/]Users[\\\\/][^\\\\/\\s]+", "<home>"),
        // Credentials.
        new Rule("(?i)\\bbearer\\s+[^\\s\"',;]+", "Bearer <redacted>"),
        new Rule("\\beyJ[\\w-]+\\.[\\w-]+\\.[\\w-]*", "<jwt>"),
        // A cookie header to the line's end: every cookie in it.
        new Rule("(?i)\\b(cookie|set-cookie)(\\s*[=:]\\s*)[^\\n]*", "$1$2<redacted>"),
        new Rule("(?i)\\b(authorization|password|passwd|secret|"
                + "(?:access_|refresh_|session_|device_|id_)?token|api[_-]?key|key|sig|signature|"
                + "ticket|nonce|otp)(\"?\\s*[=:]\\s*\"?)[^\\s\"',;&)}\\]]+",
                "$1$2<redacted>"),
        // Personal data.
        new Rule("[\\w.+-]+@[\\w-]+(?:\\.[\\w-]+)+", "<email>"),
        new Rule("\\+\\d[\\d ()-]{7,}\\d", "<number>"),
        new Rule("\\b\\d{1,3}(?:\\.\\d{1,3}){3}\\b", "<ip>"),
        new Rule("(?i)\\b(?:[0-9a-f]{1,4}:){4,7}[0-9a-f]{1,4}\\b", "<ip>"),
        new Rule("(?i)\\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\\b", "<id>"),
        // Quoted text: where a message, a name or a payload sits. Every
        // double-quoted string goes (Java's and Rust's messages quote the
        // data), but an ANR trace's thread names ("main" stays, for the
        // stack under it). Single quotes and backticks mostly quote code
        // in messages (Rust's `Option::unwrap()`, a thread's 'name'), so
        // only long runs go. Plain classes, no alternation: a long quoted
        // run must not recurse the regex engine into a stack overflow.
        new Rule("\"(?!main\")[^\"\\n]+\"", "\"<text>\""),
        new Rule("'[^'\\n]{32,}'", "'<text>'"),
        new Rule("`[^`\\n]{32,}`", "`<text>`"),
    };

    /**
     * An IPv6 address written short (`fe80::1`, `[2606:4700::6810:84e5]`,
     * as Rust prints them): replaced only with a digit in it, so a code
     * path such as `dec::add` stays.
     */
    private static final Pattern IPV6_SHORT = Pattern.compile(
            "(?i)(?<![\\w:])(?:[0-9a-f]{1,4}:){1,7}:(?:[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){0,6})?(?![\\w:])");

    /**
     * Standard base64 (with `+` and `/`) of 40 characters or more that has
     * upper and lower case letters and digits: a key or a token without a
     * label. Paths rarely have all three without a dot.
     */
    private static final Pattern BASE64 = Pattern.compile("[A-Za-z0-9+/]{40,}={0,2}");

    /**
     * A stack frame (Java's `at ...`, a native `#00 pc ...` or `native:`
     * line): code names, build ids and D8's `$$ExternalSyntheticLambda2`
     * classes, never a token, so the opaque-run check skips it.
     */
    private static final Pattern FRAME = Pattern.compile("^\\s*(?:at |native: |#\\d+ pc )");

    /**
     * An opaque run (a token, a key, a hash): 24 or more letters, digits,
     * `_`, `-` and `=` with both letters and digits in it (base64url, hex,
     * and the pieces of standard base64 that BASE64 left). Mangled C++
     * and Rust symbols (`_Z...`, `_R...`) and stack frames stay.
     */
    private static final Pattern OPAQUE = Pattern.compile("[A-Za-z0-9_=-]{24,}");

    /** The free text, redacted, its lines and length capped. */
    public static String text(String raw) {
        if (raw == null || raw.isEmpty()) return "";
        String text = raw.length() > MAX_TEXT * 2 ? raw.substring(0, MAX_TEXT * 2) : raw;
        for (Rule rule : RULES) {
            text = rule.pattern.matcher(text).replaceAll(rule.replacement);
        }
        text = replaceIf(IPV6_SHORT, text, "<ip>", Redactor::hasDigit);
        StringBuilder out = new StringBuilder(text.length());
        boolean first = true;
        for (String line : text.split("\n", -1)) {
            if (!first) out.append('\n');
            first = false;
            if (!FRAME.matcher(line).find()) {
                line = replaceIf(BASE64, line, "<redacted>", Redactor::isBase64Key);
                line = replaceIf(OPAQUE, line, "<redacted>", Redactor::isOpaque);
            }
            out.append(line);
        }
        return cap(out.toString());
    }

    /**
     * A library's path in a native frame: its file name only (the folder
     * is the random install path, or the user's).
     */
    public static String fileName(String path) {
        if (path == null || path.isEmpty()) return "";
        int slash = path.lastIndexOf('/');
        String name = slash >= 0 ? path.substring(slash + 1) : path;
        // An APK's embedded library: "base.apk!libyapqt.so".
        int bang = name.lastIndexOf('!');
        return bang >= 0 ? name.substring(bang + 1) : name;
    }

    /** `text` with each match of `pattern` that `test` accepts replaced. */
    private static String replaceIf(Pattern pattern, String text, String replacement, Predicate<String> test) {
        Matcher m = pattern.matcher(text);
        StringBuilder out = new StringBuilder(text.length());
        int last = 0;
        while (m.find()) {
            if (test.test(m.group())) {
                out.append(text, last, m.start()).append(replacement);
                last = m.end();
            }
        }
        return out.append(text, last, text.length()).toString();
    }

    private static boolean hasDigit(String run) {
        for (int i = 0; i < run.length(); i++) {
            if (Character.isDigit(run.charAt(i))) return true;
        }
        return false;
    }

    private static boolean isBase64Key(String run) {
        boolean upper = false;
        boolean lower = false;
        boolean digit = false;
        for (int i = 0; i < run.length(); i++) {
            char c = run.charAt(i);
            if (c >= 'A' && c <= 'Z') upper = true;
            else if (c >= 'a' && c <= 'z') lower = true;
            else if (c >= '0' && c <= '9') digit = true;
        }
        return upper && lower && digit;
    }

    private static boolean isOpaque(String run) {
        if (run.startsWith("_Z") || run.startsWith("_R")) return false;
        boolean letter = false;
        boolean digit = false;
        for (int i = 0; i < run.length(); i++) {
            char c = run.charAt(i);
            if (Character.isDigit(c)) digit = true;
            else if (Character.isLetter(c)) letter = true;
        }
        return letter && digit;
    }

    private static String cap(String text) {
        StringBuilder out = new StringBuilder(Math.min(text.length(), MAX_TEXT) + 16);
        for (String line : text.split("\n", -1)) {
            if (out.length() > 0) out.append('\n');
            if (line.length() > MAX_LINE) {
                out.append(line, 0, MAX_LINE).append(" [cut]");
            } else {
                out.append(line);
            }
            if (out.length() >= MAX_TEXT) {
                out.setLength(MAX_TEXT);
                out.append("\n[cut]");
                break;
            }
        }
        return out.toString();
    }
}
