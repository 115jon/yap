package dev.yap.qtproto.push;

import android.content.Intent;

import org.json.JSONException;
import org.json.JSONObject;

import java.util.Map;

/**
 * A DM call ringing this phone: from a `call_ring` push
 * (src/services/call.service.ts `buildCallRingPush` on the server) or the
 * gateway's CALL_RING through the core and the QML (Native.ringIncoming).
 * The keys are the push's own; a `call_ring_stop` push has the same ones
 * and a `reason`.
 */
final class IncomingCall {
    static final String KEY_CALL_ID = "callId";
    static final String KEY_CHANNEL_ID = "channelId";
    static final String KEY_CALLER_ID = "callerId";
    static final String KEY_CALLER_NAME = "callerName";
    static final String KEY_CALLER_AVATAR = "callerAvatar";
    static final String KEY_RING_SECONDS = "ringSeconds";

    /** The server rings for 30 s (meeting-room.ts CALL_RING_TIMEOUT_MS). */
    static final int DEFAULT_RING_SECONDS = 30;

    final String callId;
    final String channelId;
    final String callerId;
    final String callerName;
    /** As stored: may be an API-relative /api/... path; null for none. */
    final String callerAvatar;
    /** How long it may ring; the server's stop usually comes first. */
    final int ringSeconds;

    private IncomingCall(String callId, String channelId, String callerId, String callerName,
            String callerAvatar, int ringSeconds) {
        this.callId = callId;
        this.channelId = channelId;
        this.callerId = callerId;
        this.callerName = callerName;
        this.callerAvatar = callerAvatar;
        this.ringSeconds = ringSeconds;
    }

    interface Reader {
        String get(String key);
    }

    /** From push data, extras or the QML's map; null without a call and its DM. */
    static IncomingCall from(Reader read) {
        String callId = nonEmpty(read.get(KEY_CALL_ID));
        String channelId = nonEmpty(read.get(KEY_CHANNEL_ID));
        if (callId == null || channelId == null) return null;
        String callerId = read.get(KEY_CALLER_ID);
        String name = read.get(KEY_CALLER_NAME);
        int seconds = DEFAULT_RING_SECONDS;
        try {
            String s = read.get(KEY_RING_SECONDS);
            if (s != null) seconds = Integer.parseInt(s);
        } catch (NumberFormatException e) {
            // The default.
        }
        return new IncomingCall(callId, channelId, callerId == null ? "" : callerId,
                name == null || name.trim().isEmpty() ? "Someone" : name,
                nonEmpty(read.get(KEY_CALLER_AVATAR)), Math.max(5, Math.min(60, seconds)));
    }

    static IncomingCall from(Map<String, String> data) {
        return from(data::get);
    }

    static IncomingCall from(Intent intent) {
        return intent == null ? null : from(intent::getStringExtra);
    }

    static IncomingCall fromJson(String json) {
        try {
            JSONObject o = new JSONObject(json);
            return from(key -> o.has(key) && !o.isNull(key) ? o.optString(key) : null);
        } catch (JSONException e) {
            return null;
        }
    }

    /** The same call with its avatar as an absolute URL (for the QML). */
    IncomingCall withAvatar(String url) {
        return new IncomingCall(callId, channelId, callerId, callerName, url, ringSeconds);
    }

    Intent putInto(Intent intent) {
        return intent.putExtra(KEY_CALL_ID, callId)
                .putExtra(KEY_CHANNEL_ID, channelId)
                .putExtra(KEY_CALLER_ID, callerId)
                .putExtra(KEY_CALLER_NAME, callerName)
                .putExtra(KEY_CALLER_AVATAR, callerAvatar)
                .putExtra(KEY_RING_SECONDS, Integer.toString(ringSeconds));
    }

    /** For the QML (Native.incomingCall). */
    String toJson() {
        try {
            return new JSONObject()
                    .put(KEY_CALL_ID, callId)
                    .put(KEY_CHANNEL_ID, channelId)
                    .put(KEY_CALLER_ID, callerId)
                    .put(KEY_CALLER_NAME, callerName)
                    .put(KEY_CALLER_AVATAR, callerAvatar == null ? "" : callerAvatar)
                    .toString();
        } catch (JSONException e) {
            return "{}";
        }
    }

    /** Ring-stop reasons that mean the callee never picked up. */
    static boolean isMissed(String reason) {
        return "timeout".equals(reason) || "cancelled".equals(reason)
                || "abandoned".equals(reason) || "disconnected".equals(reason);
    }

    private static String nonEmpty(String s) {
        return s == null || s.isEmpty() ? null : s;
    }
}
