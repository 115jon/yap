package dev.yap.qtproto.push;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Build;
import android.provider.Settings;
import android.util.Log;

import androidx.core.app.NotificationManagerCompat;

import com.google.firebase.FirebaseApp;
import com.google.firebase.messaging.FirebaseMessaging;

/**
 * The push side of `Native` (cpp/push.cpp): the FCM token, a notification's
 * tap, Reply and Mark as read into the core while the app is in front, the
 * conversation on screen (its pushes stay quiet), and the notification
 * permission. Otherwise (the app in the background or killed) the service
 * and the receiver work on their own (PushStore, PushApi).
 */
public final class PushShim {
    private PushShim() {}

    private static final String TAG = "yap-push";

    static native void nativeToken(String token);
    static native void nativeOpen(String channelId, String serverId, String userId);
    static native void nativeReply(String channelId, String messageId, String text);
    static native void nativeMarkRead(String channelId);

    private static Activity activity;
    /** Qt is up and the callbacks above are registered. */
    private static volatile boolean running;
    /** The last intent handleIntent opened a conversation for. */
    private static Intent handled;
    /** A tap that came before install() (YapActivity.offer). */
    private static Intent pending;
    /** The conversation on screen, with the app in front (else null). */
    private static volatile String active;

    /**
     * After the natives are registered: where the core keeps its session
     * and the API's origin (for the service when the app is killed), the
     * channels, a tap on a notification that started or resumed the
     * activity before Qt was up (YapActivity), and the FCM token.
     */
    public static void install(Activity a, String dataDir, String apiBase) {
        activity = a;
        Context app = a.getApplicationContext();
        PushStore.remember(app, dataDir, apiBase);
        running = true;
        MessageNotifier.ensureChannels(app);
        Intent tap;
        synchronized (PushShim.class) {
            tap = pending;
            pending = null;
        }
        handleIntent(tap);
        fetchToken(app);
    }

    static boolean running() {
        return running;
    }

    /**
     * The activity's intent (YapActivity: a fresh start's, and each
     * onNewIntent): opened now while Qt runs, else kept for install().
     */
    public static void offer(Intent intent) {
        if (intent == null || !MessageNotifier.ACTION_OPEN.equals(intent.getAction())) return;
        if (running) {
            handleIntent(intent);
            return;
        }
        synchronized (PushShim.class) {
            pending = intent;
        }
    }

    /** A notification's tap: the conversation to open. */
    private static void handleIntent(Intent intent) {
        if (intent == null || !MessageNotifier.ACTION_OPEN.equals(intent.getAction())) return;
        // Each once: the activity keeps its intent (left as it is: the back
        // handling reads it, NativeShim.keepsTaskAtRoot).
        synchronized (PushShim.class) {
            if (intent == handled) return;
            handled = intent;
        }
        String channel = intent.getStringExtra(MessageNotifier.EXTRA_CHANNEL);
        String server = intent.getStringExtra(MessageNotifier.EXTRA_SERVER);
        String user = intent.getStringExtra(MessageNotifier.EXTRA_USER);
        if (channel == null || channel.isEmpty()) return;
        nativeOpen(channel, server == null ? "" : server, user == null ? "" : user);
    }

    private static void fetchToken(Context app) {
        try {
            if (FirebaseApp.getApps(app).isEmpty()) {
                Log.i(TAG, "no Firebase configuration in this build: no pushes");
                return;
            }
            FirebaseMessaging.getInstance().getToken().addOnCompleteListener(task -> {
                if (task.isSuccessful() && task.getResult() != null) {
                    nativeToken(task.getResult());
                } else {
                    // No Play services, or FCM unreachable: no pushes for now.
                    Exception e = task.getException();
                    Log.w(TAG, "no FCM token: " + (e == null ? "none" : e.getClass().getSimpleName()));
                }
            });
        } catch (RuntimeException e) {
            Log.w(TAG, "FCM unavailable: " + e.getClass().getSimpleName());
        }
    }

    static void token(String token) {
        if (running) nativeToken(token);
    }

    static void reply(String channelId, String messageId, String text) {
        nativeReply(channelId, messageId == null ? "" : messageId, text);
    }

    static void markRead(String channelId) {
        nativeMarkRead(channelId);
    }

    /** The conversation on screen (empty: none, or the app is away). */
    public static void setActiveConversation(String channelId) {
        active = channelId == null || channelId.isEmpty() ? null : channelId;
        if (active != null && activity != null) {
            // Seen now: its notification goes.
            MessageNotifier.cancel(activity.getApplicationContext(), active);
        }
    }

    static String activeConversation() {
        return active;
    }

    public static boolean notificationsEnabled() {
        return activity != null && NotificationManagerCompat.from(activity).areNotificationsEnabled();
    }

    /**
     * Whether the permission dialog can still show (API 33+, not granted,
     * and never asked or not refused for good); otherwise only the app's
     * notification settings can turn notifications on.
     */
    public static boolean canAsk() {
        if (activity == null || Build.VERSION.SDK_INT < 33) return false;
        if (activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                == PackageManager.PERMISSION_GRANTED) {
            return false;
        }
        return !PushStore.asked(activity)
                || activity.shouldShowRequestPermissionRationale(Manifest.permission.POST_NOTIFICATIONS);
    }

    /** The dialog was never shown (the first channel opened asks once). */
    public static boolean neverAsked() {
        return activity != null && Build.VERSION.SDK_INT >= 33 && !PushStore.asked(activity);
    }

    public static void markAsked() {
        if (activity != null) PushStore.markAsked(activity);
    }

    public static void openNotificationSettings() {
        if (activity == null) return;
        Intent intent = new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, activity.getPackageName());
        activity.runOnUiThread(() -> {
            try {
                activity.startActivity(intent);
            } catch (RuntimeException e) {
                Log.w(TAG, "no notification settings", e);
            }
        });
    }

    /** Signed out: no notification or shortcut outlives the account. */
    public static void forgetNotifications() {
        if (activity == null) return;
        MessageNotifier.forgetAll(activity.getApplicationContext());
        IncomingCalls.forget(activity);
    }
}
