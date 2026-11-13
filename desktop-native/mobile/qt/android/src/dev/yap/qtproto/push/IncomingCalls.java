package dev.yap.qtproto.push;

import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.Intent;
import android.graphics.Bitmap;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import dev.yap.qtproto.YapActivity;
import dev.yap.qtproto.voice.VoiceShim;

import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The DM call ringing this phone (the Expo app's IncomingCalls.kt, without
 * Telecom), whichever way it arrived: a `call_ring` push with the app in
 * the background or killed, or the gateway's CALL_RING through the core and
 * the QML (Native.ringIncoming). It rings once per call id, whichever comes
 * first, and wherever it is answered: the app's incoming sheet, or the
 * notification and the lock screen.
 *
 * In front (Qt up, the app visible, the phone unlocked) it only plays the
 * ringtone; the QML shows its sheet. Otherwise it posts the incoming
 * CallStyle notification with its full-screen intent (CallNotifier), which
 * opens the app over the lock screen on that sheet. Either way the QML hears
 * of the ring (Native.incomingCall), so a ring the core never saw (the app
 * started from the push) still has its sheet.
 *
 * Answer from outside the app tells the server (POST accept, so the caller
 * stops hearing the ring while Qt starts) and hands the call to the QML,
 * which joins it once signed in (Native.callAnswered). Decline tells the
 * server from here. A missed ring (timeout, the caller hung up) becomes a
 * missed-call notification unless it rang in the app. Main thread inside;
 * the entry points may be called from any thread.
 */
public final class IncomingCalls {
    private IncomingCalls() {}

    private static final String TAG = "yap-call";

    /** Qt's side (cpp/calls.cpp), once install() ran. */
    static native void nativeRing(String json);
    static native void nativeRingEnded(String callId);
    static native void nativeAnswered(String json);

    private static final class Ring {
        final IncomingCall call;
        final boolean inApp;
        final Bitmap avatar;
        final Runnable timeout;

        Ring(IncomingCall call, boolean inApp, Bitmap avatar, Runnable timeout) {
            this.call = call;
            this.inApp = inApp;
            this.avatar = avatar;
            this.timeout = timeout;
        }
    }

    private static final Handler main = new Handler(Looper.getMainLooper());
    private static final ExecutorService io = Executors.newSingleThreadExecutor();

    private static volatile boolean running;
    private static Ring ring;
    /** The ring Qt reads at start (JSON, empty for none). */
    private static volatile String current = "";
    /** Answered outside the app, until the QML takes it. */
    private static IncomingCall answered;

    /** Calls already rung and settled, so a late push or event cannot ring again. */
    private static final Map<String, Long> settled = new LinkedHashMap<>();
    private static final long SETTLED_MEMORY_MS = 5 * 60_000L;

    /** The app shows over the lock screen for a call: until it is over. */
    private static boolean overLock;
    private static final long ANSWERED_JOIN_MS = 60_000L;
    private static final Runnable dropLock = () -> {
        if (ring == null && !VoiceShim.inCall()) lock(false);
    };

    /** After cpp/calls.cpp registered the natives. */
    public static void install(Context context) {
        running = true;
        CallNotifier.ensureChannels(context.getApplicationContext());
    }

    /** The ring for Qt's start: JSON, empty for none. */
    public static String current() {
        return current;
    }

    /** The call answered outside the app, once (JSON, empty for none). */
    public static synchronized String takeAnswered() {
        IncomingCall call = answered;
        answered = null;
        return call == null ? "" : call.toJson();
    }

    // ---- Entry points ----------------------------------------------------------

    /** A ring; `then` runs once it is up (or refused), for goAsync. */
    static void ring(Context context, IncomingCall call, Runnable then) {
        Context app = context.getApplicationContext();
        io.execute(() -> {
            PushStore.Session session = PushStore.session(app);
            if (session == null) {
                // Signed out: nobody here to call.
                if (then != null) then.run();
                return;
            }
            Bitmap avatar = call.callerAvatar != null ? PushApi.avatar(session, call.callerAvatar) : null;
            // A push's avatar may be API-relative; the QML wants it whole.
            IncomingCall ringing = call.callerAvatar != null ? call.withAvatar(session.absolute(call.callerAvatar)) : call;
            main.post(() -> {
                try {
                    start(app, ringing, avatar);
                } finally {
                    if (then != null) then.run();
                }
            });
        });
    }

