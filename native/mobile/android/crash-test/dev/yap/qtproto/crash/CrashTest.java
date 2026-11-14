package dev.yap.qtproto.crash;

import android.content.Intent;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

/**
 * The debug crash trigger, in builds made with YAP_CRASH_TEST=1 only
 * (android/build.gradle adds this folder then; other builds get an empty
 * handle()). `am start -n <app id>/dev.yap.qtproto.YapActivity --es
 * yap_crash_test java|rust|segv` crashes the app three seconds later on the
 * main thread: an uncaught Java exception, a Rust panic, or a segmentation
 * fault in Rust (cpp/main.cpp registers rustCrash, rust/src/crash.rs). The
 * Java message carries made-up secrets for the report's redaction to
 * remove, as the Rust panic's does.
 */
final class CrashTest {
    private CrashTest() {}

    /** cpp/main.cpp: yap_crash_test(kind), 1 a panic, 2 a SIGSEGV. */
    static native void rustCrash(int kind);

    static void handle(Intent intent) {
        String kind = intent == null ? null : intent.getStringExtra("yap_crash_test");
        if (kind == null) return;
        Log.w("yap-crash", "crash test '" + kind + "' in 3 s");
        new Handler(Looper.getMainLooper()).postDelayed(() -> {
            switch (kind) {
                case "rust" -> rustCrash(1);
                case "segv" -> rustCrash(2);
                default -> throw new IllegalStateException("crash test: token=Zm9vYmFyYmF6cXV4MTIzNDU2"
                        + "Nzg5MGFiY2RlZg for spike@example.com at https://yap.example/api/channels/"
                        + "123e4567-e89b-12d3-a456-426614174000?sig=abc from 192.0.2.7 in "
                        + "/data/user/0/dev.yap.app/files/session.bin, said \"see you at 8, Sam\"");
            }
        }, 3000);
    }
}
