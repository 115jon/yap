package dev.yap.qtproto;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Intent;
import android.os.Bundle;

/**
 * Where the manifest's share filters land (another app's Share: pictures,
 * videos, files, text): no window of its own, it hands the share to
 * YapActivity in the app's own task and goes, as LinkActivity does with a
 * link. The intent goes on whole, its items' read grant with it (the
 * system moves EXTRA_STREAM into the ClipData it grants on), so the
 * running activity can still read them once the user picks where they go;
 * ShareShim takes it from there.
 */
public class ShareActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent in = getIntent();
        String action = in == null ? null : in.getAction();
        if (Intent.ACTION_SEND.equals(action) || Intent.ACTION_SEND_MULTIPLE.equals(action)) {
            Intent open = new Intent(in)
                    .setComponent(new ComponentName(this, YapActivity.class))
                    // Not the sender's flags (a new document, its own task).
                    .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                            | Intent.FLAG_ACTIVITY_CLEAR_TOP
                            | Intent.FLAG_ACTIVITY_SINGLE_TOP
                            | Intent.FLAG_GRANT_READ_URI_PERMISSION);
            try {
                startActivity(open);
            } catch (RuntimeException e) {
                // A grant the system would not pass on: nothing to share.
                android.util.Log.w("yap-share", "could not hand the share over", e);
            }
        }
        finish();
    }
}
