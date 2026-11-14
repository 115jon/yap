package dev.yap.qtproto.crash;

import android.app.Activity;
import android.app.ActivityManager;
import android.app.AlertDialog;
import android.app.ApplicationExitInfo;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.Process;
import android.system.ErrnoException;
import android.system.Os;
import android.util.Log;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.io.Writer;
import java.lang.ref.WeakReference;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Date;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TimeZone;

/**
 * Crash reports, kept on the phone as the desktop keeps its minidumps
 * (native/rust/src/crash.rs): nothing is uploaded. A crash leaves its
 * traces in the app's private `files/crashes` (excluded from backups):
 *
 * - a Java or Kotlin exception nothing caught: this class's handler writes
 *   `java-<pid>.txt` and hands over to Android's (which ends the process);
 * - a Rust panic: the Rust hook (rust/src/crash.rs) writes `rust-<pid>.txt`;
 * - a native crash (SIGSEGV, SIGABRT, a panic that aborts) and an ANR:
 *   Android's own record, read on the next launch through
 *   ApplicationExitInfo (API 30+), with the tombstone (API 31+) or the ANR
 *   trace. No signal handler of our own.
 *
 * The next launch joins them by pid into one report, redacted (Redactor:
 * no tokens, messages, ids, addresses or personal paths), saved as
 * `report-<time>.txt` (the newest five kept), and asks once: "Yap closed
 * unexpectedly". Share hands the report's text to the share sheet, where
 * the user picks the app and the recipient and reads it before it goes;
 * Delete removes the reports; Not now keeps them on the phone.
 */
public final class CrashReports {
    private CrashReports() {}

    private static final String TAG = "yap-crash";
    /** Reports kept: the oldest go when there are more. */
    static final int KEEP = 5;
    /** The longest raw file read (a Java stack, a Rust panic). */
    private static final int MAX_RAW = 64 * 1024;
    /** The most ANR trace lines kept: the main thread's stack. */
    private static final int MAX_ANR_LINES = 80;
    private static final String PREFS = "yap_crash";
    /** The newest exit record already looked at (its timestamp). */
    private static final String SEEN_EXIT = "seen_exit";

    private static File dir;
    private static boolean collected;

    /**
     * Once per process, before any of its components runs (CrashInit, the
     * manifest's provider): the folder, named to the Rust hook in
     * YAP_CRASH_DIR, and the exception handler.
     */
    public static synchronized void init(Context context) {
        if (dir != null) return;
        dir = new File(context.getFilesDir(), "crashes");
        if (!dir.isDirectory() && !dir.mkdirs()) Log.w(TAG, "no crash folder");
        try {
            Os.setenv("YAP_CRASH_DIR", dir.getAbsolutePath(), true);
        } catch (ErrnoException e) {
            Log.w(TAG, "YAP_CRASH_DIR not set: " + e.getMessage());
        }
        Thread.UncaughtExceptionHandler previous = Thread.getDefaultUncaughtExceptionHandler();
        Thread.setDefaultUncaughtExceptionHandler((thread, error) -> {
            writeJava(thread, error);
            if (previous != null) {
                previous.uncaughtException(thread, error);
            } else {
                Process.killProcess(Process.myPid());
                System.exit(10);
            }
        });
    }

    /**
     * From YapActivity.onCreate, before Qt starts: the first time in this
     * process, last run's crashes are collected (off the main thread) and
     * offered over the activity.
     */
    public static void install(Activity activity) {
        init(activity.getApplicationContext());
        synchronized (CrashReports.class) {
            if (collected) return;
            collected = true;
        }
        Context app = activity.getApplicationContext();
        WeakReference<Activity> shown = new WeakReference<>(activity);
        Thread worker = new Thread(() -> {
            List<File> fresh;
            try {
                fresh = collect(app);
            } catch (Throwable e) {
                // Never a crash of its own at start: the raw files and the
                // exit marker are already done with (collect's finally).
                Log.w(TAG, "crash reports not read", e);
                return;
            }
            if (fresh.isEmpty()) return;
            new Handler(Looper.getMainLooper()).post(() -> offer(shown.get(), fresh));
        }, "yap-crash-reports");
        worker.setPriority(Thread.MIN_PRIORITY);
        worker.start();
    }

