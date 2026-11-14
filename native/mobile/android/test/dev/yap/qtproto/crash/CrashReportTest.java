package dev.yap.qtproto.crash;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * The crash report's redaction and tombstone reader, on any JVM (no JUnit,
 * no Android): native-mobile.yml compiles these with the two classes and
 * runs main(), which exits 1 on a failure.
 *
 *   javac -d out android/src/dev/yap/qtproto/crash/{Redactor,Tombstone}.java \
 *       android/test/dev/yap/qtproto/crash/CrashReportTest.java
 *   java -cp out dev.yap.qtproto.crash.CrashReportTest
 */
public final class CrashReportTest {
    private static final List<String> failures = new ArrayList<>();
    private static int passed;

    public static void main(String[] args) {
        redactorShouldRemoveCredentials();
        redactorShouldRemovePersonalData();
        redactorShouldRemovePersonalPaths();
        redactorShouldKeepTheCodesOwnNames();
        redactorShouldRemoveQuotedDataAndShortForms();
        redactorShouldKeepFramesAndAnrThreadNames();
        redactorShouldCapLongText();
        fileNameShouldDropTheFolder();
        tombstoneShouldReadTheCrashingThread();
        tombstoneShouldRejectGarbage();
        for (String failure : failures) System.err.println("FAIL " + failure);
        System.out.println(passed + " passed, " + failures.size() + " failed");
        if (!failures.isEmpty()) System.exit(1);
    }

    private static void check(String name, boolean ok, String detail) {
        if (ok) passed++;
        else failures.add(name + ": " + detail);
    }

    private static void absent(String name, String text, String... secrets) {
        for (String secret : secrets) check(name, !text.contains(secret), "'" + secret + "' left in: " + text);
    }

    private static void present(String name, String text, String... kept) {
        for (String k : kept) check(name, text.contains(k), "'" + k + "' missing from: " + text);
    }

    static void redactorShouldRemoveCredentials() {
        String out = Redactor.text("Authorization: Bearer abc.def-ghi\nheader bearer xyz.1-2\n"
                + "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzcGlrZSJ9.c2lnbmF0dXJl\n"
                + "token=Zm9vYmFy session_token: \"s3cr3t\" {\"refresh_token\":\"r1\"}\n"
                + "api_key=AKIA1234 password=hunter2 ticket: tk-9\n"
                + "opaque Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MGFiY2RlZg end");
        absent("credentials", out, "abc.def-ghi", "xyz.1-2", "eyJhbGciOiJIUzI1NiJ9", "Zm9vYmFy", "s3cr3t", "r1\"",
                "AKIA1234", "hunter2", "tk-9", "Nzg5MGFiY2RlZg");
        present("credentials", out, "Authorization: <redacted>", "header Bearer <redacted>", "<jwt>", "token=<redacted>", "opaque <redacted> end");
    }

    static void redactorShouldRemovePersonalData() {
        String out = Redactor.text("from spike@example.com via 192.0.2.7 and 2001:db8:0:0:0:0:0:1\n"
                + "channel 123e4567-e89b-12d3-a456-426614174000 call +1 (555) 010-9999\n"
                + "GET https://user:pw@yap.example/api/channels/42/messages?before=9&sig=x#frag\n"
                + "wss://gw.yap.example/socket?ticket=abc\n"
                + "JSON: \"hello there, this is a private message from a friend\" and 'another long "
                + "quoted run of someone else words here'\n"
                + "content://media/external/images/media/77");
        absent("personal", out, "spike@example.com", "192.0.2.7", "2001:db8", "123e4567", "555", "user:pw",
                "/api/channels", "before=9", "frag", "ticket=abc", "private message", "someone else",
                "media/77");
        present("personal", out, "<email>", "<ip>", "<id>", "https://yap.example/<path>",
                "wss://gw.yap.example/<path>", "\"<text>\"", "content://<redacted>");
    }

