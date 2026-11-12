package dev.yap.qtproto.push;

import android.annotation.SuppressLint;
import android.app.ActivityManager;
import android.app.Notification;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.BitmapShader;
import android.graphics.Canvas;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.Shader;
import android.os.Build;
import android.os.Bundle;
import android.service.notification.StatusBarNotification;
import android.util.Log;

import androidx.core.app.NotificationChannelCompat;
import androidx.core.app.NotificationCompat;
import androidx.core.app.NotificationManagerCompat;
import androidx.core.app.Person;
import androidx.core.app.RemoteInput;
import androidx.core.content.LocusIdCompat;
import androidx.core.content.pm.ShortcutInfoCompat;
import androidx.core.content.pm.ShortcutManagerCompat;
import androidx.core.graphics.drawable.IconCompat;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Message notifications drawn on the device, as the Expo app's
 * MessageNotifier.kt draws them: one MessagingStyle notification per
 * conversation (sender avatars, the conversation's title, the last few
 * messages), grouped under a summary, with Reply and Mark as read, in the
 * brand accent. Each conversation also gets a long-lived sharing shortcut,
 * which puts it in Android 11+'s Conversations section.
 */
final class MessageNotifier {
    private MessageNotifier() {}

    private static final String TAG = "yap-push";

    // Channels people tune in the system settings. The ids are stable API
    // (the Expo app's): renaming one orphans what was set on it.
    static final String CHANNEL_DMS = "dms";
    static final String CHANNEL_MENTIONS = "mentions";
    static final String CHANNEL_OTHER = "other";

    private static final String GROUP = "yap.messages";
    /** Conversations are told apart by tag (the channel id); the id is shared. */
    private static final int CONVERSATION_ID = 1;
    private static final int SUMMARY_ID = 0x5961;
    private static final String SUMMARY_TAG = "yap.summary";
    private static final int MAX_MESSAGES = 8;
    private static final String EXTRA_MESSAGE_ID = "yap.messageId";
    private static final String EXTRA_CONVERSATION = "yap.conversation";

    static final String REMOTE_INPUT_KEY = "yap.reply";

    /** The open-conversation intent's action and extras (PushShim reads them). */
    static final String ACTION_OPEN = "dev.yap.notify.OPEN";
    static final String EXTRA_CHANNEL = "yap.channelId";
    static final String EXTRA_SERVER = "yap.serverId";
    static final String EXTRA_USER = "yap.userId";
    private static final String ACTIVITY = "dev.yap.qtproto.YapActivity";

    static void ensureChannels(Context context) {
        NotificationManagerCompat manager = NotificationManagerCompat.from(context);
        manager.createNotificationChannelsCompat(Arrays.asList(
                new NotificationChannelCompat.Builder(CHANNEL_DMS, NotificationManagerCompat.IMPORTANCE_HIGH)
                        .setName("Direct messages")
                        .setDescription("Messages sent to you directly")
                        .build(),
                new NotificationChannelCompat.Builder(CHANNEL_MENTIONS, NotificationManagerCompat.IMPORTANCE_HIGH)
                        .setName("Mentions and replies")
                        .setDescription("When someone mentions you or replies to you in a server")
                        .build(),
                new NotificationChannelCompat.Builder(CHANNEL_OTHER, NotificationManagerCompat.IMPORTANCE_DEFAULT)
                        .setName("Other")
                        .setDescription("Everything else Yap tells you")
                        .build()));
    }

    /** Adds `push` to its conversation's notification, or starts one. */
    @SuppressLint("MissingPermission")
    static void show(Context context, YapPush.Message push) {
        NotificationManagerCompat manager = NotificationManagerCompat.from(context);
        if (!manager.areNotificationsEnabled()) return;
        // The conversation is on screen: the app shows it already.
        if (push.channelId.equals(PushShim.activeConversation()) && inForeground()) return;
        PushStore.Session session = PushStore.session(context);
        if (session == null) return;
        ensureChannels(context);

        IconCompat avatar = null;
        if (push.senderAvatar != null) {
            Bitmap bitmap = PushApi.avatar(session, push.senderAvatar);
            if (bitmap != null) avatar = IconCompat.createWithBitmap(circle(bitmap));
        }
        Person sender = new Person.Builder()
                .setKey(push.senderId)
                .setName(push.senderName)
                .setIcon(avatar)
                .build();
        NotificationCompat.MessagingStyle previous = activeStyle(context, push.channelId);
        if (previous != null) {
            for (NotificationCompat.MessagingStyle.Message m : previous.getMessages()) {
                if (push.messageId.equals(m.getExtras().getString(EXTRA_MESSAGE_ID))) return;
            }
        }
        Conversation conversation = Conversation.of(push);
        NotificationCompat.MessagingStyle style = previous != null ? previous : newStyle(conversation);
        NotificationCompat.MessagingStyle.Message message =
                new NotificationCompat.MessagingStyle.Message(push.preview, push.sentAt, sender);
        message.getExtras().putString(EXTRA_MESSAGE_ID, push.messageId);
        style.addMessage(message);
        trim(style);

        String shortcutId = publishShortcut(context, conversation, sender);
        Notification notification = builder(context, conversation, style)
                .setShortcutId(shortcutId)
                .setLocusId(new LocusIdCompat(shortcutId))
                .setWhen(push.sentAt)
                .build();
        post(context, conversation.channelId, notification);
        postSummary(context, conversation.channelId, notification);
    }

