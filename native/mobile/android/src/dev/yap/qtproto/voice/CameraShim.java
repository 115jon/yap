package dev.yap.qtproto.voice;

import android.Manifest;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.PowerManager;
import android.util.Log;

import androidx.core.content.ContextCompat;

import livekit.org.webrtc.Camera1Enumerator;
import livekit.org.webrtc.Camera2Enumerator;
import livekit.org.webrtc.CameraEnumerator;
import livekit.org.webrtc.CameraVideoCapturer;
import livekit.org.webrtc.CapturerObserver;
import livekit.org.webrtc.SurfaceTextureHelper;
import livekit.org.webrtc.VideoFrame;

import java.nio.ByteBuffer;

/**
 * The phone's camera in a call (cpp/camera.cpp, `Native.cameraCapture`):
 * libwebrtc's Camera2Capturer (libwebrtc.jar) at 720p30 onto a texture, each
 * frame read back as I420 (toI420, on the capture's own thread) and handed
 * to the core with the turn that makes it upright (nativeFrame). The core
 * sends it in the call and shows it in the self-view; nothing here keeps a
 * frame past the call that delivers it.
 *
 * Started and stopped from Qt's thread; the capturer is set up and torn
 * down on a thread of its own (stopping waits for the camera to close). It
 * runs only while the app is in front (CameraLink.qml parks it otherwise),
 * and while it runs the call's foreground service carries the camera type
 * (VoiceService.setCamera). What goes wrong goes back as nativeFailed with
 * the desktop's reasons: denied, missing, busy, unknown.
 *
 * The screen going off parks it too (nativeScreen): in a call the activity
 * stays resumed behind the lock screen, so Qt's app state does not change,
 * and Android takes the camera away there.
 */
public final class CameraShim {
    private CameraShim() {}

    private static final String TAG = "yap-camera";

    private static final int WIDTH = 1280;
    private static final int HEIGHT = 720;
    private static final int FPS = 30;

    static native boolean nativeFrame(ByteBuffer y, int strideY, ByteBuffer u, int strideU,
                                      ByteBuffer v, int strideV, int width, int height, int rotation);
    static native void nativeStarted(boolean front);
    static native void nativeFailed(String reason);
    static native void nativeScreen(boolean on);

