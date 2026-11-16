package dev.yap.qtproto;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.net.Uri;
import android.util.Log;

import androidx.browser.customtabs.CustomTabsIntent;

/**
 * A web page in a Custom Tab over the app (Native.openTab): the browser's
 * own cookies and sign-ins, its address bar, and back returns to Yap.
 * Used where a page must run in a real browser and hand the app a result
 * by its link (a connection's provider sign-in, yap://connection). Without
 * a browser that offers Custom Tabs the intent opens the browser itself.
 */
public final class BrowserTab {
    private static final String TAG = "YapBrowserTab";

    private BrowserTab() {}

    /** Opens an http(s) `url`; anything else is ignored. */
    public static void open(Activity activity, String url) {
        if (activity == null || url == null) return;
        Uri uri = Uri.parse(url);
        String scheme = uri.getScheme();
        if (!"https".equalsIgnoreCase(scheme) && !"http".equalsIgnoreCase(scheme)) return;
        activity.runOnUiThread(() -> {
            CustomTabsIntent tab = new CustomTabsIntent.Builder()
                    .setShowTitle(true)
                    .setShareState(CustomTabsIntent.SHARE_STATE_OFF)
                    .build();
            try {
                tab.launchUrl(activity, uri);
            } catch (ActivityNotFoundException e) {
                Log.w(TAG, "no browser to open the page");
            }
        });
    }
}
