package dev.yap.qtproto.push;

import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.net.Uri;
import android.util.Log;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.UUID;

/**
 * The few API calls notifications make while the app is not running, on
 * the caller's (background) thread, as the Expo app's RalphApi.kt does.
 * Bearer auth with the core's session; never logs the token or message text.
 * While the app runs, the core makes these calls instead (PushShim).
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
