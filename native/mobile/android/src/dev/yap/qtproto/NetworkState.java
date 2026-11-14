package dev.yap.qtproto;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.NetworkInfo;

/**
 * Whether the device has a network, for the core's gateway (cpp/native.cpp,
 * from any thread). Android blocks a background app's network while the
 * device's stays up: a lookup that failed there is not "no network", and
 * the connection strip says Reconnecting rather than Offline.
 */
public final class NetworkState {
    private NetworkState() {}

    /**
     * 1 up, 0 none, -1 unknown. Up also when it is blocked for this app (in
     * the background, or Data Saver): getActiveNetwork() is null then, while
     * the deprecated NetworkInfo still names the network, as BLOCKED.
     */
    @SuppressWarnings("deprecation")
    public static int get(Context context) {
        if (context == null) return -1;
        try {
            ConnectivityManager cm = context.getSystemService(ConnectivityManager.class);
            if (cm == null) return -1;
            if (cm.getActiveNetwork() != null) return 1;
            NetworkInfo info = cm.getActiveNetworkInfo();
            return info != null && info.getDetailedState() == NetworkInfo.DetailedState.BLOCKED ? 1 : 0;
        } catch (RuntimeException e) {
            return -1;
        }
    }
}
