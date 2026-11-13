package dev.yap.qtproto.push;

import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioManager;
import android.media.Ringtone;
import android.media.RingtoneManager;
import android.net.Uri;
import android.os.Build;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.provider.Settings;
import android.util.Log;

/**
 * The ring while the app is in front (its incoming sheet shows; no
 * notification rings): the phone's ringtone on the ring stream, looping,
 * and the ring's vibration, as the ringer mode allows. Main thread.
 */
final class Ringer {
    private Ringer() {}

    private static final String TAG = "yap-call";
    private static final long[] PATTERN = {0, 800, 800};

    private static Ringtone ringtone;
    private static Vibrator vibrator;

    static void start(Context context) {
        stop();
        AudioManager audio = context.getSystemService(AudioManager.class);
        int mode = audio != null ? audio.getRingerMode() : AudioManager.RINGER_MODE_NORMAL;
        if (mode == AudioManager.RINGER_MODE_SILENT) return;
        if (mode == AudioManager.RINGER_MODE_NORMAL) {
            Uri uri = RingtoneManager.getActualDefaultRingtoneUri(context, RingtoneManager.TYPE_RINGTONE);
            if (uri == null) uri = Settings.System.DEFAULT_RINGTONE_URI;
            try {
                ringtone = RingtoneManager.getRingtone(context, uri);
                if (ringtone != null) {
                    ringtone.setAudioAttributes(new AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                            .build());
                    if (Build.VERSION.SDK_INT >= 28) ringtone.setLooping(true);
                    ringtone.play();
                }
            } catch (RuntimeException e) {
                Log.w(TAG, "no ringtone: " + e.getClass().getSimpleName());
                ringtone = null;
            }
        }
        vibrator = context.getSystemService(Vibrator.class);
        if (vibrator != null && vibrator.hasVibrator()) {
            vibrator.vibrate(VibrationEffect.createWaveform(PATTERN, 0), new AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                    .build());
        } else {
            vibrator = null;
        }
    }

    static void stop() {
        if (ringtone != null) {
            try {
                ringtone.stop();
            } catch (RuntimeException e) {
                // Already gone.
            }
            ringtone = null;
        }
        if (vibrator != null) {
            vibrator.cancel();
            vibrator = null;
        }
    }
}
