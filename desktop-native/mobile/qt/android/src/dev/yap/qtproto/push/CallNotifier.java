package dev.yap.qtproto.push;

import android.annotation.SuppressLint;
import android.app.Notification;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.graphics.Bitmap;
import android.media.AudioAttributes;
import android.media.RingtoneManager;
import android.net.Uri;
import android.provider.Settings;
import android.service.notification.StatusBarNotification;
import android.util.Log;

import androidx.core.app.NotificationChannelCompat;
import androidx.core.app.NotificationCompat;
import androidx.core.app.NotificationManagerCompat;
import androidx.core.app.Person;
import androidx.core.graphics.drawable.IconCompat;

import java.util.Arrays;

/**
 * The incoming call's notifications (the Expo app's
 * IncomingCallNotification.kt). Ringing: CallStyle.forIncomingCall with
 * Decline and Answer, and a full-screen intent that opens the app over the
 * lock screen on its incoming sheet (Android shows a heads-up instead while
 * the phone is in use, or when full-screen notifications are not allowed);
 * the ringtone loops from the channel, so silent mode and Do Not Disturb
 * apply. Missed: the same tag and id, so it replaces the ring; a tap opens
 * the DM.
 */
final class CallNotifier {
    private CallNotifier() {}

    private static final String TAG = "yap-call";

    // Channel ids are stable: renaming one orphans what the user set on it.
    static final String CHANNEL_RINGING = "incoming_calls";
    static final String CHANNEL_MISSED = "missed_calls";

    /** One notification per DM, told apart by tag: the ring, then the missed call. */
    private static final int ID = 0x59_43_4C;

    /** The activity's actions (YapActivity hands them to IncomingCalls). */
    static final String ACTION_RING = "dev.yap.call.RING";
    static final String ACTION_ANSWER = "dev.yap.call.ANSWER";
    private static final String ACTIVITY = "dev.yap.qtproto.YapActivity";
    /** The ringing notification's call, so an Answer for it can be told apart. */
    private static final String EXTRA_CALL_ID = "dev.yap.call.CALL_ID";

    static void ensureChannels(Context context) {
        Uri ringtone = RingtoneManager.getActualDefaultRingtoneUri(context, RingtoneManager.TYPE_RINGTONE);
        if (ringtone == null) ringtone = Settings.System.DEFAULT_RINGTONE_URI;
        NotificationManagerCompat.from(context).createNotificationChannelsCompat(Arrays.asList(
                new NotificationChannelCompat.Builder(CHANNEL_RINGING, NotificationManagerCompat.IMPORTANCE_HIGH)
                        .setName("Incoming calls")
                        .setDescription("Rings when someone calls you")
                        .setSound(ringtone, new AudioAttributes.Builder()
                                .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                                .build())
                        .setVibrationEnabled(true)
                        .setVibrationPattern(new long[] {0, 800, 800, 800, 800})
                        .build(),
                new NotificationChannelCompat.Builder(CHANNEL_MISSED, NotificationManagerCompat.IMPORTANCE_DEFAULT)
                        .setName("Missed calls")
                        .setDescription("Calls you didn't answer")
                        .build()));
    }

    @SuppressLint("MissingPermission")
    static void postRinging(Context context, IncomingCall call, Bitmap avatar) {
        NotificationManagerCompat manager = NotificationManagerCompat.from(context);
        if (!manager.areNotificationsEnabled()) {
            Log.i(TAG, "notifications are off: the ring is not shown");
            return;
        }
        ensureChannels(context);
        Person caller = new Person.Builder()
                .setKey(call.callerId)
                .setName(call.callerName)
                .setIcon(avatar != null ? IconCompat.createWithBitmap(avatar) : null)
                .setImportant(true)
                .build();
        PendingIntent screen = activityIntent(context, call, ACTION_RING);
        NotificationCompat.CallStyle style = NotificationCompat.CallStyle.forIncomingCall(
                caller,
                CallActionReceiver.declineIntent(context, call),
                // An activity, not a receiver: Android 12+ refuses activity
                // starts from a notification's broadcast (trampolines), and
                // Answer opens the app.
                activityIntent(context, call, ACTION_ANSWER));
        Notification notification = new NotificationCompat.Builder(context, CHANNEL_RINGING)
                .setSmallIcon(drawable(context, "yap_notify_call"))
                .setColor(accent(context))
                .setContentTitle(call.callerName)
                .setContentText("Incoming voice call")
                .setStyle(style)
                .setCategory(NotificationCompat.CATEGORY_CALL)
                .setPriority(NotificationCompat.PRIORITY_MAX)
                .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
                .setFullScreenIntent(screen, true)
                .setContentIntent(screen)
                .setOngoing(true)
                .setAutoCancel(false)
                .setShowWhen(false)
                .setTimeoutAfter(call.ringSeconds * 1000L + 5_000L)
                .addExtras(callExtras(call))
                .build();
        // The ringtone repeats until the ring is answered or taken down.
        notification.flags |= Notification.FLAG_INSISTENT;
        try {
            manager.notify(call.channelId, ID, notification);
        } catch (RuntimeException e) {
            Log.w(TAG, "could not post the incoming call", e);
        }
    }

