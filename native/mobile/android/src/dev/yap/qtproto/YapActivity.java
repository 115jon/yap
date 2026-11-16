package dev.yap.qtproto;

import android.content.Intent;
import android.os.Bundle;
import android.view.MotionEvent;

import org.qtproject.qt.android.bindings.QtActivity;

import dev.yap.qtproto.crash.CrashReports;
import dev.yap.qtproto.push.IncomingCalls;
import dev.yap.qtproto.push.PushShim;
import dev.yap.qtproto.voice.VoiceShim;

/**
 * Qt's activity, with a notification's tap kept for the push code and a
 * link or a share (ShareActivity's) for the link code: any can arrive
 * (onCreate, or onNewIntent on a task relaunched from Recents) before Qt's
 * main() has installed its shim, so PushShim, LinkShim and ShareShim hold
 * it until install() reads it. An incoming call's full-screen intent and
 * its Answer go to IncomingCalls, which keeps them for Qt the same way, and
 * shows the app over the lock screen while the call needs it. And with a
 * call's goodbye on the way out. Crash reports (crash/CrashReports) are
 * set up before Qt starts, and last run's crash is offered on top. Every
 * touch passes FlingShim first, for the finger's speed at a lift.
 */
public class YapActivity extends QtActivity {
    private static YapActivity shown;
    /** IncomingCalls' last word, for an activity made after it. */
    private static boolean overLock;

    @Override
    public void onCreate(Bundle savedInstanceState) {
        shown = this;
        CrashReports.install(this);
        applyOverLock();
        // A recreated activity (restored state, or started again from
        // Recents) carries the intent it was first started with: an old tap.
        boolean fresh = savedInstanceState == null
                && (getIntent().getFlags() & Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0;
        if (fresh && !IncomingCalls.offer(this, getIntent())) {
            PushShim.offer(getIntent());
            LinkShim.offer(getIntent());
            ShareShim.offer(getIntent());
        }
        if (fresh) CrashReports.test(getIntent());
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        if (!IncomingCalls.offer(this, intent)) {
            PushShim.offer(intent);
            LinkShim.offer(intent);
            ShareShim.offer(intent);
        }
        CrashReports.test(intent);
    }

    @Override
    public boolean dispatchTouchEvent(MotionEvent event) {
        FlingShim.track(event);
        return super.dispatchTouchEvent(event);
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
