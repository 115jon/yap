package dev.yap.qtproto.voice;

import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioDeviceCallback;
import android.media.AudioDeviceInfo;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.os.Build;
import android.os.Handler;
import android.os.PowerManager;
import android.util.Log;

import java.util.ArrayList;
import java.util.List;

/**
 * The phone's audio while a call runs: the communication mode, audio focus
 * (a phone call interrupts), and where the call plays and records: the
 * speaker, the earpiece, a wired headset or a Bluetooth one. libwebrtc's own
 * device module opens the mic and the output; this only tells Android which.
 *
 * Android 12+ routes with AudioManager.setCommunicationDevice; before that
 * with the speakerphone switch and Bluetooth SCO. All of it runs on the main
 * thread (VoiceShim posts there).
 *
 * Interrupted means something else has the phone's ear for a while: a
 * transient focus loss (a ringing or running phone call, an alarm, the
 * assistant), or, on Android 12+, the telephony audio mode, which also
 * tells when focus cannot (another app took focus for good earlier, music
 * under the call, and the system then no longer reports to us). A focus
 * loss for good is not an interruption: people talk over music.
 */
final class CallAudio {
    private static final String TAG = "yap-voice";

    static final String SPEAKER = "speaker";
    static final String EARPIECE = "earpiece";
    static final String WIRED = "wired";
    static final String BLUETOOTH = "bluetooth";

    /** What changed, for the shim: the routes, or the focus. */
    interface Listener {
        void onRoutes(String json);
        /** A transient focus loss began (`true`) or ended. */
        void onInterrupted(boolean interrupted);
    }

    private final AudioManager audio;
    private final PowerManager power;
    private final Handler main;
    private final Listener listener;

    private boolean running;
    /** The route the user picked, while it is still there (else automatic). */
    private String chosen;
    /** The route in use. */
    private String active = SPEAKER;
    private AudioFocusRequest focus;
    /** What the listener was last told. */
    private boolean interrupted;
    /** Focus is away for a while (it comes back, or we ask again). */
    private boolean focusAway;
    /** Android 12+: a phone call owns the audio mode. */
    private boolean phoneBusy;
    private AudioManager.OnModeChangedListener modeChanged;
    /**
     * Asks for focus again while it is away and nobody will hand it back:
     * a request refused at the start, or a transient loss turned final.
     */
    private final Runnable refocus = this::requestFocusAgain;
    private static final long REFOCUS_EVERY_MS = 2000;
    private boolean scoOn;
    private PowerManager.WakeLock proximity;
    private String reported = "";
    private AudioManager.OnCommunicationDeviceChangedListener deviceChanged;

    private final AudioDeviceCallback devices = new AudioDeviceCallback() {
        @Override
        public void onAudioDevicesAdded(AudioDeviceInfo[] added) {
            // A headset that just arrived takes the call, as the system does
            // for a phone call.
            for (AudioDeviceInfo device : added) {
                String kind = kind(device);
                if (device.isSink() && (WIRED.equals(kind) || BLUETOOTH.equals(kind))) chosen = null;
            }
            apply();
        }

        @Override
        public void onAudioDevicesRemoved(AudioDeviceInfo[] removed) {
            apply();
        }
    };

    CallAudio(Context context, Handler main, Listener listener) {
        this.audio = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
        this.power = (PowerManager) context.getSystemService(Context.POWER_SERVICE);
        this.main = main;
        this.listener = listener;
    }

    void begin() {
        if (running) return;
        running = true;
        chosen = null;
        interrupted = false;
        focusAway = false;
        phoneBusy = false;
        reported = "";
        audio.setMode(AudioManager.MODE_IN_COMMUNICATION);
        focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
                .setAudioAttributes(new AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build())
                .setOnAudioFocusChangeListener(this::onFocus, main)
                .build();
        int granted = audio.requestAudioFocus(focus);
        if (granted != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            // Refused: a phone call is up already. The call starts
            // interrupted, and asks again until the phone is free.
            Log.w(TAG, "audio focus not granted (" + granted + ")");
            focusAway = true;
            main.postDelayed(refocus, REFOCUS_EVERY_MS);
        }
        audio.registerAudioDeviceCallback(devices, main);
        if (Build.VERSION.SDK_INT >= 31) {
            deviceChanged = device -> report();
            audio.addOnCommunicationDeviceChangedListener(main::post, deviceChanged);
            modeChanged = mode -> {
                phoneBusy = telephony(mode);
                evaluate();
            };
            audio.addOnModeChangedListener(main::post, modeChanged);
            phoneBusy = telephony(audio.getMode());
        }
        apply();
        evaluate();
    }

