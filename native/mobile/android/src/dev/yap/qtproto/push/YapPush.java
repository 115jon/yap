package dev.yap.qtproto.push;

import java.time.Instant;
import java.util.Map;

/**
 * A Yap data-only push (src/services/push.service.ts on the server,
 * `buildNativeMessagePush` and `sendReadPushes`), from FCM's data map.
 * `ralph` = "2" marks the format; anything else is not ours. The call
 * kinds (`call_ring`, `call_ring_stop`, src/services/call.service.ts) are
 * read as an IncomingCall.
 *
 * A message push carries ids only (FCM never sees the text): the sender's
 * name, the conversation and the line to show are fetched by
 * `notificationId` (PushApi.notificationText) and filled in here.
 */
final class YapPush {
    private YapPush() {}

    static final String FORMAT = "2";

    /** A mention, reply or DM, drawn as a conversation message. */
    static final class Message {
        String type;
        String channelId;
        /** Null for a DM. */
        String serverId;
        String messageId;
        /** What the text is fetched by; null from a server that sends none. */
        String notificationId;
        String senderId;
        long sentAt;

        // Fetched (or, when the fetch fails, remembered from an earlier one).
        /** Null when not known: the notification then names no one. */
        String senderName;
        String senderAvatar;
        /** "#general" for a server channel; null for a DM or not known. */
        String conversation;
        String serverName;
        /** The line shown: the message's text, or the fallback's. */
        String preview;

        boolean isDm() {
            return serverId == null;
        }
    }

    static boolean isOurs(Map<String, String> data) {
        return FORMAT.equals(data.get("ralph"));
    }

    /** The conversation read somewhere: its channel id, or null. */
    static String readChannel(Map<String, String> data) {
        if (!isOurs(data) || !"read".equals(data.get("kind"))) return null;
        return nonEmpty(data.get("channelId"));
    }

    /** The message push in `data`, or null when it is not one. */
    static Message message(Map<String, String> data) {
        if (!isOurs(data) || !"message".equals(data.get("kind"))) return null;
        Message m = new Message();
        m.channelId = nonEmpty(data.get("channelId"));
        m.messageId = nonEmpty(data.get("messageId"));
        m.senderId = nonEmpty(data.get("senderId"));
        if (m.channelId == null || m.messageId == null || m.senderId == null) return null;
        m.type = data.containsKey("type") ? data.get("type") : "mention";
        m.serverId = nonEmpty(data.get("serverId"));
        m.notificationId = nonEmpty(data.get("notificationId"));
        m.sentAt = epochMillis(data.get("sentAt"));
        return m;
    }

    /** The line a message shows when its text could not be fetched. */
    static String fallbackText(String senderName) {
        return senderName != null ? "New message from " + senderName : "New message";
    }

    private static long epochMillis(String iso) {
        if (iso != null) {
            try {
                return Instant.parse(iso).toEpochMilli();
            } catch (RuntimeException e) {
                // Malformed: now, below.
            }
        }
        return System.currentTimeMillis();
    }

    private static String nonEmpty(String s) {
        return s == null || s.isEmpty() ? null : s;
    }
}