    /**
     * After an inline reply: the reply joins the conversation (or a line says
     * it failed), quietly, which also stops the action's spinner.
     */
    static void showReply(Context context, Conversation conversation, String text, boolean sent) {
        NotificationCompat.MessagingStyle style = activeStyle(context, conversation.channelId);
        if (style == null) style = newStyle(conversation);
        long now = System.currentTimeMillis();
        style.addMessage(sent
                ? new NotificationCompat.MessagingStyle.Message(text, now, self())
                // A message with no person reads as a line from the app.
                : new NotificationCompat.MessagingStyle.Message("Not sent: " + text, now, (Person) null));
        trim(style);
        Notification notification = builder(context, conversation, style)
                .setShortcutId(conversation.shortcutId())
                .setLocusId(new LocusIdCompat(conversation.shortcutId()))
                .setOnlyAlertOnce(true)
                .setSilent(true)
                .build();
        post(context, conversation.channelId, notification);
    }

    static void cancel(Context context, String channelId) {
        NotificationManagerCompat.from(context).cancel(channelId, CONVERSATION_ID);
        postSummary(context, channelId, null);
    }

    /** Sign-out: no notification or shortcut outlives the account. */
    static void forgetAll(Context context) {
        NotificationManagerCompat manager = NotificationManagerCompat.from(context);
        for (StatusBarNotification active : manager.getActiveNotifications()) {
            if (GROUP.equals(active.getNotification().getGroup())) {
                manager.cancel(active.getTag(), active.getId());
            }
        }
        ShortcutManagerCompat.removeAllDynamicShortcuts(context);
    }

    // ---- Building -----------------------------------------------------------

    private static Person self() {
        return new Person.Builder().setKey("self").setName("You").build();
    }

    private static NotificationCompat.MessagingStyle newStyle(Conversation conversation) {
        NotificationCompat.MessagingStyle style = new NotificationCompat.MessagingStyle(self());
        // A DM is titled by the person on its messages; a channel by its name.
        if (!conversation.isDm) {
            style.setConversationTitle(conversation.title);
            style.setGroupConversation(true);
        }
        return style;
    }

    private static NotificationCompat.Builder builder(Context context, Conversation conversation,
            NotificationCompat.MessagingStyle style) {
        List<NotificationCompat.MessagingStyle.Message> messages = style.getMessages();
        NotificationCompat.MessagingStyle.Message last =
                messages.isEmpty() ? null : messages.get(messages.size() - 1);
        Bundle extras = new Bundle();
        extras.putBundle(EXTRA_CONVERSATION, conversation.toBundle());
        return new NotificationCompat.Builder(context, conversation.channel)
                .setSmallIcon(drawable(context, "yap_notify_small"))
                .setColor(accent(context))
                .setStyle(style)
                .setCategory(NotificationCompat.CATEGORY_MESSAGE)
                .setGroup(GROUP)
                // The summary stays silent; each conversation alerts on its own.
                .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
                .setAutoCancel(true)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                .setPublicVersion(publicVersion(context, conversation))
                .setContentTitle(conversation.title)
                .setContentText(last != null ? last.getText() : null)
                .setContentIntent(openIntent(context, conversation))
                .addAction(replyAction(context, conversation, lastMessageId(style)))
                .addAction(markReadAction(context, conversation))
                .addExtras(extras);
    }

    /** What the lock screen shows when contents are hidden. */
    private static Notification publicVersion(Context context, Conversation conversation) {
        return new NotificationCompat.Builder(context, conversation.channel)
                .setSmallIcon(drawable(context, "yap_notify_small"))
                .setColor(accent(context))
                .setContentTitle(conversation.isDm ? "New direct message" : "New message")
                .build();
    }

    /** Keeps the last few messages, like the system's own messaging apps. */
    private static void trim(NotificationCompat.MessagingStyle style) {
        List<NotificationCompat.MessagingStyle.Message> messages = style.getMessages();
        while (messages.size() > MAX_MESSAGES) messages.remove(0);
    }

