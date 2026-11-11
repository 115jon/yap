package dev.yap.qtproto.push;

import android.os.Bundle;

/** What a notification and its actions need to know about its conversation. */
final class Conversation {
    final String channelId;
    /** The server, for a channel the app has not loaded; null for a DM. */
    final String serverId;
    /** A DM's other person (it opens by them: the DM list may not be loaded yet). */
    final String userId;
    final boolean isDm;
    /** "#general · Server" for a channel; the person's name for a DM. */
    final String title;
    /** The Android notification channel it posts on. */
    final String channel;

    Conversation(String channelId, String serverId, String userId, boolean isDm, String title,
            String channel) {
        this.channelId = channelId;
        this.serverId = serverId;
        this.userId = userId;
        this.isDm = isDm;
        this.title = title;
        this.channel = channel;
    }

    static Conversation of(YapPush.Message push) {
        String title;
        if (push.isDm()) {
            title = push.senderName;
        } else {
            StringBuilder b = new StringBuilder();
            if (push.conversation != null) b.append(push.conversation);
            if (push.serverName != null) {
                if (b.length() > 0) b.append(" · ");
                b.append(push.serverName);
            }
            title = b.length() > 0 ? b.toString() : "Yap";
        }
        return new Conversation(push.channelId, push.serverId, push.isDm() ? push.senderId : null,
                push.isDm(), title,
                push.isDm() ? MessageNotifier.CHANNEL_DMS : MessageNotifier.CHANNEL_MENTIONS);
    }

    String shortcutId() {
        return "conversation:" + channelId;
    }

    /** The reply field's hint: "Message #general" or "Message Ada". */
    String replyLabel() {
        int cut = title.indexOf(" · ");
        return "Message " + (cut >= 0 ? title.substring(0, cut) : title);
    }

    int requestCode(int slot) {
        return channelId.hashCode() * 4 + slot;
    }

    Bundle toBundle() {
        Bundle b = new Bundle();
        b.putString("channelId", channelId);
        b.putString("serverId", serverId);
        b.putString("userId", userId);
        b.putBoolean("isDm", isDm);
        b.putString("title", title);
        b.putString("channel", channel);
        return b;
    }

    static Conversation from(Bundle b) {
        if (b == null) return null;
        String channelId = b.getString("channelId");
        if (channelId == null) return null;
        String title = b.getString("title");
        String channel = b.getString("channel");
        return new Conversation(channelId, b.getString("serverId"), b.getString("userId"),
                b.getBoolean("isDm"),
                title != null ? title : "Yap",
                channel != null ? channel : MessageNotifier.CHANNEL_OTHER);
    }
}
