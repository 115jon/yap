package dev.yap.qtproto.push;

import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** The incoming call notification's Decline, also with the app killed. */
public final class CallActionReceiver extends BroadcastReceiver {
    private static final String ACTION_DECLINE = ".call.DECLINE";

    static PendingIntent declineIntent(Context context, IncomingCall call) {
        Intent intent = call.putInto(new Intent(context, CallActionReceiver.class)
                .setAction(context.getPackageName() + ACTION_DECLINE));
        return PendingIntent.getBroadcast(context, call.callId.hashCode() * 4 + 3, intent,
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    @Override
    public void onReceive(Context context, Intent intent) {
        if (!(context.getPackageName() + ACTION_DECLINE).equals(intent.getAction())) return;
        IncomingCall call = IncomingCall.from(intent);
        if (call == null) return;
        // Alive until the server has heard (the request runs off this thread).
        PendingResult pending = goAsync();
        IncomingCalls.decline(context, call.callId, call.channelId, true, pending::finish);
    }
}