    /** The conversation's notification still in the tray, as a style to add to. */
    private static NotificationCompat.MessagingStyle activeStyle(Context context, String channelId) {
        for (StatusBarNotification active : NotificationManagerCompat.from(context).getActiveNotifications()) {
            if (channelId.equals(active.getTag()) && active.getId() == CONVERSATION_ID) {
                return NotificationCompat.MessagingStyle
                        .extractMessagingStyleFromNotification(active.getNotification());
            }
        }
        return null;
    }

    @SuppressLint("MissingPermission")
    private static void post(Context context, String channelId, Notification notification) {
        try {
            NotificationManagerCompat.from(context).notify(channelId, CONVERSATION_ID, notification);
        } catch (RuntimeException e) {
            Log.w(TAG, "could not post a message notification", e);
        }
    }

    /**
     * The group's summary while any conversation is up; gone with the last,
     * never posted without one. `changed` is the conversation just posted
     * (`posted`) or cancelled (null): notify and cancel reach the system
     * asynchronously, so the active list read right after may still show a
     * cancelled one (an empty "1 conversation" summary, which the system
     * then drops as a summary without children) or miss a new one.
     */
    @SuppressLint("MissingPermission")
    private static void postSummary(Context context, String changed, Notification posted) {
        NotificationManagerCompat manager = NotificationManagerCompat.from(context);
        List<Notification> conversations = new ArrayList<>();
        if (posted != null) conversations.add(posted);
        for (StatusBarNotification active : manager.getActiveNotifications()) {
            if (GROUP.equals(active.getNotification().getGroup()) && active.getId() == CONVERSATION_ID
                    && !changed.equals(active.getTag())) {
                conversations.add(active.getNotification());
            }
        }
        if (conversations.isEmpty()) {
            manager.cancel(SUMMARY_TAG, SUMMARY_ID);
            return;
        }
        NotificationCompat.InboxStyle lines = new NotificationCompat.InboxStyle();
        for (Notification active : conversations.subList(0, Math.min(5, conversations.size()))) {
            Bundle extras = active.extras;
            CharSequence title = extras.getCharSequence(NotificationCompat.EXTRA_TITLE);
            if (title == null) continue;
            CharSequence text = extras.getCharSequence(NotificationCompat.EXTRA_TEXT);
            lines.addLine(title + "  " + (text != null ? text : ""));
        }
        int count = conversations.size();
        String summaryText = count == 1 ? "1 conversation" : count + " conversations";
        Notification summary = new NotificationCompat.Builder(context, CHANNEL_OTHER)
                .setSmallIcon(drawable(context, "yap_notify_small"))
                .setColor(accent(context))
                .setStyle(lines.setSummaryText(summaryText))
                .setContentTitle("Yap")
                .setContentText(summaryText)
                .setGroup(GROUP)
                .setGroupSummary(true)
                .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
                .setSilent(true)
                .setAutoCancel(true)
                .setContentIntent(launchIntent(context))
                .build();
        try {
            manager.notify(SUMMARY_TAG, SUMMARY_ID, summary);
        } catch (RuntimeException e) {
            Log.w(TAG, "could not post the summary", e);
        }
    }

    // ---- Intents ------------------------------------------------------------

    /** Opens the app at the conversation (PushShim.handleIntent reads it). */
    static Intent openConversation(Context context, Conversation conversation) {
        Intent intent = new Intent(ACTION_OPEN)
                .setClassName(context, ACTIVITY)
                .putExtra(EXTRA_CHANNEL, conversation.channelId)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        if (conversation.serverId != null) intent.putExtra(EXTRA_SERVER, conversation.serverId);
        if (conversation.userId != null) intent.putExtra(EXTRA_USER, conversation.userId);
        return intent;
    }

