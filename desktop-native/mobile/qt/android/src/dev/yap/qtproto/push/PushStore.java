package dev.yap.qtproto.push;

import android.content.Context;
import android.content.SharedPreferences;

import dev.yap.qtproto.TokenVault;

import java.io.File;

/**
 * What the push code needs when the app is not running (a push, a Reply
 * with the app killed): where the core keeps its session file and the
 * API's origin, both handed over by the app at each start (PushShim.install),
 * and whether the notification permission was asked for. The session token
 * itself stays in the core's sealed file (mobile/qt/vault), opened
 * here with the Keystore key only when needed (TokenVault).
 */
final class PushStore {
    private PushStore() {}

    private static final String PREFS = "yap_push";
    private static final String KEY_DATA_DIR = "dataDir";
    private static final String KEY_API_BASE = "apiBase";
    private static final String KEY_ASKED = "askedPermission";

    /** The session as the Java side sees it: the API and the bearer token. */
    static final class Session {
        final String apiBase;
        final String token;

        Session(String apiBase, String token) {
            this.apiBase = apiBase;
            this.token = token;
        }

        /** An API-relative path (`/api/...`) as an absolute URL. */
        String absolute(String url) {
            return url.startsWith("/") ? apiBase + url : url;
        }

        /** Only the API's own host gets the bearer token. */
        boolean owns(String url) {
            return url.startsWith(apiBase + "/");
        }
    }

    private static SharedPreferences prefs(Context context) {
        return context.getApplicationContext().getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    static void remember(Context context, String dataDir, String apiBase) {
        prefs(context).edit()
                .putString(KEY_DATA_DIR, dataDir)
                .putString(KEY_API_BASE, apiBase)
                .apply();
    }

    /**
     * The signed-in session, or null when signed out (no session file) or
     * when the sealed file does not open (the core signs out at its next
     * start). On a background thread: it reads a file and uses the Keystore.
     */
    static Session session(Context context) {
        SharedPreferences prefs = prefs(context);
        String dir = prefs.getString(KEY_DATA_DIR, null);
        String base = prefs.getString(KEY_API_BASE, null);
        if (dir == null || base == null) return null;
        String token = TokenVault.readToken(new File(dir));
        return token == null ? null : new Session(base, token);
    }

    static boolean asked(Context context) {
        return prefs(context).getBoolean(KEY_ASKED, false);
    }

    static void markAsked(Context context) {
        prefs(context).edit().putBoolean(KEY_ASKED, true).apply();
    }
}
