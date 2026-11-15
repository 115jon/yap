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

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * The call's foreground service (type microphone, camera while the phone's
 * camera captures, mediaProjection while its screen is shared): it keeps
 * the process and its mic while the app is in the background or the screen
 * is off, and shows the ongoing call notification with Mute and Leave (and
 * Stop sharing while the screen is shared). It holds no call state:
 * VoiceShim starts it when the core's call starts and stops it when the
 * last one ends, and the actions go through VoiceShim (ScreenShim) to the
 * core.
 *
 * Started only while the app is in front (a join is a tap), which is what
 * lets a microphone service start and keep the mic afterwards. The camera
 * type is added the same way: the camera starts from a tap in front
 * (CameraShim), and goes again when it stops. The mediaProjection type is
 * added after the user consented to a share and before the capture takes
 * the projection (ScreenShim), which Android 14 requires, and goes when the
 * capture stops.
 */
public final class VoiceService extends Service {
    private static final String TAG = "yap-voice";

    static final String ACTION_START = "dev.yap.voice.START";
    static final String ACTION_STOP = "dev.yap.voice.STOP";
    static final String ACTION_REFRESH = "dev.yap.voice.REFRESH";
    static final String ACTION_MUTE = "dev.yap.voice.MUTE";
    static final String ACTION_LEAVE = "dev.yap.voice.LEAVE";
    static final String ACTION_TYPE = "dev.yap.voice.TYPE";
    static final String ACTION_STOP_SHARE = "dev.yap.voice.STOP_SHARE";

    private static final String CHANNEL = "call";
    /** Clear of the message notifications' ids (conversation hashes). */
    private static final int NOTIFICATION_ID = 0x59A9;

    /** The service is up and in the foreground (written on the main thread). */
    private static volatile boolean running;
    /** The phone's camera captures: the service carries the camera type. */
    private static volatile boolean camera;
    /** The screen is about to be or is shared: the mediaProjection type. */
    private static volatile boolean projection;
    /** The type the service runs with now (written on the main thread). */
    private static volatile int runningType;
    /** Waiting for the next type change to be applied (main thread). */
    private static final List<Runnable> typeApplied = new ArrayList<>();
    private static final Handler main = new Handler(Looper.getMainLooper());
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

    /**
     * The camera started or stopped (any thread): a running service takes
     * the camera type while it captures, and drops it after.
     */
    static void setCamera(Context context, boolean on) {
        main.post(() -> {
            if (camera == on) return;
            camera = on;
            if (running) deliver(context, ACTION_TYPE);
        });
    }

    /**
     * The screen share's capture is about to start or has stopped (any
     * thread but the main one when waiting): a running service takes the
     * mediaProjection type, or drops it. With `waitMs`, waits that long for
     * the type to be applied; returns whether the service then runs with it
     * (always true for `on` false).
     */
    static boolean setProjection(Context context, boolean on, long waitMs) {
        CountDownLatch done = new CountDownLatch(1);
        main.post(() -> {
            projection = on;
            if (!running || type() == runningType) {
                done.countDown();
                return;
            }
            typeApplied.add(done::countDown);
            deliver(context, ACTION_TYPE);
        });
        if (!on) return true;
        try {
            done.await(waitMs, TimeUnit.MILLISECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        return projecting();
    }

    /** The service runs with the mediaProjection type (or needs none, before API 29). */
    private static boolean projecting() {
        if (!running) return false;
        if (Build.VERSION.SDK_INT < 29) return true;
        return (runningType & projectionType()) != 0;
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
        } else if (ACTION_TYPE.equals(action)) {
            if (running && type() != runningType) foreground();
            for (Runnable done : typeApplied) done.run();
            typeApplied.clear();
        } else if (ACTION_STOP_SHARE.equals(action)) {
            ScreenShim.stopFromNotification();
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
        int type = type();
        try {
            try {
                ServiceCompat.startForeground(this, NOTIFICATION_ID, build(), type);
            } catch (SecurityException e) {
                int extra = cameraType() | projectionType();
                if ((type & extra) == 0) throw e;
                // The camera or projection type was refused (the app went
                // behind first, no consent): the call keeps its microphone
                // service, and the capture that wanted the type fails.
                Log.w(TAG, "no camera or mediaProjection type: " + e);
                type &= ~extra;
                ServiceCompat.startForeground(this, NOTIFICATION_ID, build(), type);
            }
            running = true;
            runningType = type;
            return true;
        } catch (RuntimeException e) {
            // No microphone permission, or (Android 12+) the app is not in
            // front any more: the call carries on without the service while
            // the app stays visible.
            Log.w(TAG, "not a foreground service: " + e);
            return false;
        }
    }

    /**
     * Microphone, camera while it captures and mediaProjection while the
     * screen is shared (API 30+; before, only API 29's mediaProjection).
     */
    private static int type() {
        int share = projection ? projectionType() : 0;
        if (Build.VERSION.SDK_INT < 30) return share;
        return ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE | (camera ? cameraType() : 0) | share;
    }

    private static int cameraType() {
        return Build.VERSION.SDK_INT >= 30 ? ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA : 0;
    }

    private static int projectionType() {
        return Build.VERSION.SDK_INT >= 29 ? ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION : 0;
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
        boolean sharing = ScreenShim.sharing();
        String title = VoiceShim.title();
        String state = deafened ? "Deafened" : muted ? "Muted" : "Voice connected";
        NotificationCompat.Builder builder = new NotificationCompat.Builder(this, CHANNEL)
                .setSmallIcon(drawable("yap_notify_small"))
                .setColor(accent())
                .setContentTitle(title.isEmpty() ? "Voice call" : title)
                .setContentText(sharing ? state + " · Sharing your screen" : state)
                .setCategory(NotificationCompat.CATEGORY_CALL)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setSilent(true)
                .setShowWhen(true)
                .setWhen(VoiceShim.startedAt())
                .setUsesChronometer(true)
                .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
                .addAction(0, muted ? "Unmute" : "Mute", action(ACTION_MUTE, 1));
        if (sharing) builder.addAction(0, "Stop sharing", action(ACTION_STOP_SHARE, 3));
        builder.addAction(0, "Leave", action(ACTION_LEAVE, 2));
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
