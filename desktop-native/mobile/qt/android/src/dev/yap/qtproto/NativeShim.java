package dev.yap.qtproto;

import android.app.Activity;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.hardware.SensorManager;
import android.net.Uri;
import android.os.Build;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.util.Log;
import android.webkit.MimeTypeMap;
import android.view.HapticFeedbackConstants;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.WindowInsets;
import android.view.WindowInsetsAnimation;
import android.window.BackEvent;
import android.window.OnBackAnimationCallback;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

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
    static native void nativeMediaCopying();
    static native void nativeMediaPicked(String[] paths);

    /** The most files one pick hands to the tray. */
    private static final int PICK_MAX = 10;
    /** Picked files are copied under the cache, here, until the next start. */
    private static final String UPLOADS = "uploads";

    private static Activity activity;
    private static OnBackInvokedCallback back;
    private static boolean backRegistered;
    private static float density = 1f;

    public static void install(Activity a) {
        activity = a;
        density = a.getResources().getDisplayMetrics().density;
        a.runOnUiThread(NativeShim::installOnUi);
        // Nothing uploads before the app is up: last run's copies can go.
        final File uploads = new File(a.getCacheDir(), UPLOADS);
        new Thread(() -> deleteTree(uploads), "yap-uploads-clean").start();
    }

    private static void installOnUi() {
        // The keyboard (API 30+: WindowInsets.Type and the insets animation;
        // before that Qt's own adjustNothing handling is all there is): the
        // resting IME inset after any change, through the decor view's
        // listener, which hands the insets on unchanged so Qt's safe-area
        // handling still sees them; and every frame of the IME animation
        // through the content view. Not the decor view: Qt sets its own
        // animation callback there on each showKeyboard, replacing ours.
        if (Build.VERSION.SDK_INT >= 30) {
            installIme();
        }
        installBack();
    }

    private static void installIme() {
        View decor = activity.getWindow().getDecorView();
        decor.setOnApplyWindowInsetsListener((v, insets) -> {
            reportIme(insets);
            return v.onApplyWindowInsets(insets);
        });
        View content = activity.findViewById(android.R.id.content);
        content.setWindowInsetsAnimationCallback(
                new WindowInsetsAnimation.Callback(
                        WindowInsetsAnimation.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
                    @Override
                    public WindowInsets onProgress(WindowInsets insets,
                            List<WindowInsetsAnimation> running) {
                        reportIme(insets);
                        return insets;
                    }
                });
    }

    private static void installBack() {
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

    // ---- Photos and videos for the composer ---------------------------------

    /**
     * The system photo picker (API 33+: no permission, the user chooses what
     * the app sees), else the documents UI on images and videos. The C++
     * side starts it for a result (QtAndroidPrivate::startActivity).
     */
    public static Intent pickMediaIntent() {
        if (Build.VERSION.SDK_INT >= 33) {
            return new Intent(MediaStore.ACTION_PICK_IMAGES)
                    .putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX,
                            Math.min(PICK_MAX, MediaStore.getPickImagesMaxLimit()));
        }
        return new Intent(Intent.ACTION_GET_CONTENT)
                .addCategory(Intent.CATEGORY_OPENABLE)
                .setType("*/*")
                .putExtra(Intent.EXTRA_MIME_TYPES, new String[] {"image/*", "video/*"})
                .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
    }

    /**
     * The picker's result. The core uploads from a path and a content://
     * grant ends with the activity, so each item is copied into the app's
     * cache under its display name (with the extension its type implies),
     * off the UI thread; the paths go back through nativeMediaPicked, empty
     * when nothing was picked or nothing could be read.
     */
    public static void mediaPicked(int resultCode, Intent data) {
        final List<Uri> uris = new ArrayList<>();
        if (activity != null && resultCode == Activity.RESULT_OK && data != null) {
            ClipData clip = data.getClipData();
            if (clip != null) {
                for (int i = 0; i < clip.getItemCount() && uris.size() < PICK_MAX; i++) {
                    Uri uri = clip.getItemAt(i).getUri();
                    if (uri != null) uris.add(uri);
                }
            } else if (data.getData() != null) {
                uris.add(data.getData());
            }
        }
        if (uris.isEmpty()) {
            nativeMediaPicked(new String[0]);
            return;
        }
        nativeMediaCopying();
        final File batch = new File(new File(activity.getCacheDir(), UPLOADS),
                Long.toString(System.nanoTime()));
        new Thread(() -> {
            List<String> paths = new ArrayList<>();
            for (int i = 0; i < uris.size(); i++) {
                // One folder per item: two picks may share a display name.
                File copy = copyIn(uris.get(i), new File(batch, Integer.toString(i)));
                if (copy != null) paths.add(copy.getAbsolutePath());
            }
            nativeMediaPicked(paths.toArray(new String[0]));
        }, "yap-pick-copy").start();
    }

    private static File copyIn(Uri uri, File dir) {
        ContentResolver resolver = activity.getContentResolver();
        if (!dir.mkdirs() && !dir.isDirectory()) return null;
        File out = new File(dir, fileName(resolver, uri, resolver.getType(uri)));
        try (InputStream in = resolver.openInputStream(uri)) {
            if (in == null) return null;
            try (OutputStream to = new FileOutputStream(out)) {
                byte[] buffer = new byte[64 * 1024];
                int n;
                while ((n = in.read(buffer)) > 0) to.write(buffer, 0, n);
            }
            return out;
        } catch (IOException | RuntimeException e) {
            Log.w("yap-native", "could not copy a picked file", e);
            out.delete();
            return null;
        }
    }

    /** The item's display name, safe as a file name, with its type's extension. */
    private static String fileName(ContentResolver resolver, Uri uri, String mime) {
        String name = null;
        try (Cursor c = resolver.query(uri, new String[] {OpenableColumns.DISPLAY_NAME},
                null, null, null)) {
            if (c != null && c.moveToFirst() && !c.isNull(0)) name = c.getString(0);
        } catch (RuntimeException e) {
            // A provider without the column: the URI's last segment below.
        }
        if (name == null || name.trim().isEmpty()) name = uri.getLastPathSegment();
        if (name == null) name = "";
        name = name.replaceAll("[\\\\/:*?\"<>|\\x00-\\x1f]", "_").replaceAll("^[.\\s]+", "").trim();
        if (name.isEmpty()) name = "file";
        if (name.length() > 120) name = name.substring(name.length() - 120);
        // The core types an upload by its extension.
        String ext = mime == null ? null : MimeTypeMap.getSingleton().getExtensionFromMimeType(mime);
        String lower = name.toLowerCase(Locale.ROOT);
        if (ext != null && !lower.endsWith("." + ext)
                && !(ext.equals("jpg") && (lower.endsWith(".jpeg") || lower.endsWith(".jfif")))) {
            name = name + "." + ext;
        }
        return name;
    }

    private static void deleteTree(File file) {
        File[] children = file.listFiles();
        if (children != null) {
            for (File child : children) deleteTree(child);
        }
        file.delete();
    }
}
