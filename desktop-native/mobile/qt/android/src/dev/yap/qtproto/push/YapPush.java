package dev.yap.qtproto.push;

import java.time.Instant;
import java.util.Map;

/**
 * A Yap data-only push (src/services/push.service.ts on the server,
 * `buildNativeMessagePush` and `sendReadPushes`), from FCM's data map.
 * `ralph` = "2" marks the format; anything else is not ours.
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
        String senderId;
        String senderName;
        String senderAvatar;
        /** "#general" for a server channel; null for a DM. */
        String conversation;
        String serverName;
        String preview;
        long sentAt;

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
        String name = nonEmpty(data.get("senderName"));
        m.senderName = name != null ? name : "Someone";
        m.senderAvatar = nonEmpty(data.get("senderAvatar"));
        m.conversation = nonEmpty(data.get("conversation"));
        m.serverName = nonEmpty(data.get("serverName"));
        String preview = data.get("preview");
        m.preview = preview != null ? preview : "";
        m.sentAt = epochMillis(data.get("sentAt"));
        return m;
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
