package dev.yap.qtproto.voice;

import android.Manifest;
import android.app.Activity;
import android.app.ActivityManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.util.Log;

import androidx.core.content.ContextCompat;

import dev.yap.qtproto.push.IncomingCalls;

/**
 * The voice side of `Native` (cpp/voice.cpp): libwebrtc's Java start-up,
 * and what Android asks of an app in a call. While the core has a call
 * running, a foreground service of type microphone holds the process (and
 * its mic) with the screen off or the app in the background, behind an
 * ongoing notification with Mute and Leave; the audio session (mode, focus,
 * route) lives as long (CallAudio).
 *
 * Nothing here waits on Qt's thread: the core says a call started or
 * ended from its own threads, and the notification's actions and a focus
 * loss go straight to the core. The state mirrored here (muted, deafened,
 * the channel's name) comes from the QML, which keeps running in the
 * background (the manifest's background_running), and from this class's
 * own actions. The call's fields belong to the main thread.
 */
public final class VoiceShim {
    private VoiceShim() {}

    private static final String TAG = "yap-voice";

    static native boolean nativeInit(Context context);
    static native void nativeMute(boolean muted);
    static native void nativeDeafen(boolean deafened);
    static native void nativeLeave();
    static native void nativeQuit();
    static native void nativeRoutes(String json);

    /**
     * How long the service outlives the last call: a call that restarts
     * (the room reset it, a move to another channel) keeps the service it
     * has, since a new one may not start from the background.
     */
    private static final long STOP_AFTER_MS = 1200;

    private static final Handler main = new Handler(Looper.getMainLooper());
    private static volatile Context app;
    private static volatile CallAudio audio;
    /** The natives above are registered and libwebrtc took the context. */
    private static volatile boolean ready;

    private static boolean active;
    private static long startedAt;
    private static String title = "";
    private static boolean muted;
    private static boolean deafened;
    /** A focus loss deafened the call; getting focus back undoes it. */
    private static boolean interruptDeafened;
    private static final Runnable stop = VoiceShim::stopNow;

    /**
     * After the natives are registered, on the thread that called in from
     * Qt: libwebrtc finds its Java classes through this call's class loader.
     */
    public static boolean install(Activity a) {
        Context context = a.getApplicationContext();
        audio = new CallAudio(context, main, new CallAudio.Listener() {
            @Override
            public void onRoutes(String json) {
                if (ready) nativeRoutes(json);
            }

            @Override
            public void onInterrupted(boolean interrupted) {
                VoiceShim.onInterrupted(interrupted);
            }
        });
        app = context;
        boolean ok = false;
        try {
            ok = nativeInit(context);
        } catch (Throwable t) {
            Log.e(TAG, "libwebrtc did not start", t);
        }
        ready = ok;
        return ok;
    }

    /** From the core, on any thread: a call runs, or the last one ended. */
    public static void setActive(boolean on) {
        main.post(() -> {
            main.removeCallbacks(stop);
            if (!on) {
                if (active) main.postDelayed(stop, STOP_AFTER_MS);
                return;
            }
            if (active || app == null) return;
            active = true;
            startedAt = System.currentTimeMillis();
            interruptDeafened = false;
            audio.begin();
            // A microphone service may only start with the permission and
            // from the foreground (Android 14 refuses it otherwise, and a
            // service asked to start must then become a foreground one).
            // Without it the call runs while the app stays in front.
            if (!mayStartService()) {
                Log.w(TAG, "the call's service was not started (no microphone permission, or not in front)");
                return;
            }
            try {
                ContextCompat.startForegroundService(app, VoiceService.intent(app, VoiceService.ACTION_START));
            } catch (RuntimeException e) {
                Log.w(TAG, "the call's service did not start: " + e);
            }
        });
    }