    /** This call's ring is up (posted by this process or one let go since). */
    static boolean isRinging(Context context, IncomingCall call) {
        NotificationManager manager = context.getSystemService(NotificationManager.class);
        if (manager == null) return false;
        for (StatusBarNotification n : manager.getActiveNotifications()) {
            Notification shown = n.getNotification();
            if (n.getId() == ID && call.channelId.equals(n.getTag())
                    && CHANNEL_RINGING.equals(shown.getChannelId())
                    && call.callId.equals(shown.extras.getString(EXTRA_CALL_ID))) {
                return true;
            }
        }
        return false;
    }

    private static android.os.Bundle callExtras(IncomingCall call) {
        android.os.Bundle extras = new android.os.Bundle();
        extras.putString(EXTRA_CALL_ID, call.callId);
        return extras;
    }

    static void cancel(Context context, String channelId) {
        NotificationManagerCompat.from(context).cancel(channelId, ID);
    }

    /** Every ring and missed call (signed out). */
    static void cancelAll(Context context) {
        NotificationManager manager = context.getSystemService(NotificationManager.class);
        if (manager == null) return;
        for (StatusBarNotification n : manager.getActiveNotifications()) {
            if (n.getId() == ID) manager.cancel(n.getTag(), ID);
        }
    }

    @SuppressLint("MissingPermission")
    static void postMissed(Context context, IncomingCall call, Bitmap avatar) {
        NotificationManagerCompat manager = NotificationManagerCompat.from(context);
        if (!manager.areNotificationsEnabled()) return;
        ensureChannels(context);
        // The DM, as a message notification's tap opens it (PushShim).
        Intent open = new Intent(MessageNotifier.ACTION_OPEN)
                .setClassName(context, ACTIVITY)
                .putExtra(MessageNotifier.EXTRA_CHANNEL, call.channelId)
                .putExtra(MessageNotifier.EXTRA_USER, call.callerId)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent tap = PendingIntent.getActivity(context, call.channelId.hashCode() * 4 + 2, open,
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        Notification notification = new NotificationCompat.Builder(context, CHANNEL_MISSED)
                .setSmallIcon(drawable(context, "yap_notify_call_missed"))
                .setColor(accent(context))
                .setContentTitle("Missed call")
                .setContentText(call.callerName)
                .setLargeIcon(avatar)
                .setCategory(NotificationCompat.CATEGORY_MISSED_CALL)
                .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                .setAutoCancel(true)
                .setShowWhen(true)
                .setWhen(System.currentTimeMillis())
                .setContentIntent(tap)
                .build();
        try {
            manager.notify(call.channelId, ID, notification);
        } catch (RuntimeException e) {
            Log.w(TAG, "could not post the missed call", e);
        }
    }

    /** The app on its incoming sheet (`ACTION_RING`), or answering (`ACTION_ANSWER`). */
    private static PendingIntent activityIntent(Context context, IncomingCall call, String action) {
        Intent intent = call.putInto(new Intent(action).setClassName(context, ACTIVITY))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        return PendingIntent.getActivity(context,
                call.callId.hashCode() * 4 + (ACTION_ANSWER.equals(action) ? 1 : 0), intent,
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    /** A resource by name (R's package is the application id; res/raw/yap_push_keep.xml keeps it). */
    private static int drawable(Context context, String name) {
        int id = context.getResources().getIdentifier(name, "drawable", context.getPackageName());
        return id != 0 ? id : android.R.drawable.sym_call_incoming;
    }

    private static int accent(Context context) {
        int id = context.getResources().getIdentifier("yap_accent", "color", context.getPackageName());
        return id != 0 ? context.getColor(id) : NotificationCompat.COLOR_DEFAULT;
    }
}
