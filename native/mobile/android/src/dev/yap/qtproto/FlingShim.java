package dev.yap.qtproto;

import android.app.Activity;
import android.util.Log;
import android.view.MotionEvent;
import android.view.VelocityTracker;
import android.view.ViewConfiguration;

/**
 * The finger's speed as Android's own lists measure it. Qt sees one touch
 * point per frame, timed when it reaches Qt's thread, and averages its
 * last smoothed speeds, so a flick that speeds up to the lift reads slow.
 * YapActivity hands every touch here first (all its samples, the batched
 * ones in its history too, with their own times), and the C++ side
 * (cpp/fling.cpp) takes the speed at the lift to start the list's fling.
 * Speeds are in dp/s (Qt's logical px), down and right positive.
 */
public final class FlingShim {
    private static final String TAG = "yap-fling";

    private static VelocityTracker tracker;
    private static int pointer = -1;
    private static float density = 1f;
    private static float maxFling = 8000f;
    private static float minFling = 50f;
    private static boolean log;

    // The last lift, until the C++ side takes it.
    private static boolean lifted;
    private static float liftX;
    private static float liftY;

    // How the touch came in (logged): events, samples, from down to up.
    private static int events;
    private static int samples;
    private static long downTime;

    private FlingShim() {}

    /** From the C++ side once Qt is up: the device's fling limits. */
    public static synchronized void install(Activity activity, boolean logging) {
        density = activity.getResources().getDisplayMetrics().density;
        ViewConfiguration config = ViewConfiguration.get(activity);
        maxFling = config.getScaledMaximumFlingVelocity();
        minFling = config.getScaledMinimumFlingVelocity();
        log = logging;
    }

    /** The fastest fling, in dp/s. */
    public static synchronized float maxVelocity() {
        return maxFling / density;
    }

    /** The slowest lift that still flings, in dp/s. */
    public static synchronized float minVelocity() {
        return minFling / density;
    }

    /** Every touch, before Qt has it (YapActivity.dispatchTouchEvent). */
    static synchronized void track(MotionEvent event) {
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                if (tracker == null) tracker = VelocityTracker.obtain();
                tracker.clear();
                pointer = event.getPointerId(0);
                lifted = false;
                events = 1;
                samples = 1;
                downTime = event.getEventTime();
                tracker.addMovement(event);
                break;
            case MotionEvent.ACTION_MOVE:
                if (tracker == null) return;
                events++;
                samples += 1 + event.getHistorySize();
                tracker.addMovement(event);
                break;
            case MotionEvent.ACTION_POINTER_UP:
                if (tracker == null) return;
                // The finger the list followed lifted first: no fling to give.
                if (event.getPointerId(event.getActionIndex()) == pointer) pointer = -1;
                tracker.addMovement(event);
                break;
            case MotionEvent.ACTION_UP:
                if (tracker == null) return;
                tracker.addMovement(event);
                if (pointer >= 0) {
                    tracker.computeCurrentVelocity(1000, maxFling);
                    liftX = tracker.getXVelocity(pointer) / density;
                    liftY = tracker.getYVelocity(pointer) / density;
                    lifted = true;
                    if (log) {
                        long ms = event.getEventTime() - downTime;
                        Log.i(TAG, String.format("touch: %d events, %d samples in %d ms (%.0f Hz input, %.0f Hz samples), lift %.0f,%.0f dp/s",
                                events, samples, ms, ms > 0 ? events * 1000f / ms : 0f, ms > 0 ? samples * 1000f / ms : 0f,
                                liftX, liftY));
                    }
                }
                pointer = -1;
                break;
            case MotionEvent.ACTION_CANCEL:
                lifted = false;
                pointer = -1;
                break;
            default:
                if (tracker != null) tracker.addMovement(event);
                break;
        }
    }

    /**
     * The last lift's speed, once: {1, x, y}, or {0, 0, 0} when there is
     * none to give (a cancel, a second finger, already taken).
     */
    public static synchronized float[] take() {
        float[] out = { lifted ? 1f : 0f, liftX, liftY };
        lifted = false;
        return out;
    }
}
