package dev.yap.qtproto;

import android.content.Intent;
import android.net.Uri;

/**
 * Links Android opens the app with (the manifest's VIEW filters: the web's
 * invite and chat links, and yap://): YapActivity offers each intent, at a
 * fresh start and on every onNewIntent. A link that comes before Qt's
 * main() has installed the shim (a cold start) waits here until install()
 * reads it; after that each goes to `Native` (cpp/links.cpp), which keeps it
 * for the QML until it is signed in. Which links count is the core's rule
 * (Invites.openLink); this only hands the URL over.
 */
public final class LinkShim {
    private LinkShim() {}

    static native void nativeLink(String url);

    /** Qt is up and nativeLink is registered. */
    private static volatile boolean running;
    /** The last intent handed over (the activity keeps its intent). */
    private static Intent handled;
    /** A link that came before install(). */
    private static String pending;

    /** After nativeLink is registered: a link from before, if any. */
    public static void install() {
        String url;
        synchronized (LinkShim.class) {
            running = true;
            url = pending;
            pending = null;
        }
        if (url != null) nativeLink(url);
    }

    /** The activity's intent: a VIEW with a URL is a link. */
    public static void offer(Intent intent) {
        if (intent == null || !Intent.ACTION_VIEW.equals(intent.getAction())) return;
        Uri data = intent.getData();
        if (data == null) return;
        String url = data.toString();
        synchronized (LinkShim.class) {
            if (intent == handled) return;
            handled = intent;
            if (!running) {
                // The latest wins, as the core's held link does.
                pending = url;
                return;
            }
        }
        nativeLink(url);
    }
}
