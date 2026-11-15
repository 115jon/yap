package dev.yap.qtproto.voice;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.hardware.display.DisplayManager;
import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioPlaybackCaptureConfiguration;
import android.media.AudioRecord;
import android.media.projection.MediaProjection;
import android.media.projection.MediaProjectionManager;
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.Process;
import android.util.DisplayMetrics;
import android.util.Log;
import android.view.Display;

import androidx.core.content.ContextCompat;

import livekit.org.webrtc.CapturerObserver;
import livekit.org.webrtc.ScreenCapturerAndroid;
import livekit.org.webrtc.SurfaceTextureHelper;
import livekit.org.webrtc.VideoFrame;

import java.nio.ByteBuffer;
import java.util.concurrent.FutureTask;
import java.util.concurrent.TimeUnit;

/**
 * The phone's screen share in a call (cpp/screen.cpp, rust/src/screen.rs).
 *
 * The user consents in MediaProjection's own dialog (consentIntent, started
 * by Native for its result; consented keeps a yes). The call then starts
 * the capture from one of its threads (start, which blocks it): the call's
 * foreground service takes the mediaProjection type first (Android 14
 * refuses a projection without it), then libwebrtc's ScreenCapturerAndroid
 * (libwebrtc.jar) mirrors the screen onto a virtual display sized to fit
 * the preset's box whichever way up the screen is, on a texture. Frames
 * come at the screen's refresh rate while it changes and not at all while
 * it is still; each one past the preset's pace is read back as I420
 * (toI420, on the capture's thread) and handed to the core, which repeats
 * the last one for a still screen. A consent starts one capture: a share
 * asks again each time.
 *
 * The device's sound (API 29+, asked for): AudioPlaybackCapture of what
 * other apps play as media, games or unknown, never Yap's own uid. The
 * call plays as voice communication, which no ordinary app can capture,
 * so the share never carries the call back to the people in it.
 *
 * Android's own Stop (the cast notification, the status bar's chip, the
 * system ending the projection) reaches MediaProjection.Callback.onStop:
 * the capture is torn down and the core hears its source ended
 * (nativeEnded). The call notification's Stop sharing (VoiceService) goes
 * to the core as a stop (nativeStop).
 */
public final class ScreenShim {
    private ScreenShim() {}

    private static final String TAG = "yap-screen";

    private static final int RATE = 48000;
    /** 10 ms of 16-bit stereo. */
    private static final int CHUNK_BYTES = RATE / 100 * 2 * 2;

    // start's answers below 0 (rust/src/screen.rs).
    private static final long NO_CONSENT = -1;
    private static final long NO_SERVICE = -2;
    private static final long FAILED = -3;

    static native boolean nativeFrame(ByteBuffer y, int strideY, ByteBuffer u, int strideU,
                                      ByteBuffer v, int strideV, int width, int height);
    static native void nativeAudio(ByteBuffer pcm, int length);
    static native void nativeEnded(long capture);
    static native void nativeStop();

    /** The user's yes, until a capture takes it. */
    private static Intent consent;

    private static Handler control;
    private static volatile Context app;

    // The capture's state, on the control thread only.
    private static ScreenCapturerAndroid capturer;
    private static SurfaceTextureHelper helper;
    private static DisplayManager.DisplayListener rotation;
    private static int longSide;
    private static int shortSide;
    /** The captured content's size (the screen, or the app shared alone). */
    private static int contentWidth;
    private static int contentHeight;
    /** The running capture's number: callbacks from a stopped one are dropped. */
    private static volatile long generation;
    /** The minimum gap between frames sent, for the preset's rate. */
    private static volatile long frameGapNs;
    private static long lastFrameNs;
    private static AudioLoop audio;
    /** A capture runs (the call notification says so). */
    private static volatile boolean sharing;

    private static synchronized Handler control() {
        if (control == null) {
            HandlerThread thread = new HandlerThread("yap-screen-control");
            thread.start();
            control = new Handler(thread.getLooper());
        }
        return control;
    }

