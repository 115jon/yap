package dev.yap.qtproto.push;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.util.Log;

import androidx.core.app.RemoteInput;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Reply and Mark as read from a message notification. With the app in
 * front they go into the core (its toast reply and mark-read, as the
 * desktop's toasts do: its unread state follows at once, and a failed send
 * shows in the app). Otherwise, the process alive in the background or not
 * running at all, they call the API here with the core's stored session.
 * A backgrounded core cannot be trusted with them: Android suspends it and
 * blocks the app's network outside the action's broadcast, so its request
 * waited for the next resume, failed and was never retried. The broadcast
 * of a notification action comes with a temporary network allowance, and
 * goAsync keeps the receiver (and the allowance) alive for the request. The
 * core catches up when its gateway is back: the server's read push (or the
 * fresh session's read states) and the reply's echo.
 */
public final class NotificationActionReceiver extends BroadcastReceiver {
    static final String ACTION_REPLY = "reply";
    static final String ACTION_MARK_READ = "read";
    static final String EXTRA_CONVERSATION = "yap.conversation";
    static final String EXTRA_MESSAGE = "yap.replyTo";

    private static final String TAG = "yap-push";
    /** The pause before a Mark as read's one retry. */
    private static final long RETRY_MS = 1_500;
    /** A failure faster than this is retried (PushApi's timeouts are 10 s). */
    private static final long QUICK_MS = 3_000;

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
        if (PushShim.running() && MessageNotifier.inForeground()) {
            if (reply != null) {
                PushShim.reply(conversation.channelId, messageId, reply);
                MessageNotifier.showReply(app, conversation, reply, true);
            } else {
                MessageNotifier.cancel(app, conversation.channelId);
                PushShim.markRead(conversation.channelId);
            }
            return;
        }
        // The API, with the real outcome on the notification ("Not sent:
        // ..." on a failure). The gateway's echo brings a reply into the
        // app's chat once it is back.
        final String text = reply;
        final PendingResult pending = goAsync();
        WORKER.execute(() -> {
            try {
                PushStore.Session session = PushStore.session(app);
                if (session == null) {
                    MessageNotifier.cancel(app, conversation.channelId);
                } else if (text != null) {
                    // Not retried: a send that timed out may have landed.
                    int status = PushApi.reply(session, conversation.channelId, messageId, text);
                    MessageNotifier.showReply(app, conversation, text, ok(status));
                } else {
                    // Down at once; the server's read push clears other devices.
                    // Its notifications too, as the core's Mark as read does,
                    // or its mentions stay counted past the read mark.
                    // Both idempotent: retried once after a quick failure
                    // (the network not there yet); a slow one is not, so
                    // the work stays well inside the broadcast's time.
                    MessageNotifier.cancel(app, conversation.channelId);
                    long start = SystemClock.elapsedRealtime();
                    int read = PushApi.markRead(session, conversation.channelId);
                    if (retry(read) && quick(start)) {
                        pause();
                        read = PushApi.markRead(session, conversation.channelId);
                    }
                    start = SystemClock.elapsedRealtime();
                    int notifications = PushApi.markNotificationsRead(session, conversation.channelId);
                    if (notifications < 0 && quick(start)) {
                        pause();
                        notifications = PushApi.markNotificationsRead(session, conversation.channelId);
                    }
                    Log.i(TAG, "mark as read: read state " + read + ", notifications " + notifications);
                }
            } finally {
                pending.finish();
            }
        });
    }

    private static boolean ok(int status) {
        return status >= 200 && status < 300;
    }

    /** No answer, or the server's own failure: worth one more try. */
    private static boolean retry(int status) {
        return status < 0 || status >= 500;
    }

    /** Failed within QUICK_MS of `start`: a refused lookup or connection, not a timeout. */
    private static boolean quick(long start) {
        return SystemClock.elapsedRealtime() - start < QUICK_MS;
    }

    private static void pause() {
        try {
            Thread.sleep(RETRY_MS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }
}