    private static boolean telephony(int mode) {
        return mode == AudioManager.MODE_RINGTONE || mode == AudioManager.MODE_IN_CALL
                || mode == AudioManager.MODE_CALL_SCREENING;
    }

    void end() {
        if (!running) return;
        running = false;
        audio.unregisterAudioDeviceCallback(devices);
        main.removeCallbacks(refocus);
        if (Build.VERSION.SDK_INT >= 31) {
            if (deviceChanged != null) audio.removeOnCommunicationDeviceChangedListener(deviceChanged);
            deviceChanged = null;
            if (modeChanged != null) audio.removeOnModeChangedListener(modeChanged);
            modeChanged = null;
            audio.clearCommunicationDevice();
        } else {
            stopSco();
            setSpeakerphone(false);
        }
        if (focus != null) audio.abandonAudioFocusRequest(focus);
        focus = null;
        interrupted = false;
        focusAway = false;
        phoneBusy = false;
        audio.setMode(AudioManager.MODE_NORMAL);
        holdProximity(false);
        reported = "";
    }

    /** The user's pick (one of the ids `routes` lists). */
    void choose(String id) {
        if (!running) return;
        chosen = id;
        apply();
    }

    /** `[{"id","name","active"}]`: what the call can play through now. */
    String routes() {
        if (!running) return "[]";
        StringBuilder json = new StringBuilder("[");
        for (String[] route : available()) {
            if (json.length() > 1) json.append(',');
            json.append("{\"id\":\"").append(route[0])
                    .append("\",\"name\":\"").append(escape(route[1]))
                    .append("\",\"active\":").append(route[0].equals(inUse())).append('}');
        }
        return json.append(']').toString();
    }

    /**
     * The route in use: on Android 12+ the one the system says it routes
     * to (a refused or failed switch shows as what it fell back to), else
     * the one last asked for.
     */
    private String inUse() {
        if (Build.VERSION.SDK_INT >= 31) {
            AudioDeviceInfo device = audio.getCommunicationDevice();
            String kind = device == null ? null : kind(device);
            if (kind != null) return kind;
        }
        return active;
    }

    private void onFocus(int change) {
        if (!running) return;
        if (change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
            // A phone call, an alarm, the assistant: nothing of ours plays or
            // records until it is over.
            main.removeCallbacks(refocus);
            focusAway = true;
        } else if (change == AudioManager.AUDIOFOCUS_GAIN) {
            main.removeCallbacks(refocus);
            focusAway = false;
        } else if (change == AudioManager.AUDIOFOCUS_LOSS) {
            // For good: the system will not hand it back. If that ended a
            // transient loss (the ringing became the call), ask again until
            // the phone is free; a refusal means it is not. Otherwise
            // another app started playing, and the call carries on over it.
            if (focusAway) {
                main.removeCallbacks(refocus);
                main.postDelayed(refocus, REFOCUS_EVERY_MS);
            }
        }
        // Ducking leaves the call as it is.
        evaluate();
    }

    private void requestFocusAgain() {
        if (!running || !focusAway || focus == null) return;
        if (audio.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            focusAway = false;
            evaluate();
        } else {
            main.postDelayed(refocus, REFOCUS_EVERY_MS);
        }
    }

    /** Tells the listener when the interruption starts or ends. */
    private void evaluate() {
        if (!running) return;
        boolean now = focusAway || phoneBusy;
        if (now == interrupted) return;
        interrupted = now;
        if (!now) {
            // The other side may have changed the mode and the route.
            audio.setMode(AudioManager.MODE_IN_COMMUNICATION);
            apply();
        }
        listener.onInterrupted(now);
    }

    /** Each route as `{id, name}`, in menu order. */
    private List<String[]> available() {
        boolean earpiece = false, wired = false;
        String bluetooth = null;
        for (AudioDeviceInfo device : outputs()) {
            String kind = kind(device);
            if (EARPIECE.equals(kind)) earpiece = true;
            else if (WIRED.equals(kind)) wired = true;
            else if (BLUETOOTH.equals(kind) && bluetooth == null) {
                CharSequence name = device.getProductName();
                bluetooth = name != null && name.length() > 0 ? name.toString() : "Bluetooth";
            }
        }
        List<String[]> routes = new ArrayList<>();
        if (bluetooth != null) routes.add(new String[] {BLUETOOTH, bluetooth});
        if (wired) routes.add(new String[] {WIRED, "Headset"});
        routes.add(new String[] {SPEAKER, "Speaker"});
        // A plugged headset takes the earpiece's place, as in a phone call.
        if (earpiece && !wired) routes.add(new String[] {EARPIECE, "Phone"});
        return routes;
    }