    /** A capture runs. */
    public static boolean sharing() {
        return sharing;
    }

    /** The system's consent dialog, or null where there is none. */
    public static Intent consentIntent(Context context) {
        MediaProjectionManager manager =
                (MediaProjectionManager) context.getSystemService(Context.MEDIA_PROJECTION_SERVICE);
        return manager == null ? null : manager.createScreenCaptureIntent();
    }

    /** The dialog's answer (Android's main thread): a yes is kept. */
    public static synchronized boolean consented(int resultCode, Intent data) {
        consent = resultCode == Activity.RESULT_OK && data != null ? data : null;
        Log.i(TAG, consent != null ? "consent given" : "consent refused");
        return consent != null;
    }

    private static synchronized Intent takeConsent() {
        Intent taken = consent;
        consent = null;
        return taken;
    }

    /** The device's sound can be captured (API 29+, with the microphone permission). */
    public static boolean audioSupported(Context context) {
        return Build.VERSION.SDK_INT >= 29
                && ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO)
                        == PackageManager.PERMISSION_GRANTED;
    }

    /**
     * Starts a capture with the kept consent (from one of the core's
     * threads; blocks it for up to a few seconds). Returns its number shifted
     * left once, with bit 0 set when the sound is captured too, or below 0:
     * no consent, no mediaProjection service, or the capture failed.
     */
    public static long start(Context context, int wantLong, int wantShort, int fps, boolean withAudio) {
        app = context.getApplicationContext();
        Intent data = takeConsent();
        if (data == null) return NO_CONSENT;
        if (!VoiceService.setProjection(app, true, 4000)) {
            Log.w(TAG, "the call's service did not take the mediaProjection type");
            VoiceService.setProjection(app, false, 0);
            return NO_SERVICE;
        }
        FutureTask<Long> task = new FutureTask<>(() -> open(data, wantLong, wantShort, fps, withAudio));
        control().post(task);
        try {
            return task.get(6, TimeUnit.SECONDS);
        } catch (Exception e) {
            Log.w(TAG, "capture did not start: " + e);
            control().post(ScreenShim::close);
            return FAILED;
        }
    }

    /** Stops capture `capture` (nothing when another one runs). */
    public static void stop(long capture) {
        control().post(() -> {
            if (capture == generation) close();
        });
    }

    /** A new box and pacing for capture `capture`. */
    public static void format(long capture, int wantLong, int wantShort, int fps) {
        control().post(() -> {
            if (capture != generation || capturer == null) return;
            longSide = wantLong;
            shortSide = wantShort;
            frameGapNs = gap(fps);
            resize();
        });
    }

    /** The call notification's Stop sharing (Android's main thread). */
    static void stopFromNotification() {
        if (sharing) nativeStop();
    }

    private static long gap(int fps) {
        // A little under the period: frames arrive on the display's vsync,
        // which does not divide every rate evenly.
        return fps > 0 ? 850_000_000L / fps : 0;
    }

    private static long open(Intent data, int wantLong, int wantShort, int fps, boolean withAudio) {
        close();
        final long mine = ++generation;
        longSide = wantLong;
        shortSide = wantShort;
        frameGapNs = gap(fps);
        lastFrameNs = 0;
        readDisplay();
        // Its own EGL context; frames are read back to memory, never shared
        // with Qt's.
        helper = SurfaceTextureHelper.create("yap-screen", null);
        if (helper == null) {
            VoiceService.setProjection(app, false, 0);
            return FAILED;
        }
        capturer = new ScreenCapturerAndroid(data, callback(mine));
        int[] size = fit();
        try {
            capturer.initialize(helper, app, observer(mine));
            capturer.startCapture(size[0], size[1], fps);
        } catch (RuntimeException e) {
            // A consent the system no longer honours, or no virtual display.
            Log.w(TAG, "projection refused: " + e);
            close();
            return FAILED;
        }
        if (Build.VERSION.SDK_INT < 34) watchRotation(mine);
        boolean sound = withAudio && startAudio(capturer.getMediaProjection());
        sharing = true;
        VoiceService.refresh(app);
        Log.i(TAG, "capturing " + size[0] + "x" + size[1] + " at " + fps + " fps"
                + (sound ? " with the device's sound" : ""));
        return (mine << 1) | (sound ? 1 : 0);
    }

    private static void close() {
        if (capturer == null) return;
        ++generation;
        stopAudio();
        if (rotation != null) {
            DisplayManager displays = (DisplayManager) app.getSystemService(Context.DISPLAY_SERVICE);
            if (displays != null) displays.unregisterDisplayListener(rotation);
            rotation = null;
        }
        try {
            capturer.stopCapture();
        } catch (RuntimeException e) {
            Log.d(TAG, "stop: " + e);
        }
        capturer.dispose();
        capturer = null;
        if (helper != null) {
            helper.dispose();
            helper = null;
        }
        sharing = false;
        VoiceService.setProjection(app, false, 0);
        VoiceService.refresh(app);
        Log.i(TAG, "stopped");
    }

    /** The default display's full size, as the screen is turned now. */
    private static void readDisplay() {
        DisplayManager displays = (DisplayManager) app.getSystemService(Context.DISPLAY_SERVICE);
        Display display = displays == null ? null : displays.getDisplay(Display.DEFAULT_DISPLAY);
        if (display == null) {
            contentWidth = 1080;
            contentHeight = 1920;
            return;
        }
        DisplayMetrics metrics = new DisplayMetrics();
        display.getRealMetrics(metrics);
        contentWidth = metrics.widthPixels;
        contentHeight = metrics.heightPixels;
    }

    /** The content fitted into the box turned its way, never up, even sides. */
    private static int[] fit() {
        int w = Math.max(2, contentWidth);
        int h = Math.max(2, contentHeight);
        int boxW = w >= h ? longSide : shortSide;
        int boxH = w >= h ? shortSide : longSide;
        double scale = Math.min(1.0, Math.min(boxW / (double) w, boxH / (double) h));
        int outW = Math.max(2, (int) Math.round(w * scale) & ~1);
        int outH = Math.max(2, (int) Math.round(h * scale) & ~1);
        return new int[] {outW, outH};
    }

    private static void resize() {
        if (capturer == null) return;
        int[] size = fit();
        try {
            capturer.changeCaptureFormat(size[0], size[1], 0);
            Log.i(TAG, "capturing " + size[0] + "x" + size[1]);
        } catch (RuntimeException e) {
            Log.w(TAG, "resize refused: " + e);
        }
    }

    /** Before API 34 (no content resize callback): the screen turning. */
    private static void watchRotation(long mine) {
        DisplayManager displays = (DisplayManager) app.getSystemService(Context.DISPLAY_SERVICE);
        if (displays == null) return;
        rotation = new DisplayManager.DisplayListener() {
            @Override
            public void onDisplayChanged(int displayId) {
                if (displayId != Display.DEFAULT_DISPLAY || mine != generation) return;
                int w = contentWidth;
                int h = contentHeight;
                readDisplay();
                if (w != contentWidth || h != contentHeight) resize();
            }

            @Override
            public void onDisplayAdded(int displayId) {}

            @Override
            public void onDisplayRemoved(int displayId) {}
        };
        displays.registerDisplayListener(rotation, control());
    }

    private static MediaProjection.Callback callback(long mine) {
        return new MediaProjection.Callback() {
            @Override
            public void onStop() {
                control().post(() -> {
                    if (mine != generation) return;
                    Log.i(TAG, "the projection was stopped outside the app");
                    close();
                    nativeEnded(mine);
                });
            }

            // API 34+: the screen turned, or the app shared alone resized.
            @Override
            public void onCapturedContentResize(int width, int height) {
                control().post(() -> {
                    if (mine != generation || width <= 0 || height <= 0) return;
                    if (width == contentWidth && height == contentHeight) return;
                    contentWidth = width;
                    contentHeight = height;
                    resize();
                });
            }
        };
    }

    private static CapturerObserver observer(long mine) {
        return new CapturerObserver() {
            @Override
            public void onCapturerStarted(boolean success) {}

            @Override
            public void onCapturerStopped() {}

            // The capture's thread (the texture helper's).
            @Override
            public void onFrameCaptured(VideoFrame frame) {
                if (mine != generation) return;
                long at = frame.getTimestampNs();
                if (lastFrameNs != 0 && at - lastFrameNs < frameGapNs) return;
                lastFrameNs = at;
                VideoFrame.I420Buffer i420 = frame.getBuffer().toI420();
                if (i420 == null) return;
                try {
                    nativeFrame(i420.getDataY(), i420.getStrideY(), i420.getDataU(), i420.getStrideU(),
                            i420.getDataV(), i420.getStrideV(), i420.getWidth(), i420.getHeight());
                } finally {
                    i420.release();
                }
            }
        };
    }

    // The device's sound.

    @SuppressWarnings("MissingPermission")
    private static boolean startAudio(MediaProjection projection) {
        if (Build.VERSION.SDK_INT < 29 || projection == null || !audioSupported(app)) return false;
        AudioPlaybackCaptureConfiguration config;
        try {
            config = new AudioPlaybackCaptureConfiguration.Builder(projection)
                    .addMatchingUsage(AudioAttributes.USAGE_MEDIA)
                    .addMatchingUsage(AudioAttributes.USAGE_GAME)
                    .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN)
                    .excludeUid(Process.myUid())
                    .build();
        } catch (RuntimeException e) {
            Log.w(TAG, "no playback capture: " + e);
            return false;
        }
        AudioFormat format = new AudioFormat.Builder()
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .setSampleRate(RATE)
                .setChannelMask(AudioFormat.CHANNEL_IN_STEREO)
                .build();
        int min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_STEREO,
                AudioFormat.ENCODING_PCM_16BIT);
        AudioRecord record;
        try {
            record = new AudioRecord.Builder()
                    .setAudioFormat(format)
                    .setBufferSizeInBytes(Math.max(min, CHUNK_BYTES * 10))
                    .setAudioPlaybackCaptureConfig(config)
                    .build();
        } catch (RuntimeException e) {
            Log.w(TAG, "no playback recorder: " + e);
            return false;
        }
        if (record.getState() != AudioRecord.STATE_INITIALIZED) {
            record.release();
            return false;
        }
        try {
            record.startRecording();
        } catch (RuntimeException e) {
            Log.w(TAG, "playback capture refused: " + e);
            record.release();
            return false;
        }
        if (record.getRecordingState() != AudioRecord.RECORDSTATE_RECORDING) {
            record.release();
            return false;
        }
        audio = new AudioLoop(record);
        audio.start();
        return true;
    }

    private static void stopAudio() {
        if (audio == null) return;
        audio.finish();
        audio = null;
    }

    /** Reads 10 ms at a time on a thread of its own, into the core. */
    private static final class AudioLoop extends Thread {
        private final AudioRecord record;
        private volatile boolean running = true;

        AudioLoop(AudioRecord record) {
            super("yap-screen-audio");
            this.record = record;
        }

        @Override
        public void run() {
            Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO);
            ByteBuffer chunk = ByteBuffer.allocateDirect(CHUNK_BYTES);
            while (running) {
                chunk.clear();
                int read = record.read(chunk, CHUNK_BYTES, AudioRecord.READ_BLOCKING);
                if (read < 0) {
                    Log.w(TAG, "playback capture ended: " + read);
                    break;
                }
                if (read > 0 && running) nativeAudio(chunk, read);
            }
        }

        void finish() {
            running = false;
            try {
                record.stop();
            } catch (RuntimeException e) {
                Log.d(TAG, "audio stop: " + e);
            }
            try {
                join(500);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
            record.release();
        }
    }
}
