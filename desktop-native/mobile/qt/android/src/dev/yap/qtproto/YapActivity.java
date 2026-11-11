package dev.yap.qtproto;

import android.content.Intent;
import android.os.Bundle;

import org.qtproject.qt.android.bindings.QtActivity;

import dev.yap.qtproto.push.PushShim;

/**
 * Qt's activity, with a notification's tap kept for the push code: a tap
 * can arrive (onCreate, or onNewIntent on a task relaunched from Recents)
 * before Qt's main() has installed the push shim, so PushShim holds it
 * until install() reads it.
 */
public class YapActivity extends QtActivity {
    @Override
    public void onCreate(Bundle savedInstanceState) {
        // A recreated activity (restored state, or started again from
        // Recents) carries the intent it was first started with: an old tap.
        boolean fresh = savedInstanceState == null
                && (getIntent().getFlags() & Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0;
        if (fresh) PushShim.offer(getIntent());
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        PushShim.offer(intent);
    }
}
