package dev.yap.qtproto.voice;

import android.app.Notification;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.util.Log;

import androidx.core.app.NotificationChannelCompat;
import androidx.core.app.NotificationCompat;
import androidx.core.app.NotificationManagerCompat;
import androidx.core.app.ServiceCompat;

/**
 * The call's foreground service (type microphone): it keeps the process
 * and its mic while the app is in the background or the screen is off, and
 * shows the ongoing call notification with Mute and Leave. It holds no call
 * state: VoiceShim starts it when the core's call starts and stops it when
 * the last one ends, and the actions go through VoiceShim to the core.
 *
 * Started only while the app is in front (a join is a tap), which is what
 * lets a microphone service start and keep the mic afterwards.
 */
public final class VoiceService extends Service {
    private static final String TAG = "yap-voice";

    static final String ACTION_START = "dev.yap.voice.START";
    static final String ACTION_STOP = "dev.yap.voice.STOP";
    static final String ACTION_REFRESH = "dev.yap.voice.REFRESH";
    static final String ACTION_MUTE = "dev.yap.voice.MUTE";
    static final String ACTION_LEAVE = "dev.yap.voice.LEAVE";

    private static final String CHANNEL = "call";
    /** Clear of the message notifications' ids (conversation hashes). */
    private static final int NOTIFICATION_ID = 0x59A9;

    /** The service is up and in the foreground (main thread). */
    private static boolean running;
    /** The created service, for stopping it without a start command. */
    private static VoiceService instance;
    /** The newest start command's id. */
    private int lastStartId;

    static Intent intent(Context context, String action) {
        return new Intent(context, VoiceService.class).setAction(action);
    }

    /** The call ended. Through the service itself, so a start still on its
     *  way is answered (startForeground) before it stops. */
    static void stop(Context context) {
        deliver(context, ACTION_STOP);
    }

    /**
     * The process is about to go: stopped here and now, since a start
     * command would outlive the process and bring it back just to be
     * delivered. A start queued behind the last one seen keeps the service
     * (it carries a later id).
     */
    static void stopNow() {
        if (instance != null) instance.finish(instance.lastStartId);
    }

    /** The notification's text or actions changed. */
    static void refresh(Context context) {
        if (running) deliver(context, ACTION_REFRESH);
    }

    private static void deliver(Context context, String action) {
        if (context == null) return;
        try {
            context.startService(intent(context, action));
        } catch (RuntimeException e) {
            // Not running, and the app is in the background: nothing to tell.
            Log.d(TAG, action + " not delivered: " + e);
        }
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    @Override
    public void onCreate() {
        super.onCreate();
        instance = this;
        // Created with no call and no start command behind it: the system
        // brought the service back after the process died under it (seen on
        // Android 15 for a foreground service, START_NOT_STICKY or not), and
        // would keep its notification up over nothing. Checked a moment
        // later, not here: a start command queued with the creation must be
        // answered first (onStartCommand stops the service itself then).
        if (!VoiceShim.active()) {
            new Handler(Looper.getMainLooper()).postDelayed(() -> {
                if (instance == this && !running && !VoiceShim.active()) {
                    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE);
                    stopSelf();
                }
            }, 2000);
        }
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        lastStartId = startId;
        String action = intent == null ? null : intent.getAction();
        if (ACTION_START.equals(action)) {
            // Always: a service started as a foreground one must become one.
            if (!foreground() || !VoiceShim.active()) finish(startId);
        } else if (ACTION_MUTE.equals(action)) {
            VoiceShim.toggleMute();
        } else if (ACTION_LEAVE.equals(action)) {
            VoiceShim.leave();
        } else if (ACTION_REFRESH.equals(action)) {
            if (running) post();
        } else if (ACTION_STOP.equals(action)) {
            // A call that began since (a leave, then a join) keeps the
            // service: its own start is on its way.
            if (!VoiceShim.active()) finish(startId);
        }
        // Started by a stale notification's action after the process died:
        // there is no call to act on. With the id, so a start queued behind
        // this one still gets to become a foreground service.
        if (!running) stopSelf(startId);
        return START_NOT_STICKY;
    }

    /**
     * The task was swiped away. Qt ends the process when its activity is
     * destroyed, so the room is told now (YapActivity.onDestroy does the
     * same, whichever comes first).
     */
    @Override
    public void onTaskRemoved(Intent rootIntent) {
        VoiceShim.quit();
        super.onTaskRemoved(rootIntent);
    }

    @Override
    public void onDestroy() {
        running = false;
        if (instance == this) instance = null;
        super.onDestroy();
    }

    private boolean foreground() {
        NotificationManagerCompat.from(this).createNotificationChannel(
                new NotificationChannelCompat.Builder(CHANNEL, NotificationManagerCompat.IMPORTANCE_LOW)
                        .setName("Voice calls")
                        .setDescription("Shown while you are in a voice channel")
                        .setShowBadge(false)
                        .build());
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, build(),
                    Build.VERSION.SDK_INT >= 30 ? ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE : 0);
            running = true;
            return true;
        } catch (RuntimeException e) {
            // No microphone permission, or (Android 12+) the app is not in
            // front any more: the call carries on without the service while
            // the app stays visible.
            Log.w(TAG, "not a foreground service: " + e);
            return false;
        }
    }

    /** Stops, unless a newer start is queued (it carries a later id). */
    private void finish(int startId) {
        running = false;
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE);
        stopSelf(startId);
    }

    @SuppressWarnings("MissingPermission")
    private void post() {
        // Without the notification permission the service still runs; the
        // system lists it under its active apps.
        try {
            NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, build());
        } catch (RuntimeException e) {
            Log.d(TAG, "notification not updated: " + e);
        }
    }

    private Notification build() {
        boolean deafened = VoiceShim.deafened();
        boolean muted = VoiceShim.muted() || deafened;
        String title = VoiceShim.title();
        NotificationCompat.Builder builder = new NotificationCompat.Builder(this, CHANNEL)
                .setSmallIcon(drawable("yap_notify_small"))
                .setColor(accent())
                .setContentTitle(title.isEmpty() ? "Voice call" : title)
                .setContentText(deafened ? "Deafened" : muted ? "Muted" : "Voice connected")
                .setCategory(NotificationCompat.CATEGORY_CALL)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setSilent(true)
                .setShowWhen(true)
                .setWhen(VoiceShim.startedAt())
                .setUsesChronometer(true)
                .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
                .addAction(0, muted ? "Unmute" : "Mute", action(ACTION_MUTE, 1))
                .addAction(0, "Leave", action(ACTION_LEAVE, 2));
        Intent open = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (open != null) {
            builder.setContentIntent(PendingIntent.getActivity(this, 0, open, PendingIntent.FLAG_IMMUTABLE));
        }
        return builder.build();
    }

    private PendingIntent action(String action, int request) {
        return PendingIntent.getService(this, request, intent(this, action),
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    private int drawable(String name) {
        int id = getResources().getIdentifier(name, "drawable", getPackageName());
        return id != 0 ? id : getApplicationInfo().icon;
    }

    private int accent() {
        int id = getResources().getIdentifier("yap_accent", "color", getPackageName());
        return id != 0 ? getColor(id) : NotificationCompat.COLOR_DEFAULT;
    }
}
