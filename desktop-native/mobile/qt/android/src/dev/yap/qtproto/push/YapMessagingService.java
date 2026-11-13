package dev.yap.qtproto.push;

import com.google.firebase.messaging.FirebaseMessagingService;
import com.google.firebase.messaging.RemoteMessage;

import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * FCM's messages and token rotation. Yap's data-only pushes are drawn here,
 * natively, so they show with the app killed, without starting Qt.
 * onMessageReceived runs on a background thread and may block for a few
 * seconds (FCM allows about 20), which covers fetching an avatar.
 */
public final class YapMessagingService extends FirebaseMessagingService {
    @Override
    public void onMessageReceived(RemoteMessage remoteMessage) {
        Map<String, String> data = remoteMessage.getData();
        YapPush.Message message = YapPush.message(data);
        if (message != null) {
            MessageNotifier.show(getApplicationContext(), message);
            return;
        }
        String read = YapPush.readChannel(data);
        if (read != null) {
            MessageNotifier.cancel(getApplicationContext(), read);
            return;
        }
        // A DM call rings, or stops ringing (src/services/call.service.ts).
        String kind = YapPush.isOurs(data) ? data.get("kind") : null;
        IncomingCall call = kind != null && kind.startsWith("call_") ? IncomingCall.from(data) : null;
        if (call == null) return;
        if ("call_ring".equals(kind)) {
            // Up before this returns: FCM may let the process go after.
            CountDownLatch up = new CountDownLatch(1);
            IncomingCalls.ring(getApplicationContext(), call, up::countDown);
            try {
                up.await(8, TimeUnit.SECONDS);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        } else if ("call_ring_stop".equals(kind)) {
            String reason = data.get("reason");
            IncomingCalls.stop(getApplicationContext(), call.callId, reason == null ? "ended" : reason, call);
        }
    }

    @Override
    public void onNewToken(String token) {
        if (PushShim.running()) {
            PushShim.token(token);
            return;
        }
        // Not running: register for the stored session ourselves; the next
        // start registers it through the core again anyway.
        PushStore.Session session = PushStore.session(getApplicationContext());
        if (session != null) PushApi.register(session, token);
    }
}
