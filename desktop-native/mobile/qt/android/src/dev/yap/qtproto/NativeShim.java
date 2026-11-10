package dev.yap.qtproto;

import android.app.Activity;
import android.content.Intent;
import android.hardware.SensorManager;
import android.os.Build;
import android.provider.Settings;
import android.view.HapticFeedbackConstants;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.WindowInsets;
import android.view.WindowInsetsAnimation;
import android.window.BackEvent;
import android.window.OnBackAnimationCallback;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

import java.util.List;

/**
 * The Android side of `Native` (cpp/native.h): what Qt Quick does not do by
 * itself, as thin calls the C++ singleton makes over JNI. Everything here is
 * stateless platform API; the app's logic stays in the shared core and QML.
 */
public final class NativeShim {
    private NativeShim() {}

    static native void nativeBackProgress(float progress);
    static native void nativeBackCancelled();
    static native void nativeBackInvoked();
    static native void nativeIme(float dp);

    private static Activity activity;
    private static OnBackInvokedCallback back;
    private static boolean backRegistered;
    private static float density = 1f;

    public static void install(Activity a) {
        activity = a;
        density = a.getResources().getDisplayMetrics().density;
        a.runOnUiThread(NativeShim::installOnUi);
    }

    private static void installOnUi() {
        View decor = activity.getWindow().getDecorView();
        // The keyboard: the IME inset on every frame of its animation, and
        // its resting value after any change. The listener hands the insets
        // on unchanged, so Qt's own safe-area handling still sees them.
        decor.setOnApplyWindowInsetsListener((v, insets) -> {
            reportIme(insets);
            return v.onApplyWindowInsets(insets);
        });
        decor.setWindowInsetsAnimationCallback(
                new WindowInsetsAnimation.Callback(
                        WindowInsetsAnimation.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
                    @Override
                    public WindowInsets onProgress(WindowInsets insets,
                            List<WindowInsetsAnimation> running) {
                        reportIme(insets);
                        return insets;
                    }
                });

        // The back gesture. API 34+: the predictive preview's progress.
        if (Build.VERSION.SDK_INT >= 34) {
            back = new OnBackAnimationCallback() {
                @Override
                public void onBackStarted(BackEvent e) { nativeBackProgress(0f); }

                @Override
                public void onBackProgressed(BackEvent e) {
                    nativeBackProgress(e.getProgress());
                }

                @Override
                public void onBackCancelled() { nativeBackCancelled(); }

                @Override
                public void onBackInvoked() { nativeBackInvoked(); }
            };
        } else if (Build.VERSION.SDK_INT >= 33) {
            back = NativeShim::nativeBackInvoked;
        }
    }

    private static void reportIme(WindowInsets insets) {
        nativeIme(insets.getInsets(WindowInsets.Type.ime()).bottom / density);
    }

    /** Takes the back gesture while the app has a page to go back to. */
    public static void setHandlesBack(boolean on) {
        if (activity == null || back == null) return;
        activity.runOnUiThread(() -> {
            OnBackInvokedDispatcher d = activity.getOnBackInvokedDispatcher();
            if (on && !backRegistered) {
                d.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, back);
                backRegistered = true;
            } else if (!on && backRegistered) {
                d.unregisterOnBackInvokedCallback(back);
                backRegistered = false;
            }
        });
    }

    public static void haptic(int kind) {
        if (activity == null) return;
        final int constant;
        switch (kind) {
            case 1: constant = HapticFeedbackConstants.LONG_PRESS; break;
            case 2: constant = Build.VERSION.SDK_INT >= 30
                    ? HapticFeedbackConstants.CONFIRM : HapticFeedbackConstants.VIRTUAL_KEY; break;
            case 3: constant = Build.VERSION.SDK_INT >= 30
                    ? HapticFeedbackConstants.REJECT : HapticFeedbackConstants.LONG_PRESS; break;
            default: constant = HapticFeedbackConstants.CLOCK_TICK; break;
        }
        activity.runOnUiThread(
                () -> activity.getWindow().getDecorView().performHapticFeedback(constant));
    }

    public static void share(String text) {
        if (activity == null) return;
        Intent send = new Intent(Intent.ACTION_SEND)
                .setType("text/plain")
                .putExtra(Intent.EXTRA_TEXT, text);
        activity.runOnUiThread(() -> activity.startActivity(Intent.createChooser(send, null)));
    }

    public static void leave() {
        if (activity != null) activity.runOnUiThread(() -> activity.moveTaskToBack(true));
    }

    public static float fontScale() {
        return activity == null ? 1f : activity.getResources().getConfiguration().fontScale;
    }

    public static boolean reducedMotion() {
        if (activity == null) return false;
        return Settings.Global.getFloat(activity.getContentResolver(),
                Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f;
    }

    /** Material You's primary accent (system_accent1_500), or 0 before 12. */
    public static int accent() {
        if (activity == null || Build.VERSION.SDK_INT < 31) return 0;
        return activity.getColor(android.R.color.system_accent1_500);
    }

    /** The platform's top fling speed, in dp/s (Qt's logical px). */
    public static float maxFlingVelocity() {
        if (activity == null) return 8000f;
        return ViewConfiguration.get(activity).getScaledMaximumFlingVelocity() / density;
    }

    /**
     * A constant deceleration (dp/s^2) that makes Qt's fling travel as far as
     * Android's spline fling (OverScroller) does from a typical 2500 dp/s.
     */
    public static float flingDeceleration() {
        if (activity == null) return 1500f;
        final double inflexion = 0.35;
        final double rate = Math.log(0.78) / Math.log(0.9);
        double ppi = activity.getResources().getDisplayMetrics().density * 160.0;
        double physical = SensorManager.GRAVITY_EARTH * 39.37 * ppi * 0.84;
        double friction = ViewConfiguration.getScrollFriction() * physical;
        double v = 2500.0 * density;
        double l = Math.log(inflexion * v / friction);
        double distance = friction * Math.exp(rate / (rate - 1.0) * l);
        double decel = v * v / (2.0 * distance);
        return (float) (decel / density);
    }
}
