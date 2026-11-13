package dev.yap.qtproto;

import android.content.Intent;
import android.os.Bundle;

import org.qtproject.qt.android.bindings.QtActivity;

import dev.yap.qtproto.push.PushShim;
import dev.yap.qtproto.voice.VoiceShim;

/**
 * Qt's activity, with a notification's tap kept for the push code and a
 * link for the link code: either can arrive (onCreate, or onNewIntent on a
 * task relaunched from Recents) before Qt's main() has installed its shim,
 * so PushShim and LinkShim hold it until install() reads it. And with a
 * call's goodbye on the way out.
 */
public class YapActivity extends QtActivity {
    @Override
    public void onCreate(Bundle savedInstanceState) {
        // A recreated activity (restored state, or started again from
        // Recents) carries the intent it was first started with: an old tap.
        boolean fresh = savedInstanceState == null
                && (getIntent().getFlags() & Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0;
        if (fresh) {
            PushShim.offer(getIntent());
            LinkShim.offer(getIntent());
        }
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        PushShim.offer(intent);
        LinkShim.offer(intent);
    }

    @Override
    protected void onDestroy() {
        // Qt's onDestroy ends the process: a call still up tells its room
        // it left first (up to 800 ms; nothing outside a call).
        VoiceShim.quit();
        super.onDestroy();
    }
}
