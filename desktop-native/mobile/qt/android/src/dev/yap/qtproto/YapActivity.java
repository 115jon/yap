package dev.yap.qtproto;

import android.content.Intent;
import android.os.Bundle;

import org.qtproject.qt.android.bindings.QtActivity;

import dev.yap.qtproto.push.IncomingCalls;
import dev.yap.qtproto.push.PushShim;
import dev.yap.qtproto.voice.VoiceShim;

/**
 * Qt's activity, with a notification's tap kept for the push code and a
 * link for the link code: either can arrive (onCreate, or onNewIntent on a
 * task relaunched from Recents) before Qt's main() has installed its shim,
 * so PushShim and LinkShim hold it until install() reads it. An incoming
 * call's full-screen intent and its Answer go to IncomingCalls, which
 * keeps them for Qt the same way, and shows the app over the lock screen
 * while the call needs it. And with a
 * call's goodbye on the way out.
 */
public class YapActivity extends QtActivity {
    private static YapActivity shown;
    /** IncomingCalls' last word, for an activity made after it. */
    private static boolean overLock;

    @Override
    public void onCreate(Bundle savedInstanceState) {
        shown = this;
        applyOverLock();
        // A recreated activity (restored state, or started again from
        // Recents) carries the intent it was first started with: an old tap.
        boolean fresh = savedInstanceState == null
                && (getIntent().getFlags() & Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0;
        if (fresh && !IncomingCalls.offer(this, getIntent())) {
            PushShim.offer(getIntent());
            LinkShim.offer(getIntent());
        }
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        if (!IncomingCalls.offer(this, intent)) {
            PushShim.offer(intent);
            LinkShim.offer(intent);
        }
    }

    @Override
    protected void onDestroy() {
        if (shown == this) shown = null;
        // Qt's onDestroy ends the process: a call still up tells its room
        // it left first (up to 800 ms; nothing outside a call).
        VoiceShim.quit();
        super.onDestroy();
    }

    /**
     * From IncomingCalls, on the main thread: a ring or an answered call
     * shows the app over the lock screen (and wakes the screen), until it
     * is over.
     */
    public static void showOverLock(boolean on) {
        overLock = on;
        if (shown != null) shown.applyOverLock();
    }

    private void applyOverLock() {
        setShowWhenLocked(overLock);
        setTurnScreenOn(overLock);
    }
}