    private static PendingIntent openIntent(Context context, Conversation conversation) {
        return PendingIntent.getActivity(context, conversation.requestCode(0),
                openConversation(context, conversation),
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    private static PendingIntent launchIntent(Context context) {
        Intent intent = context.getPackageManager().getLaunchIntentForPackage(context.getPackageName());
        if (intent == null) return null;
        return PendingIntent.getActivity(context, SUMMARY_ID, intent, PendingIntent.FLAG_IMMUTABLE);
    }

    private static PendingIntent actionIntent(Context context, String action, Conversation conversation,
            boolean mutable, String messageId) {
        Intent intent = new Intent(context, NotificationActionReceiver.class)
                .setAction(context.getPackageName() + ".notify." + action)
                .putExtra(NotificationActionReceiver.EXTRA_CONVERSATION, conversation.toBundle())
                .putExtra(NotificationActionReceiver.EXTRA_MESSAGE, messageId);
        // RemoteInput fills the intent in, so Reply's must be mutable (an
        // explicit intent to our own receiver, which Android 12+ allows).
        int mutability = !mutable ? PendingIntent.FLAG_IMMUTABLE
                : Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0;
        int request = conversation.requestCode(
                NotificationActionReceiver.ACTION_REPLY.equals(action) ? 1 : 2);
        return PendingIntent.getBroadcast(context, request, intent,
                PendingIntent.FLAG_UPDATE_CURRENT | mutability);
    }

    private static NotificationCompat.Action replyAction(Context context, Conversation conversation,
            String replyTo) {
        RemoteInput input = new RemoteInput.Builder(REMOTE_INPUT_KEY)
                .setLabel(conversation.replyLabel())
                .build();
        return new NotificationCompat.Action.Builder(
                IconCompat.createWithResource(context, drawable(context, "yap_notify_reply")),
                "Reply",
                actionIntent(context, NotificationActionReceiver.ACTION_REPLY, conversation, true, replyTo))
                .addRemoteInput(input)
                .setAllowGeneratedReplies(true)
                .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
                .setShowsUserInterface(false)
                .build();
    }

    private static NotificationCompat.Action markReadAction(Context context, Conversation conversation) {
        return new NotificationCompat.Action.Builder(
                IconCompat.createWithResource(context, drawable(context, "yap_notify_read")),
                "Mark as read",
                actionIntent(context, NotificationActionReceiver.ACTION_MARK_READ, conversation, false, null))
                .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_MARK_AS_READ)
                .setShowsUserInterface(false)
                .build();
    }

    /** The newest pushed message in the conversation: what Reply answers. */
    private static String lastMessageId(NotificationCompat.MessagingStyle style) {
        List<NotificationCompat.MessagingStyle.Message> messages = style.getMessages();
        for (int i = messages.size() - 1; i >= 0; i--) {
            String id = messages.get(i).getExtras().getString(EXTRA_MESSAGE_ID);
            if (id != null) return id;
        }
        return null;
    }

    // ---- Shortcuts ----------------------------------------------------------

    /** The conversation's long-lived shortcut, pushed (rate-limit safe) to the front. */
    private static String publishShortcut(Context context, Conversation conversation, Person sender) {
        IconCompat icon = conversation.isDm ? sender.getIcon() : null;
        if (icon == null) icon = IconCompat.createWithResource(context, context.getApplicationInfo().icon);
        CharSequence label = conversation.isDm && sender.getName() != null ? sender.getName() : conversation.title;
        ShortcutInfoCompat.Builder shortcut = new ShortcutInfoCompat.Builder(context, conversation.shortcutId())
                .setShortLabel(label)
                .setLongLived(true)
                .setLocusId(new LocusIdCompat(conversation.shortcutId()))
                .setIcon(icon)
                .setIntent(openConversation(context, conversation));
        if (conversation.isDm) shortcut.setPerson(sender);
        try {
            ShortcutManagerCompat.pushDynamicShortcut(context, shortcut.build());
        } catch (RuntimeException e) {
            Log.w(TAG, "could not publish a conversation shortcut", e);
        }
        return conversation.shortcutId();
    }

    // ---- Helpers --------------------------------------------------------------

    static boolean inForeground() {
        ActivityManager.RunningAppProcessInfo info = new ActivityManager.RunningAppProcessInfo();
        ActivityManager.getMyMemoryState(info);
        return info.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND;
    }

    /**
     * A resource of the app's by name: R's package is the application id,
     * which the build sets (CMake's YAP_ANDROID_APP_ID). res/raw/yap_push_keep.xml
     * keeps these through resource shrinking.
     */
    static int drawable(Context context, String name) {
        int id = context.getResources().getIdentifier(name, "drawable", context.getPackageName());
        return id != 0 ? id : context.getApplicationInfo().icon;
    }

    private static int accent(Context context) {
        int id = context.getResources().getIdentifier("yap_accent", "color", context.getPackageName());
        return id != 0 ? context.getColor(id) : NotificationCompat.COLOR_DEFAULT;
    }

    private static Bitmap circle(Bitmap source) {
        int size = Math.min(source.getWidth(), source.getHeight());
        Bitmap output = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888);
        BitmapShader shader = new BitmapShader(source, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP);
        Matrix matrix = new Matrix();
        matrix.setTranslate(-(source.getWidth() - size) / 2f, -(source.getHeight() - size) / 2f);
        shader.setLocalMatrix(matrix);
        Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        paint.setShader(shader);
        new Canvas(output).drawCircle(size / 2f, size / 2f, size / 2f, paint);
        return output;
    }
}