    /** From the QML: the core's ring (CALL_RING). */
    public static void ringFromApp(Context context, String json) {
        IncomingCall call = IncomingCall.fromJson(json);
        if (call != null) ring(context, call, null);
    }

    /** A ring stop (`call_ring_stop`, or the core's); `caller` describes it for a missed call. */
    static void stop(Context context, String callId, String reason, IncomingCall caller) {
        Context app = context.getApplicationContext();
        main.post(() -> finish(app, callId, reason, caller));
    }

    /** From the QML: the core's ring stopped (`reason` its end, or "accepted"). */
    public static void stopFromApp(Context context, String callId, String reason) {
        stop(context, callId, reason, null);
    }

    /**
     * Answered. `tellServer`: POST accept (the core did not see this ring,
     * or the gateway may be down). `fromApp`: the app's own sheet, which
     * joins itself; otherwise the QML hears of it (Native.callAnswered).
     */
    public static void answer(Context context, String callId, boolean tellServer, boolean fromApp) {
        Context app = context.getApplicationContext();
        main.post(() -> answerOnMain(app, callId, null, tellServer, fromApp));
    }

    /** Declined, from anywhere; `tellServer` unless the core sends it (op 38). */
    public static void decline(Context context, String callId, boolean tellServer, Runnable then) {
        decline(context, callId, null, tellServer, then);
    }

    /**
     * The notification's Decline: `channelId` takes its ring down also when
     * this process no longer holds it (the one that posted it was let go).
     */
    static void decline(Context context, String callId, String channelId, boolean tellServer, Runnable then) {
        Context app = context.getApplicationContext();
        main.post(() -> {
            tearDown(app, callId);
            if (channelId != null) CallNotifier.cancel(app, channelId);
            markSettled(callId);
            if (tellServer) {
                tellServer(app, callId, "decline", then);
            } else if (then != null) {
                then.run();
            }
        });
    }

    public static void declineFromApp(Context context, String callId, boolean tellServer) {
        decline(context, callId, tellServer, null);
    }

    /**
     * The activity's intent (YapActivity: a fresh start's, and each
     * onNewIntent): the full-screen ring or the notification's Answer.
     * True when it was one of these.
     */
    public static boolean offer(Context context, Intent intent) {
        if (intent == null) return false;
        String action = intent.getAction();
        boolean show = CallNotifier.ACTION_RING.equals(action);
        boolean answer = CallNotifier.ACTION_ANSWER.equals(action);
        if (!show && !answer) return false;
        IncomingCall call = IncomingCall.from(intent);
        if (call == null) return true;
        main.post(() -> {
            // Only the ring this process holds: the activity is exported,
            // so another app could send these actions with any extras, and
            // they show the app over the lock screen and answer for the
            // user. A ring this process no longer holds (it was let go
            // after posting) just opens the app; its notification ends on
            // its own (setTimeoutAfter) or with the stop push.
            if (ring == null || !ring.call.callId.equals(call.callId)) {
                Log.i(TAG, "call intent for a ring not held here: just the app");
                return;
            }
            lock(true);
            if (answer) answerOnMain(context.getApplicationContext(), call.callId, null, true, false);
        });
        return true;
    }

    /** Signed out: the ring and the call notifications go with the account. */
    static void forget(Context context) {
        Context app = context.getApplicationContext();
        main.post(() -> {
            if (ring != null) tearDown(app, ring.call.callId);
            synchronized (IncomingCalls.class) {
                answered = null;
            }
            CallNotifier.cancelAll(app);
        });
    }

    /** From VoiceShim: the last call ended (the lock screen is the lock screen again). */
    public static void callEnded() {
        main.post(() -> {
            if (ring == null) lock(false);
        });
    }

    // ---- Inside -----------------------------------------------------------------

