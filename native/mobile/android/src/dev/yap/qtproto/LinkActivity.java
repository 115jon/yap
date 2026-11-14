package dev.yap.qtproto;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;

/**
 * Where the manifest's link filters land (the web's invite and chat links,
 * yap://): no window of its own, it hands the link to YapActivity in the
 * app's own task and goes. Started from another app (a browser, a chat app)
 * a link would otherwise open a second Qt activity in that app's task;
 * this way the running one takes it (onNewIntent), or a cold start makes
 * the app's task with it.
 */
public class LinkActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent in = getIntent();
        if (in != null && in.getData() != null) {
            Intent open = new Intent(Intent.ACTION_VIEW, in.getData(), this, YapActivity.class)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                            | Intent.FLAG_ACTIVITY_CLEAR_TOP
                            | Intent.FLAG_ACTIVITY_SINGLE_TOP);
            startActivity(open);
        }
        finish();
    }
}