    /** After the natives are registered: reports the screen going on and off. */
    public static void install(Context context) {
        Context appContext = context.getApplicationContext();
        PowerManager power = (PowerManager) appContext.getSystemService(Context.POWER_SERVICE);
        nativeScreen(power == null || power.isInteractive());
        IntentFilter filter = new IntentFilter(Intent.ACTION_SCREEN_OFF);
        filter.addAction(Intent.ACTION_SCREEN_ON);
        BroadcastReceiver receiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context c, Intent intent) {
                nativeScreen(Intent.ACTION_SCREEN_ON.equals(intent.getAction()));
            }
        };
        // System broadcasts: no export flag needed for them.
        appContext.registerReceiver(receiver, filter);
    }

    private static Handler control;
    private static Context app;

    // The capture's state, on the control thread only.
    private static CameraVideoCapturer capturer;
    private static SurfaceTextureHelper helper;
    /** The running capture's number: callbacks from a stopped one are dropped. */
    private static volatile int generation;
    private static boolean front = true;

    private static synchronized Handler control() {
        if (control == null) {
            HandlerThread thread = new HandlerThread("yap-camera-control");
            thread.start();
            control = new Handler(thread.getLooper());
        }
        return control;
    }

    /** The camera permission is granted. */
    public static boolean allowed(Context context) {
        return ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA)
                == PackageManager.PERMISSION_GRANTED;
    }

    /** Starts capturing, from the front camera or the back one. */
    public static void start(Context context, boolean wantFront) {
        app = context.getApplicationContext();
        control().post(() -> open(wantFront));
    }

    /** Stops capturing (nothing when it is not running). */
    public static void stop() {
        control().post(CameraShim::close);
    }

    /** Front to back, or back to front, while it runs. */
    public static void flip() {
        control().post(() -> {
            if (capturer == null) return;
            final int mine = generation;
            capturer.switchCamera(new CameraVideoCapturer.CameraSwitchHandler() {
                @Override
                public void onCameraSwitchDone(boolean isFront) {
                    control().post(() -> {
                        if (mine != generation) return;
                        front = isFront;
                        nativeStarted(isFront);
                    });
                }

                @Override
                public void onCameraSwitchError(String error) {
                    Log.w(TAG, "switch failed: " + error);
                }
            });
        });
    }

    private static void open(boolean wantFront) {
        if (capturer != null) return;
        if (!allowed(app)) {
            nativeFailed("denied");
            return;
        }
        CameraEnumerator cameras = Camera2Enumerator.isSupported(app)
                ? new Camera2Enumerator(app) : new Camera1Enumerator(false);
        String name = pick(cameras, wantFront);
        if (name == null) {
            nativeFailed("missing");
            return;
        }
        front = cameras.isFrontFacing(name);
        final int mine = ++generation;
        CameraVideoCapturer next;
        try {
            next = cameras.createCapturer(name, events(mine));
        } catch (RuntimeException e) {
            Log.w(TAG, "no capturer: " + e);
            nativeFailed("unknown");
            return;
        }
        if (next == null) {
            nativeFailed("missing");
            return;
        }
        // Its own EGL context; frames are read back to memory, never shared
        // with Qt's.
        helper = SurfaceTextureHelper.create("yap-camera", null);
        if (helper == null) {
            nativeFailed("unknown");
            return;
        }
        capturer = next;
        capturer.initialize(helper, app, observer(mine));
        capturer.startCapture(WIDTH, HEIGHT, FPS);
        Log.i(TAG, "capturing " + name + (front ? " (front)" : " (back)"));
    }

    private static void close() {
        if (capturer == null) return;
        ++generation;
        try {
            capturer.stopCapture();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        capturer.dispose();
        capturer = null;
        if (helper != null) {
            helper.dispose();
            helper = null;
        }
        VoiceService.setCamera(app, false);
        Log.i(TAG, "stopped");
    }

    /** The camera facing the wanted way, else the first there is. */
    private static String pick(CameraEnumerator cameras, boolean wantFront) {
        String[] names;
        try {
            names = cameras.getDeviceNames();
        } catch (RuntimeException e) {
            Log.w(TAG, "cameras not listed: " + e);
            return null;
        }
        for (String name : names) {
            if (wantFront ? cameras.isFrontFacing(name) : cameras.isBackFacing(name)) return name;
        }
        return names.length > 0 ? names[0] : null;
    }

    /** A failure of the running capture: it stops, and the call hears why. */
    private static void failed(int mine, String reason) {
        control().post(() -> {
            if (mine != generation) return;
            close();
            nativeFailed(reason);
        });
    }

    private static CapturerObserver observer(int mine) {
        return new CapturerObserver() {
            @Override
            public void onCapturerStarted(boolean success) {
                if (!success) {
                    failed(mine, "busy");
                    return;
                }
                control().post(() -> {
                    if (mine != generation) return;
                    nativeStarted(front);
                    VoiceService.setCamera(app, true);
                });
            }

            @Override
            public void onCapturerStopped() {}

            // The capture's thread (the texture helper's).
            @Override
            public void onFrameCaptured(VideoFrame frame) {
                if (mine != generation) return;
                VideoFrame.I420Buffer i420 = frame.getBuffer().toI420();
                if (i420 == null) return;
                try {
                    nativeFrame(i420.getDataY(), i420.getStrideY(), i420.getDataU(), i420.getStrideU(),
                            i420.getDataV(), i420.getStrideV(), i420.getWidth(), i420.getHeight(),
                            frame.getRotation());
                } finally {
                    i420.release();
                }
            }
        };
    }

    private static CameraVideoCapturer.CameraEventsHandler events(int mine) {
        return new CameraVideoCapturer.CameraEventsHandler() {
            @Override
            public void onCameraError(String error) {
                Log.w(TAG, "camera error: " + error);
                String text = error == null ? "" : error.toLowerCase(java.util.Locale.ROOT);
                failed(mine, text.contains("in use") || text.contains("too many") || text.contains("disabled")
                        ? "busy" : "unknown");
            }

            @Override
            public void onCameraDisconnected() {
                // Another app (or the system) took the camera.
                failed(mine, "busy");
            }

            @Override
            public void onCameraFreezed(String error) {
                Log.w(TAG, "camera frozen: " + error);
            }

            @Override
            public void onCameraOpening(String name) {}

            @Override
            public void onFirstFrameAvailable() {}

            @Override
            public void onCameraClosed() {}
        };
    }
}
