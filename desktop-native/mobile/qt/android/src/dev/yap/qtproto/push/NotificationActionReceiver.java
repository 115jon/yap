package dev.yap.qtproto.push;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;

import androidx.core.app.RemoteInput;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Reply and Mark as read from a message notification. With the app
 * running (a reply: in front) they go into the core (its toast reply and
 * mark-read, as the desktop's toasts do); otherwise they call the API with
 * the core's stored session. goAsync keeps the receiver alive for the
 * request.
 */
public final class NotificationActionReceiver extends BroadcastReceiver {
    static final String ACTION_REPLY = "reply";
    static final String ACTION_MARK_READ = "read";
    static final String EXTRA_CONVERSATION = "yap.conversation";
    static final String EXTRA_MESSAGE = "yap.replyTo";

    private static final ExecutorService WORKER = Executors.newSingleThreadExecutor();

    @Override
    public void onReceive(Context context, Intent intent) {
        Bundle extras = intent.getExtras();
        Conversation conversation =
                Conversation.from(extras == null ? null : extras.getBundle(EXTRA_CONVERSATION));
        String action = intent.getAction();
        if (conversation == null || action == null) return;
        action = action.substring(action.lastIndexOf('.') + 1);
        final Context app = context.getApplicationContext();
        final String messageId = intent.getStringExtra(EXTRA_MESSAGE);
        String reply = null;
        if (ACTION_REPLY.equals(action)) {
            Bundle results = RemoteInput.getResultsFromIntent(intent);
            CharSequence text = results == null ? null : results.getCharSequence(MessageNotifier.REMOTE_INPUT_KEY);
            reply = text == null ? null : text.toString().trim();
            if (reply == null || reply.isEmpty()) return;
        } else if (!ACTION_MARK_READ.equals(action)) {
            return;
        }
        if (PushShim.running() && (reply == null || MessageNotifier.inForeground())) {
            // The core does it, as for a desktop toast: Mark as read whenever
            // the app runs (its unread state follows at once), a reply only
            // with the app in front, where a failed send shows in the app.
            if (reply != null) {
                PushShim.reply(conversation.channelId, messageId, reply);
                MessageNotifier.showReply(app, conversation, reply, true);
            } else {
                MessageNotifier.cancel(app, conversation.channelId);
                PushShim.markRead(conversation.channelId);
            }
            return;
        }
        // Otherwise the API, with the real outcome on the notification
        // ("Not sent: ..." on a failure). The gateway's echo brings a reply
        // into a running app's chat.
        final String text = reply;
        final PendingResult pending = goAsync();
        WORKER.execute(() -> {
            try {
                PushStore.Session session = PushStore.session(app);
                if (session == null) {
                    MessageNotifier.cancel(app, conversation.channelId);
                } else if (text != null) {
                    int status = PushApi.reply(session, conversation.channelId, messageId, text);
                    MessageNotifier.showReply(app, conversation, text, status >= 200 && status < 300);
                } else {
                    // Down at once; the server's read push clears other devices.
                    // Its notifications too, as the core's Mark as read does,
                    // or its mentions stay counted past the read mark.
                    MessageNotifier.cancel(app, conversation.channelId);
                    PushApi.markRead(session, conversation.channelId);
                    PushApi.markNotificationsRead(session, conversation.channelId);
                }
            } finally {
                pending.finish();
            }
        });
    }
}