    static void redactorShouldRemovePersonalPaths() {
        String out = Redactor.text("at /data/user/0/dev.yap.app/files/session.bin\n"
                + "at /data/data/dev.yap.app/cache/x\n"
                + "lib /data/app/~~Qk3x==/dev.yap.app-Zz9_==/lib/arm64/libyapqt_arm64-v8a.so\n"
                + "photo /storage/emulated/0/DCIM/Camera/IMG_holiday_mum.jpg: open failed\n"
                + "panicked at /home/runner/.cargo/registry/src/index/x-1.0/src/lib.rs:3:9\n"
                + "built in C:\\Users\\jon\\src\\yap and /Users/jon/src\n");
        absent("paths", out, "/data/user/0", "/data/data", "~~Qk3x", "Zz9_", "DCIM", "holiday", "runner",
                "jon");
        present("paths", out, "<app>/files/session.bin", "<apk>/lib/arm64/libyapqt_arm64-v8a.so",
                "<storage>: open failed", "<home>/.cargo/registry", "<home>\\src\\yap");
    }

    static void redactorShouldKeepTheCodesOwnNames() {
        String stack = "java.lang.IllegalStateException: Expected BEGIN_OBJECT\n"
                + "\tat dev.yap.qtproto.NativeShim.lambda$install$0(NativeShim.java:142)\n"
                + "\tat org.qtproject.qt.android.QtNative.startQtApplication(QtNative.java:520)\n"
                + "\tat _ZN4core9panicking5panic17h1a2b3c4d5e6f7a8bE\n"
                + "panic on thread 'tokio-runtime-worker' at src/client.rs:120:9\n"
                + "called `Option::unwrap()` on a `None` value";
        String out = Redactor.text(stack);
        check("code names", out.equals(stack), "changed: " + out);
    }

    static void redactorShouldRemoveQuotedDataAndShortForms() {
        String out = Redactor.text("For input string: \"Sam Doe\"\n"
                + "invalid type: string \"see you at 8\", expected u64\n"
                + "open /storage/emulated/0/Download/My Tax Return.pdf failed\n"
                + "connect [2606:4700::6810:84e5]:443 and fe80::1ff:fe23:4567:890a\n"
                + "Cookie: yap.sid=shortvalue; other=abc\n"
                + "key Zm9vYmFyYmF6cXV4+MTIzNDU2Nzg5/MGFiY2RlZmdoaWprbG1ub3BxcnN0==\n"
                + "long \"" + "y".repeat(5000) + "\" end");
        absent("quoted", out, "Sam Doe", "see you", "Download", "Tax Return", "2606:4700", "fe80", "shortvalue",
                "other=abc", "Zm9vYmFy", "yyyy");
        present("quoted", out, "For input string: \"<text>\"", "<storage>", "[<ip>]:443", "Cookie: <redacted>",
                "key <redacted>", "long \"<text>\" end");
    }

    static void redactorShouldKeepFramesAndAnrThreadNames() {
        String trace = "\"main\" prio=5 tid=1 Native\n"
                + "  native: #00 pc 000a1b2c  /apex/com.android.runtime/lib64/bionic/libc.so (__epoll_pwait+8) "
                + "(BuildId: 5d1a2b3c4d5e6f708192a3b4c5d6e7f8)\n"
                + "  at dev.yap.qtproto.crash.CrashReports$$ExternalSyntheticLambda2.run(D8$$SyntheticClass:0)\n"
                + "  at android.os.Handler.handleCallback(Handler.java:959)\n"
                + "  at std::vector, dec::add";
        String out = Redactor.text(trace);
        check("frames", out.equals(trace), "changed: " + out);
    }

    static void redactorShouldCapLongText() {
        String out = Redactor.text("x".repeat(Redactor.MAX_LINE + 50) + "\nshort");
        check("cap line", out.startsWith("x".repeat(Redactor.MAX_LINE) + " [cut]\nshort"), out.substring(0, 20));
        String many = ("line of text\n").repeat(10_000);
        check("cap text", Redactor.text(many).length() <= Redactor.MAX_TEXT + 8, "too long");
        check("empty", Redactor.text(null).isEmpty(), "null");
    }