    private List<AudioDeviceInfo> outputs() {
        if (Build.VERSION.SDK_INT >= 31) return audio.getAvailableCommunicationDevices();
        List<AudioDeviceInfo> list = new ArrayList<>();
        for (AudioDeviceInfo device : audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS)) list.add(device);
        return list;
    }

    private static String kind(AudioDeviceInfo device) {
        switch (device.getType()) {
            case AudioDeviceInfo.TYPE_BUILTIN_SPEAKER:
                return SPEAKER;
            case AudioDeviceInfo.TYPE_BUILTIN_EARPIECE:
                return EARPIECE;
            case AudioDeviceInfo.TYPE_WIRED_HEADSET:
            case AudioDeviceInfo.TYPE_WIRED_HEADPHONES:
            case AudioDeviceInfo.TYPE_USB_HEADSET:
                return WIRED;
            case AudioDeviceInfo.TYPE_BLUETOOTH_SCO:
            case AudioDeviceInfo.TYPE_HEARING_AID:
            case AudioDeviceInfo.TYPE_BLE_HEADSET:
                return BLUETOOTH;
            default:
                return null;
        }
    }

    /** Routes the call: the pick while it is there, else headset first. */
    private void apply() {
        if (!running) return;
        List<String[]> routes = available();
        String want = null;
        for (String[] route : routes) {
            if (route[0].equals(chosen)) want = chosen;
        }
        if (want == null) {
            chosen = null;
            // Menu order is the automatic order: Bluetooth, wired, speaker.
            want = routes.get(0)[0];
        }
        if (Build.VERSION.SDK_INT >= 31) {
            AudioDeviceInfo target = null;
            for (AudioDeviceInfo device : audio.getAvailableCommunicationDevices()) {
                if (want.equals(kind(device))) {
                    target = device;
                    break;
                }
            }
            AudioDeviceInfo current = audio.getCommunicationDevice();
            if (target != null && (current == null || current.getId() != target.getId())) {
                try {
                    if (!audio.setCommunicationDevice(target)) Log.w(TAG, "route refused: " + want);
                } catch (IllegalArgumentException e) {
                    // The device went away between the list and the call.
                    Log.w(TAG, "route gone: " + want);
                }
            }
        } else {
            applyLegacy(want);
        }
        active = want;
        holdProximity(EARPIECE.equals(inUse()));
        report();
    }

    @SuppressWarnings("deprecation")
    private void applyLegacy(String want) {
        if (BLUETOOTH.equals(want)) {
            setSpeakerphone(false);
            if (!scoOn) {
                scoOn = true;
                audio.startBluetoothSco();
                audio.setBluetoothScoOn(true);
            }
            return;
        }
        stopSco();
        setSpeakerphone(SPEAKER.equals(want));
    }

    @SuppressWarnings("deprecation")
    private void stopSco() {
        if (!scoOn) return;
        scoOn = false;
        audio.setBluetoothScoOn(false);
        audio.stopBluetoothSco();
    }

    @SuppressWarnings("deprecation")
    private void setSpeakerphone(boolean on) {
        if (audio.isSpeakerphoneOn() != on) audio.setSpeakerphoneOn(on);
    }

    /** The screen goes dark against the ear while the earpiece plays. */
    private void holdProximity(boolean hold) {
        if (hold) {
            if (proximity == null && power != null
                    && power.isWakeLockLevelSupported(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK)) {
                proximity = power.newWakeLock(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK, "yap:call");
                proximity.setReferenceCounted(false);
            }
            if (proximity != null && !proximity.isHeld()) proximity.acquire();
        } else if (proximity != null && proximity.isHeld()) {
            proximity.release();
        }
    }

    private void report() {
        if (!running) return;
        holdProximity(EARPIECE.equals(inUse()));
        String json = routes();
        if (json.equals(reported)) return;
        reported = json;
        listener.onRoutes(json);
    }

    private static String escape(String text) {
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (c == '"' || c == '\\') out.append('\\').append(c);
            else if (c < 0x20) out.append(' ');
            else out.append(c);
        }
        return out.toString();
    }
}
