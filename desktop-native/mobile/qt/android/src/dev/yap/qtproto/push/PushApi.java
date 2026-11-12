package dev.yap.qtproto.push;

import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.net.Uri;
import android.util.Log;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/**
 * The few API calls notifications make while the app is not in front, on
 * the caller's (background) thread, as the Expo app's RalphApi.kt does.
 * Bearer auth with the core's session; never logs the token or message text.
 * With the app in front, the core makes these calls instead (PushShim).
 */
final class PushApi {
    private PushApi() {}

    private static final String TAG = "yap-push";
    private static final int TIMEOUT_MS = 10_000;
    private static final int AVATAR_TIMEOUT_MS = 5_000;
    private static final int AVATAR_PX = 192;
    private static final int AVATAR_MAX_BYTES = 4 * 1024 * 1024;

    /** POST /api/channels/:id/messages, as a reply. The status, or -1 offline. */
    static int reply(PushStore.Session session, String channelId, String messageId, String content) {
        try {
            JSONObject body = new JSONObject()
                    .put("content", content)
                    // The echo over the gateway finds the send by its nonce.
                    .put("nonce", UUID.randomUUID().toString());
            if (messageId != null && !messageId.isEmpty()) body.put("reply_to_id", messageId);
            return request(session, "POST",
                    "/api/channels/" + Uri.encode(channelId) + "/messages", body.toString());
        } catch (JSONException e) {
            return -1;
        }
    }

    /** PUT /api/channels/:id/read-state. */
    static int markRead(PushStore.Session session, String channelId) {
        return request(session, "PUT", "/api/channels/" + Uri.encode(channelId) + "/read-state", "{}");
    }

    /**
     * The conversation's unread notifications marked read, as the core's
     * Mark as read does while the app runs (toast_mark_read): the read state
     * alone leaves its mentions counted on every device. GET the unread
     * ones (the core's own page, 100), PATCH those of this channel. The
     * PATCH's status, 0 when there was nothing to mark, -1 on a failure.
     */
    static int markNotificationsRead(PushStore.Session session, String channelId) {
        String body = get(session, "/api/notifications?limit=100&unread=true");
        if (body == null) return -1;
        try {
            JSONObject json = new JSONObject(body);
            JSONObject data = json.optJSONObject("data");
            JSONArray list = (data != null ? data : json).optJSONArray("notifications");
            if (list == null) return -1;
            List<String> ids = new ArrayList<>();
            for (int i = 0; i < list.length(); i++) {
                JSONObject n = list.optJSONObject(i);
                if (n != null && channelId.equals(n.optString("channel_id")) && !n.optBoolean("is_read")) {
                    String id = n.optString("id");
                    if (!id.isEmpty()) ids.add(id);
                }
            }
            if (ids.isEmpty()) return 0;
            String patch = new JSONObject().put("ids", new JSONArray(ids)).toString();
            return request(session, "PATCH", "/api/notifications", patch);
        } catch (JSONException e) {
            Log.w(TAG, "notifications: unreadable answer");
            return -1;
        }
    }

    /** POST /api/push-tokens (a rotated token, the app not running). */
    static int register(PushStore.Session session, String token) {
        try {
            String body = new JSONObject()
                    .put("token", token)
                    .put("platform", "android")
                    .put("app_variant", "qt")
                    .put("push_format", "native")
                    .toString();
            return request(session, "POST", "/api/push-tokens", body);
        } catch (JSONException e) {
            return -1;
        }
    }

    private static int request(PushStore.Session session, String method, String path, String body) {
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(session.apiBase + path).openConnection();
            connection.setRequestMethod(method);
            connection.setConnectTimeout(TIMEOUT_MS);
            connection.setReadTimeout(TIMEOUT_MS);
            connection.setDoOutput(true);
            connection.setRequestProperty("Authorization", "Bearer " + session.token);
            connection.setRequestProperty("Content-Type", "application/json");
            try (OutputStream out = connection.getOutputStream()) {
                out.write(body.getBytes(StandardCharsets.UTF_8));
            }
            return connection.getResponseCode();
        } catch (Exception e) {
            Log.w(TAG, method + " failed: " + e.getClass().getSimpleName());
            return -1;
        } finally {
            if (connection != null) connection.disconnect();
        }
    }

    /** A GET's body on a 2xx, else null. */
    private static String get(PushStore.Session session, String path) {
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(session.apiBase + path).openConnection();
            connection.setConnectTimeout(TIMEOUT_MS);
            connection.setReadTimeout(TIMEOUT_MS);
            connection.setRequestProperty("Authorization", "Bearer " + session.token);
            int status = connection.getResponseCode();
            if (status < 200 || status > 299) {
                Log.w(TAG, "GET: HTTP " + status);
                return null;
            }
            try (InputStream in = connection.getInputStream()) {
                ByteArrayOutputStream all = new ByteArrayOutputStream();
                byte[] buffer = new byte[16 * 1024];
                int n;
                while ((n = in.read(buffer)) > 0) all.write(buffer, 0, n);
                return all.toString(StandardCharsets.UTF_8.name());
            }
        } catch (Exception e) {
            Log.w(TAG, "GET failed: " + e.getClass().getSimpleName());
            return null;
        } finally {
            if (connection != null) connection.disconnect();
        }
    }

    /** An avatar, downsampled to about notification size; null on any failure. */
    static Bitmap avatar(PushStore.Session session, String url) {
        String absolute = session != null ? session.absolute(url) : url;
        if (!absolute.startsWith("https://")) return null;
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(absolute).openConnection();
            connection.setConnectTimeout(AVATAR_TIMEOUT_MS);
            connection.setReadTimeout(AVATAR_TIMEOUT_MS);
            if (session != null && session.owns(absolute)) {
                connection.setRequestProperty("Authorization", "Bearer " + session.token);
            }
            int status = connection.getResponseCode();
            if (status < 200 || status > 299) {
                Log.w(TAG, "avatar: HTTP " + status);
                return null;
            }
            byte[] bytes;
            try (InputStream in = connection.getInputStream()) {
                ByteArrayOutputStream all = new ByteArrayOutputStream();
                byte[] buffer = new byte[16 * 1024];
                int n;
                while ((n = in.read(buffer)) > 0) {
                    all.write(buffer, 0, n);
                    if (all.size() > AVATAR_MAX_BYTES) return null;
                }
                bytes = all.toByteArray();
            }
            BitmapFactory.Options bounds = new BitmapFactory.Options();
            bounds.inJustDecodeBounds = true;
            BitmapFactory.decodeByteArray(bytes, 0, bytes.length, bounds);
            int sample = 1;
            while (bounds.outWidth / (sample * 2) >= AVATAR_PX
                    && bounds.outHeight / (sample * 2) >= AVATAR_PX) {
                sample *= 2;
            }
            BitmapFactory.Options options = new BitmapFactory.Options();
            options.inSampleSize = sample;
            return BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
        } catch (Exception e) {
            Log.w(TAG, "avatar: " + e.getClass().getSimpleName());
            return null;
        } finally {
            if (connection != null) connection.disconnect();
        }
    }
}
