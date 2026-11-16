package dev.yap.qtproto.voice;

import android.media.AudioFormat;
import android.media.AudioRecord;
import android.media.MediaRecorder;
import android.util.Log;

/**
 * Settings > Voice & video's mic test, outside a call: the microphone
 * opened on its own thread as a call opens it (the voice communication
 * source, so Android's own echo and noise processing run as in a call), and
 * the RMS of every 20 ms handed to Native (cpp/voice.cpp), which turns it
 * into the meter's level. Nothing is recorded or played back. A mic that
 * cannot open reports -1 once and the test ends.
 *
 * Needs the microphone permission; the QML asks for it first.
 */
public final class MicTest {
    private static final String TAG = "yap-voice";
    private static final int RATE = 48000;
    /** 20 ms of mono samples. */
    private static final int FRAME = RATE / 50;

    static native void nativeLevel(double rms);

    private static Thread thread;
    private static volatile boolean running;

    private MicTest() {}

    public static synchronized void start() {
        if (thread != null) return;
        running = true;
        thread = new Thread(MicTest::run, "yap-mic-test");
        thread.start();
    }

    public static synchronized void stop() {
        running = false;
        Thread t = thread;
        thread = null;
        if (t == null) return;
        try {
            t.join(500);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    private static void run() {
        AudioRecord record = open();
        if (record == null) {
            nativeLevel(-1);
            return;
        }
        short[] samples = new short[FRAME];
        try {
            record.startRecording();
            if (record.getRecordingState() != AudioRecord.RECORDSTATE_RECORDING) {
                Log.w(TAG, "mic test: the mic did not start");
                nativeLevel(-1);
                return;
            }
            Log.i(TAG, "mic test: on");
            // The loudest of the first second, logged once: a meter that
            // stays dark reads as a mic that hears nothing here.
            int frames = 0;
            double peak = 0;
            while (running) {
                int n = record.read(samples, 0, FRAME);
                if (n < 0) {
                    Log.w(TAG, "mic test: read failed (" + n + ")");
                    nativeLevel(-1);
                    return;
                }
                if (n == 0) continue;
                double sum = 0;
                for (int i = 0; i < n; i++) {
                    double x = samples[i] / 32768.0;
                    sum += x * x;
                }
                double rms = Math.sqrt(sum / n);
                nativeLevel(rms);
                peak = Math.max(peak, rms);
                if (++frames == 50) Log.i(TAG, String.format(java.util.Locale.ROOT, "mic test: first second peak rms %.5f", peak));
            }
        } catch (IllegalStateException e) {
            Log.w(TAG, "mic test: " + e.getMessage());
            nativeLevel(-1);
        } finally {
            try {
                record.stop();
            } catch (IllegalStateException ignored) {
                // Never started.
            }
            record.release();
            Log.i(TAG, "mic test: off");
        }
    }

    private static AudioRecord open() {
        int min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT);
        if (min <= 0) return null;
        try {
            AudioRecord record = new AudioRecord(MediaRecorder.AudioSource.VOICE_COMMUNICATION, RATE,
                    AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, Math.max(min, FRAME * 8));
            if (record.getState() == AudioRecord.STATE_INITIALIZED) return record;
            record.release();
            Log.w(TAG, "mic test: the mic did not open");
        } catch (SecurityException | IllegalArgumentException e) {
            Log.w(TAG, "mic test: " + e.getMessage());
        }
        return null;
    }
}