    private static void start(Context app, IncomingCall call, Bitmap avatar) {
        if (isSettled(call.callId)) return;
        if (ring != null) {
            // The same ring again (the push and the gateway), or a second
            // caller while one rings (the server says busy to them).
            return;
        }
        boolean inApp = running && isInFront(app);
        Runnable timeout = () -> finish(app, call.callId, "timeout", null);
        ring = new Ring(call, inApp, avatar, timeout);
        main.postDelayed(timeout, call.ringSeconds * 1000L);
        if (inApp) {
            Ringer.start(app);
        } else {
            CallNotifier.postRinging(app, call, avatar);
        }
        current = call.toJson();
        if (running) nativeRing(current);
        Log.i(TAG, "ringing " + (inApp ? "in the app" : "as a notification"));
    }

    private static void answerOnMain(Context app, String callId, IncomingCall fallback, boolean tellServer,
            boolean fromApp) {
        Ring rung = ring != null && ring.call.callId.equals(callId) ? ring : null;
        IncomingCall call = rung != null ? rung.call : fallback;
        if (call == null) return;
        if (rung != null) {
            ring = null;
            main.removeCallbacks(rung.timeout);
        }
        markSettled(callId);
        Ringer.stop();
        CallNotifier.cancel(app, call.channelId);
        if (tellServer) tellServer(app, callId, "accept", null);
        if (!fromApp) {
            synchronized (IncomingCalls.class) {
                answered = call;
            }
            if (running) nativeAnswered(call.toJson());
        }
        ended(callId);
        // Answered over the lock screen: the app stays over it for the call
        // (VoiceShim says when it ends), or a minute if it never joins.
        if (overLock) {
            main.removeCallbacks(dropLock);
            main.postDelayed(dropLock, ANSWERED_JOIN_MS);
        }
    }

    private static void finish(Context app, String callId, String reason, IncomingCall caller) {
        Ring rung = ring != null && ring.call.callId.equals(callId) ? ring : null;
        if (rung == null) {
            // Not rung by this process: one that was let go may have left
            // its ringing notification up (ongoing, the ringtone looping),
            // so it goes by its DM. A missed call (the push came late, or
            // not at all) still gets its notification.
            if (caller != null) CallNotifier.cancel(app, caller.channelId);
            if (IncomingCall.isMissed(reason) && caller != null && !isSettled(callId)) {
                CallNotifier.postMissed(app, caller, null);
            }
            markSettled(callId);
            return;
        }
        tearDown(app, callId);
        if (IncomingCall.isMissed(reason) && !rung.inApp) {
            CallNotifier.postMissed(app, rung.call, rung.avatar);
        }
    }

    /** Stops the ring for `callId` (if it is the one ringing) and says so. */
    private static void tearDown(Context app, String callId) {
        Ring rung = ring != null && ring.call.callId.equals(callId) ? ring : null;
        if (rung == null) return;
        ring = null;
        markSettled(callId);
        main.removeCallbacks(rung.timeout);
        Ringer.stop();
        CallNotifier.cancel(app, rung.call.channelId);
        ended(callId);
        if (!VoiceShim.inCall()) lock(false);
    }

    private static void ended(String callId) {
        current = "";
        if (running) nativeRingEnded(callId);
    }

    private static void tellServer(Context app, String callId, String answer, Runnable then) {
        io.execute(() -> {
            try {
                PushStore.Session session = PushStore.session(app);
                if (session == null) return;
                int status = PushApi.answerCall(session, callId, answer);
                if (status < 200 || status > 299) Log.i(TAG, answer + ": HTTP " + status);
            } finally {
                if (then != null) then.run();
            }
        });
    }

    private static void lock(boolean on) {
        if (on == overLock) return;
        overLock = on;
        if (!on) main.removeCallbacks(dropLock);
        YapActivity.showOverLock(on);
    }

    private static boolean isInFront(Context app) {
        ActivityManager.RunningAppProcessInfo info = new ActivityManager.RunningAppProcessInfo();
        ActivityManager.getMyMemoryState(info);
        boolean visible = info.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND;
        KeyguardManager keyguard = app.getSystemService(KeyguardManager.class);
        boolean locked = keyguard != null && keyguard.isKeyguardLocked();
        return visible && !locked;
    }

    private static boolean isSettled(String callId) {
        long now = System.currentTimeMillis();
        for (Iterator<Long> it = settled.values().iterator(); it.hasNext(); ) {
            if (now - it.next() > SETTLED_MEMORY_MS) it.remove();
        }
        return settled.containsKey(callId);
    }

    private static void markSettled(String callId) {
        settled.put(callId, System.currentTimeMillis());
    }
}