    /**
     * The microphone is allowed already (Native.requestMicrophone answers
     * at once then, without Qt's permission request).
     */
    public static boolean microphoneAllowed(Context context) {
        return ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO)
                == PackageManager.PERMISSION_GRANTED;
    }

    private static boolean mayStartService() {
        if (ContextCompat.checkSelfPermission(app, Manifest.permission.RECORD_AUDIO)
                != PackageManager.PERMISSION_GRANTED) {
            return false;
        }
        ActivityManager.RunningAppProcessInfo state = new ActivityManager.RunningAppProcessInfo();
        ActivityManager.getMyMemoryState(state);
        return state.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND;
    }

    private static void stopNow() {
        if (!active) return;
        active = false;
        endInterruption();
        audio.end();
        VoiceService.stop(app);
        if (ready) nativeRoutes("[]");
        IncomingCalls.callEnded();
    }

    /**
     * The interruption's deafen is not the user's: it goes when the call
     * does, or the core would keep it for the next call (it remembers the
     * mic and deafen switches).
     */
    private static void endInterruption() {
        if (!interruptDeafened) return;
        interruptDeafened = false;
        deafened = false;
        if (ready) nativeDeafen(false);
    }

    /** From the QML: what the notification shows. */
    public static void setState(String channel, boolean isMuted, boolean isDeafened) {
        main.post(() -> {
            String next = channel == null ? "" : channel;
            if (next.equals(title) && isMuted == muted && isDeafened == deafened) return;
            title = next;
            muted = isMuted;
            deafened = isDeafened;
            VoiceService.refresh(app);
        });
    }

    /**
     * From the QML: the user pressed mute or deafen. Theirs stands when an
     * interruption ends (the mirror above cannot tell: it also echoes the
     * interruption's own deafen back).
     */
    public static void userChanged() {
        main.post(() -> interruptDeafened = false);
    }

    public static void setRoute(String id) {
        main.post(() -> {
            if (audio != null) audio.choose(id);
        });
    }

    /** The app's page in the system settings (a refused microphone). */
    public static void openAppSettings(Activity a) {
        try {
            a.startActivity(new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                    Uri.fromParts("package", a.getPackageName(), null))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        } catch (RuntimeException e) {
            Log.w(TAG, "no app settings: " + e);
        }
    }

    /**
     * The process is about to go (the task swiped away, the activity
     * destroyed: Qt exits with it). On the main thread; with a call up it
     * blocks up to 800 ms while the room is told we left, so nobody watches
     * a ghost for the room's resume grace.
     */
    public static void quit() {
        if (!active || !ready) return;
        endInterruption();
        nativeQuit();
        // Should the process live on after all, the call ends here too.
        nativeLeave();
        // And the service now: left running, the system starts the process
        // again for it once it is gone.
        main.removeCallbacks(stop);
        active = false;
        audio.end();
        VoiceService.stopNow();
    }

    /** A call runs (main thread): the lock screen stays behind the app for it. */
    public static boolean inCall() {
        return active;
    }

    // What the service asks (main thread).

    static boolean active() {
        return active;
    }

    static long startedAt() {
        return startedAt;
    }

    static String title() {
        return title;
    }

    static boolean muted() {
        return muted;
    }

    static boolean deafened() {
        return deafened;
    }

    /** The notification's Mute or Unmute. */
    static void toggleMute() {
        if (!active || !ready) return;
        // Deafened is muted too (the core mutes with it).
        muted = !(muted || deafened);
        // As the core has it: unmuting while deafened undeafens.
        if (!muted) {
            deafened = false;
            interruptDeafened = false;
        }
        nativeMute(muted);
        VoiceService.refresh(app);
    }

    /** The notification's Leave. */
    static void leave() {
        if (!active || !ready) return;
        endInterruption();
        nativeLeave();
    }

    private static void onInterrupted(boolean interrupted) {
        if (!active || !ready) return;
        if (interrupted) {
            if (deafened) return;
            interruptDeafened = true;
            deafened = true;
            nativeDeafen(true);
        } else if (interruptDeafened) {
            interruptDeafened = false;
            deafened = false;
            nativeDeafen(false);
        } else {
            return;
        }
        VoiceService.refresh(app);
    }
}