    static void fileNameShouldDropTheFolder() {
        check("file name", Redactor.fileName("/data/app/~~a/dev.yap.app-b/lib/arm64/libyapqt.so")
                .equals("libyapqt.so"), "folder kept");
        check("apk entry", Redactor.fileName("/data/app/~~a/base.apk!libc++_shared.so")
                .equals("libc++_shared.so"), "apk kept");
    }

    // ---- A hand-made tombstone --------------------------------------------

    private static final class Proto {
        final ByteArrayOutputStream out = new ByteArrayOutputStream();

        Proto varint(int field, long value) {
            key(field, 0);
            raw(value);
            return this;
        }

        Proto string(int field, String value) {
            return bytes(field, value.getBytes(StandardCharsets.UTF_8));
        }

        Proto message(int field, Proto inner) {
            return bytes(field, inner.out.toByteArray());
        }

        Proto bytes(int field, byte[] value) {
            key(field, 2);
            raw(value.length);
            out.write(value, 0, value.length);
            return this;
        }

        private void key(int field, int wire) {
            raw(((long) field << 3) | wire);
        }

        private void raw(long v) {
            while ((v & ~0x7fL) != 0) {
                out.write((int) ((v & 0x7f) | 0x80));
                v >>>= 7;
            }
            out.write((int) v);
        }
    }

    private static Proto thread(int id, String name, String function) {
        Proto frame = new Proto().varint(1, 0x1a2b).varint(2, 0x7f001a2b).string(4, function).varint(5, 16)
                .string(6, "/data/app/~~x/dev.yap.app-y/lib/arm64/libyapqt_arm64-v8a.so")
                .string(8, "0123abcd");
        Proto registers = new Proto().string(1, "x0").varint(2, 0xdead);
        Proto t = new Proto().varint(1, id).string(2, name).message(3, registers).message(4, frame)
                .message(4, frame);
        return new Proto().varint(1, id).message(2, t);
    }

    static void tombstoneShouldReadTheCrashingThread() {
        Proto signal = new Proto().varint(1, 11).string(2, "SIGSEGV").varint(3, 1).string(4, "SEGV_MAPERR")
                .varint(8, 1).varint(9, 0);
        Proto cause = new Proto().string(1, "null pointer dereference");
        Proto log = new Proto().string(1, "main").string(2, "a logcat line with spike@example.com");
        byte[] bytes = new Proto().varint(1, 3).string(2, "samsung/fingerprint").varint(5, 100).varint(6, 101)
                .message(16, thread(100, "main", "other"))
                .message(10, signal).string(14, "abort: boom").message(15, cause)
                .message(16, thread(101, "yap-worker", "yap_crash_test"))
                .message(18, log)
                .out.toByteArray();
        Tombstone t = Tombstone.parse(bytes);
        check("tombstone", t != null, "null");
        if (t == null) return;
        check("signal", t.signal.equals("SIGSEGV") && t.signalCode.equals("SEGV_MAPERR"), t.signal);
        check("fault", t.hasFaultAddress && t.faultAddress == 0, "fault");
        check("abort", t.abortMessage.equals("abort: boom"), t.abortMessage);
        check("cause", t.cause.equals("null pointer dereference"), t.cause);
        check("thread", t.threadName.equals("yap-worker"), t.threadName);
        check("frames", t.frames.size() == 2, String.valueOf(t.frames));
        if (!t.frames.isEmpty()) {
            check("frame", t.frames.get(0).equals(
                    "#00 pc 0x1a2b libyapqt_arm64-v8a.so (yap_crash_test+16) (BuildId: 0123abcd)"),
                    t.frames.get(0));
        }
    }

    static void tombstoneShouldRejectGarbage() {
        check("garbage", Tombstone.parse(new byte[] {(byte) 0xff, (byte) 0xff, 0x01}) == null, "parsed");
        check("text", Tombstone.parse("not a tombstone at all".getBytes(StandardCharsets.UTF_8)) == null,
                "parsed text");
    }
}
