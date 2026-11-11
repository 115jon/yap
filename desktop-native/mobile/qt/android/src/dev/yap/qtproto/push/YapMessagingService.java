package dev.yap.qtproto.push;

import com.google.firebase.messaging.FirebaseMessagingService;
import com.google.firebase.messaging.RemoteMessage;

import java.util.Map;

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
        if (read != null) MessageNotifier.cancel(getApplicationContext(), read);
        // Call rings (kind call_*) need voice, which this app does not have yet.
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