    // ---- At the crash -------------------------------------------------------

    /** The exception's stack, raw (redacted on the next launch). */
    private static void writeJava(Thread thread, Throwable error) {
        try {
            StringWriter stack = new StringWriter();
            stack.append("Java exception on thread '").append(thread.getName()).append("'\n");
            error.printStackTrace(new PrintWriter(stack));
            write(new File(dir, "java-" + Process.myPid() + ".txt"), stack.toString());
        } catch (Throwable ignored) {
            // The process is going down either way.
        }
    }

    // ---- On the next launch ------------------------------------------------

    /**
     * Last run's crashes as reports, saved and redacted: the new ones. The
     * exit marker moves on and the raw files go before and whatever
     * happens, so a report that cannot be made is not tried again on every
     * start.
     */
    static List<File> collect(Context context) {
        Map<Integer, File> java = raw("java-");
        Map<Integer, File> rust = raw("rust-");
        // This process's own (a panic it survived) stay for its next launch.
        java.remove(Process.myPid());
        rust.remove(Process.myPid());
        List<File> used = new ArrayList<>(java.values());
        used.addAll(rust.values());
        List<File> fresh = new ArrayList<>();
        try {
            String header = header(context);
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                for (ApplicationExitInfo exit : newExits(context)) {
                    String body = fromExit(exit, java.remove(exit.getPid()), rust.remove(exit.getPid()));
                    File saved = save(exit.getTimestamp(), header + body);
                    if (saved != null) fresh.add(saved);
                }
            }
            // A Java crash with no exit record (before API 30, or one
            // Android dropped): its own file is the report. A panic no exit
            // record names was survived (or is too old to say): dropped.
            for (File file : java.values()) {
                String body = "Kind: Java exception\n\n" + Redactor.text(read(file));
                File saved = save(file.lastModified(), header + body);
                if (saved != null) fresh.add(saved);
            }
            prune();
        } finally {
            for (File file : used) delete(file);
        }
        return fresh;
    }

    /**
     * The main process's crashes and ANRs since the last look, oldest
     * first; the marker moves past them before they are read. The first
     * launch with crash reports reads none (older builds kept no files).
     */
    private static List<ApplicationExitInfo> newExits(Context context) {
        List<ApplicationExitInfo> found = new ArrayList<>();
        ActivityManager am = context.getSystemService(ActivityManager.class);
        if (am == null) return found;
        List<ApplicationExitInfo> exits = am.getHistoricalProcessExitReasons(context.getPackageName(), 0, 16);
        SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        long seen = prefs.getLong(SEEN_EXIT, -1);
        long newest = Math.max(seen, 0);
        for (ApplicationExitInfo exit : exits) newest = Math.max(newest, exit.getTimestamp());
        if (newest != seen) prefs.edit().putLong(SEEN_EXIT, newest).commit();
        if (seen < 0) return found;
        // Newest first from Android.
        for (int i = exits.size() - 1; i >= 0; i--) {
            ApplicationExitInfo exit = exits.get(i);
            if (exit.getTimestamp() <= seen) continue;
            if (!context.getPackageName().equals(exit.getProcessName())) continue;
            int reason = exit.getReason();
            if (reason == ApplicationExitInfo.REASON_CRASH
                    || reason == ApplicationExitInfo.REASON_CRASH_NATIVE
                    || reason == ApplicationExitInfo.REASON_ANR) {
                found.add(exit);
            }
        }
        return found;
    }

    /** The report's body for one exit record and the files of its pid. */
    private static String fromExit(ApplicationExitInfo exit, File java, File rust) {
        StringBuilder out = new StringBuilder();
        int reason = exit.getReason();
        out.append("Kind: ").append(reason == ApplicationExitInfo.REASON_CRASH ? "Java exception"
                : reason == ApplicationExitInfo.REASON_ANR ? "not responding (ANR)" : "native crash")
                .append('\n');
        out.append("Time: ").append(utc(exit.getTimestamp())).append('\n');
        out.append("Exit status: ").append(exit.getStatus()).append('\n');
        String description = Redactor.text(exit.getDescription());
        if (!description.isEmpty()) out.append("Description: ").append(description).append('\n');
        out.append('\n');
        if (java != null) {
            out.append(Redactor.text(read(java))).append("\n\n");
            delete(java);
        }
        if (rust != null) {
            out.append("Rust panic\n").append(Redactor.text(read(rust))).append("\n\n");
            delete(rust);
        }
        if (reason == ApplicationExitInfo.REASON_CRASH_NATIVE) {
            Tombstone tombstone = null;
            try (InputStream trace = exit.getTraceInputStream()) {
                if (trace != null) tombstone = Tombstone.read(trace);
            } catch (IOException | RuntimeException e) {
                Log.w(TAG, "no tombstone: " + e);
            }
            if (tombstone != null) out.append(nativeCrash(tombstone));
            else out.append("(no tombstone)\n");
        } else if (reason == ApplicationExitInfo.REASON_ANR) {
            try (InputStream trace = exit.getTraceInputStream()) {
                if (trace != null) out.append(Redactor.text(mainThread(trace)));
                else out.append("(no trace)\n");
            } catch (IOException | RuntimeException e) {
                out.append("(no trace)\n");
            }
        }
        return out.toString();
    }

    private static String nativeCrash(Tombstone t) {
        StringBuilder out = new StringBuilder();
        out.append("Signal: ").append(t.signal);
        if (!t.signalCode.isEmpty()) out.append(" (").append(t.signalCode).append(')');
        if (t.hasFaultAddress) out.append(String.format(Locale.ROOT, ", fault address 0x%x", t.faultAddress));
        out.append('\n');
        if (!t.cause.isEmpty()) out.append("Cause: ").append(Redactor.text(t.cause)).append('\n');
        if (!t.abortMessage.isEmpty()) {
            out.append("Abort message: ").append(Redactor.text(t.abortMessage)).append('\n');
        }
        out.append("Thread: ").append(Redactor.text(t.threadName)).append('\n');
        out.append("Backtrace:\n");
        for (String frame : t.frames) out.append("  ").append(frame).append('\n');
        return out.toString();
    }

    /** An ANR trace's main thread: from `"main"` to the blank line. */
    static String mainThread(InputStream trace) throws IOException {
        BufferedReader in = new BufferedReader(new InputStreamReader(trace, StandardCharsets.UTF_8));
        StringBuilder out = new StringBuilder();
        boolean inMain = false;
        int lines = 0;
        String line;
        while ((line = in.readLine()) != null) {
            if (!inMain) {
                inMain = line.startsWith("\"main\"");
                if (!inMain) continue;
            } else if (line.isEmpty()) {
                break;
            }
            out.append(line).append('\n');
            if (++lines >= MAX_ANR_LINES) break;
        }
        return out.length() == 0 ? "(no main thread in the trace)\n" : out.toString();
    }

    /** The app's version and the Android it ran on: no device or user. */
    private static String header(Context context) {
        String version = "?";
        try {
            PackageInfo info = context.getPackageManager().getPackageInfo(context.getPackageName(), 0);
            version = info.versionName + " (" + info.getLongVersionCode() + ")";
        } catch (Exception ignored) {
            // The report says "?".
        }
        String abi = Build.SUPPORTED_ABIS.length > 0 ? Build.SUPPORTED_ABIS[0] : "?";
        return "Yap crash report\n"
                + "App: " + version + "\n"
                + "Android: " + Build.VERSION.RELEASE + " (API " + Build.VERSION.SDK_INT + "), " + abi + "\n";
    }

    // ---- Files ----------------------------------------------------------

    /** The raw files with `prefix`, by the pid in their name. */
    private static Map<Integer, File> raw(String prefix) {
        Map<Integer, File> found = new HashMap<>();
        File[] files = dir.listFiles();
        if (files == null) return found;
        for (File file : files) {
            String name = file.getName();
            if (!name.startsWith(prefix) || !name.endsWith(".txt")) continue;
            try {
                found.put(Integer.parseInt(name.substring(prefix.length(), name.length() - 4)), file);
            } catch (NumberFormatException ignored) {
                delete(file);
            }
        }
        return found;
    }

    private static File save(long at, String text) {
        File file = new File(dir, "report-" + at + ".txt");
        try {
            write(file, text);
            Log.i(TAG, "crash report saved: " + file.getName());
            return file;
        } catch (IOException e) {
            Log.w(TAG, "crash report not saved: " + e.getMessage());
            return null;
        }
    }

    /** The reports, newest first. */
    static List<File> reports() {
        File[] files = dir.listFiles((d, name) -> name.startsWith("report-") && name.endsWith(".txt"));
        if (files == null) return new ArrayList<>();
        List<File> list = new ArrayList<>(Arrays.asList(files));
        list.sort((a, b) -> Long.compare(stamp(b), stamp(a)));
        return list;
    }

    private static long stamp(File report) {
        String name = report.getName();
        try {
            return Long.parseLong(name.substring("report-".length(), name.length() - 4));
        } catch (NumberFormatException e) {
            return report.lastModified();
        }
    }

    private static void prune() {
        List<File> list = reports();
        for (int i = KEEP; i < list.size(); i++) delete(list.get(i));
    }

    private static void write(File file, String text) throws IOException {
        try (Writer out = new OutputStreamWriter(new FileOutputStream(file), StandardCharsets.UTF_8)) {
            out.write(text);
        }
    }

    static String read(File file) {
        try {
            byte[] bytes = Files.readAllBytes(file.toPath());
            int n = Math.min(bytes.length, MAX_RAW);
            return new String(bytes, 0, n, StandardCharsets.UTF_8);
        } catch (IOException e) {
            return "";
        }
    }

    private static void delete(File file) {
        if (file != null && file.exists() && !file.delete()) Log.w(TAG, "not deleted: " + file.getName());
    }

    private static String utc(long millis) {
        SimpleDateFormat format = new SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.ROOT);
        format.setTimeZone(TimeZone.getTimeZone("UTC"));
        return format.format(new Date(millis));
    }

    // ---- The prompt ---------------------------------------------------------

    /** "Yap closed unexpectedly": Share, Delete or Not now. */
    private static void offer(Activity activity, List<File> fresh) {
        if (activity == null || activity.isFinishing() || activity.isDestroyed()) return;
        File newest = fresh.get(fresh.size() - 1);
        String more = fresh.size() > 1 ? " (" + fresh.size() + " crashes since the last start)" : "";
        new AlertDialog.Builder(activity)
                .setTitle("Yap closed unexpectedly")
                .setMessage("A crash report was saved on this phone" + more + ". It says where the app "
                        + "failed and which app and Android versions it ran on. Tokens, addresses, ids "
                        + "and quoted text are taken out of it.\n\nShare it with the Yap developers? "
                        + "You pick the app to send it with and can read it before it goes.")
                .setPositiveButton("Share report", (d, w) -> share(activity, newest))
                .setNeutralButton("Delete", (d, w) -> {
                    for (File report : reports()) delete(report);
                })
                .setNegativeButton("Not now", null)
                .show();
    }

    private static void share(Activity activity, File report) {
        String text = read(report);
        Intent send = new Intent(Intent.ACTION_SEND)
                .setType("text/plain")
                .putExtra(Intent.EXTRA_SUBJECT, "Yap crash report")
                .putExtra(Intent.EXTRA_TEXT, text);
        try {
            activity.startActivity(Intent.createChooser(send, "Share the crash report"));
        } catch (RuntimeException e) {
            Log.w(TAG, "no share sheet: " + e);
        }
    }

    // ---- The debug trigger --------------------------------------------------

    /**
     * The debug trigger's intent (CrashTest). Only builds made with
     * YAP_CRASH_TEST=1 have the trigger (android/crash-test); every other
     * build, the release ones included, gets an empty CrashTest from
     * android/build.gradle.
     */
    public static void test(Intent intent) {
        CrashTest.handle(intent);
    }
}
