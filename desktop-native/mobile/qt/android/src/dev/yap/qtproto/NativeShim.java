package dev.yap.qtproto;

import android.Manifest;
import android.app.Activity;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.database.Cursor;
import android.graphics.Insets;
import android.hardware.SensorManager;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.util.Log;
import android.webkit.MimeTypeMap;
import android.view.Display;
import android.view.HapticFeedbackConstants;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewConfiguration;
import android.view.WindowInsets;
import android.view.WindowInsetsAnimation;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.window.BackEvent;
import android.window.OnBackAnimationCallback;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;
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
    static native void nativeEdges(float gestureBottom, float clippedBottom);
    static native void nativeMediaCopying();
    static native void nativeMediaPicked(String[] paths, int failed);
    static native void nativeImageSaved(int result);

    /** The most files one pick hands to the tray. */
    private static final int PICK_MAX = 10;
    /** Picked files are copied under the cache, here, until the next start. */
    private static final String UPLOADS = "uploads";
    /** Pictures being saved wait under the cache, here, on their way to the gallery. */
    private static final String SAVES = "saves";
    /** Saved pictures go in the gallery's Pictures/Yap. */
    private static final String GALLERY_FOLDER = "Yap";
    /** saveToGallery's results (Native.imageSaved). */
    private static final int SAVE_OK = 0;
    private static final int SAVE_FAILED = 1;
    private static final int SAVE_NEEDS_PERMISSION = 2;
    /** The storage permission's request code (API 28 only). */
    private static final int SAVE_PERMISSION_REQUEST = 0x5962;
    /** File names stay well under the file system's 255 bytes. */
    private static final int NAME_BYTES = 160;
    /** The account's attachment limit for the pick under way (bytes). */
    private static volatile long pickLimit = Long.MAX_VALUE;

    private static Activity activity;
    private static OnBackInvokedCallback back;
    private static boolean backRegistered;
    /** The app has a page to go back to (Native.handlesBack). */
    private static boolean backWanted;
    /** Back at the root leaves the task alive without our help. */
    private static boolean systemKeepsTask;
    private static float density = 1f;
    /** What nativeEdges was last told (px), so a layout pass that changes nothing stays quiet. */
    private static int edgeGesture = -1;
    private static int edgeClipped = -1;

    public static void install(Activity a) {
        activity = a;
        density = a.getResources().getDisplayMetrics().density;
        a.runOnUiThread(NativeShim::installOnUi);
        // Nothing uploads before the app is up: last run's copies can go.
        final File uploads = new File(a.getCacheDir(), UPLOADS);
        final File saves = new File(a.getCacheDir(), SAVES);
        new Thread(() -> {
            deleteTree(uploads);
            deleteTree(saves);
        }, "yap-uploads-clean").start();
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
            preferFastestRefresh();
        }
        installBack();
    }

    /**
     * The display's fastest refresh rate at the current resolution, asked
     * for by the window and by the surface Qt draws into. Without a vote the
     * system runs an app it knows nothing about at its "high" frame rate
     * category (90 Hz on some 120 Hz panels). The user's own cap in the
     * system settings still applies: these are requests.
     */
    private static void preferFastestRefresh() {
        Display display = activity.getDisplay();
        if (display == null) return;
        Display.Mode current = display.getMode();
        Display.Mode fastest = current;
        for (Display.Mode mode : display.getSupportedModes()) {
            if (mode.getPhysicalWidth() == current.getPhysicalWidth()
                    && mode.getPhysicalHeight() == current.getPhysicalHeight()
                    && mode.getRefreshRate() > fastest.getRefreshRate()) {
                fastest = mode;
            }
        }
        final float rate = fastest.getRefreshRate();
        WindowManager.LayoutParams attrs = activity.getWindow().getAttributes();
        attrs.preferredDisplayModeId = fastest.getModeId();
        attrs.preferredRefreshRate = rate;
        activity.getWindow().setAttributes(attrs);
        // Qt draws into a SurfaceView of its own (made after this runs, and
        // made again after the app was away): its surface asks too.
        View decor = activity.getWindow().getDecorView();
        decor.getViewTreeObserver().addOnGlobalLayoutListener(() -> {
            SurfaceView view = findSurfaceView(decor);
            if (view == null || view == refreshView) return;
            refreshView = view;
            SurfaceHolder holder = view.getHolder();
            holder.addCallback(new SurfaceHolder.Callback() {
                @Override
                public void surfaceCreated(SurfaceHolder h) { setFrameRate(h.getSurface(), rate); }

                @Override
                public void surfaceChanged(SurfaceHolder h, int format, int width, int height) {}

                @Override
                public void surfaceDestroyed(SurfaceHolder h) {}
            });
            setFrameRate(holder.getSurface(), rate);
        });
    }

    /** The SurfaceView whose refresh rate is asked for (Qt's). */
    private static SurfaceView refreshView;

    private static SurfaceView findSurfaceView(View view) {
        if (view instanceof SurfaceView) return (SurfaceView) view;
        if (!(view instanceof ViewGroup)) return null;
        ViewGroup group = (ViewGroup) view;
        for (int i = 0; i < group.getChildCount(); i++) {
            SurfaceView found = findSurfaceView(group.getChildAt(i));
            if (found != null) return found;
        }
        return null;
    }

    private static void setFrameRate(Surface surface, float rate) {
        if (surface == null || !surface.isValid()) return;
        try {
            surface.setFrameRate(rate, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT);
        } catch (RuntimeException e) {
            Log.w("yap-native", "could not ask for the refresh rate", e);
        }
    }

    private static void installIme() {
        View decor = activity.getWindow().getDecorView();
        decor.setOnApplyWindowInsetsListener((v, insets) -> {
            reportIme(insets);
            reportEdges(insets);
            return v.onApplyWindowInsets(insets);
        });
        // Every layout pass: the window Qt draws can come out taller than
        // the view that holds it (below).
        decor.getViewTreeObserver().addOnGlobalLayoutListener(() -> {
            WindowInsets insets = decor.getRootWindowInsets();
            if (insets != null) reportEdges(insets);
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
        systemKeepsTask = keepsTaskAtRoot();
        applyBack();
    }

    /**
     * Whether the system moves the task to the back, instead of finishing
     * the activity, when Back reaches its default handler at the root.
     * Finishing ends the process (Qt's onDestroy exits), so the next start
     * would be a cold one. Android 12 and 13 keep any launcher activity that
     * is its task's base; Android 14 and later only one the home app started
     * (ActivityClientController.shouldMoveTaskToBack), so a start from a
     * notification, a link or another app is finished. There the app takes
     * Back at the root itself and moves the task back (leave()); the price
     * is the system's back-to-home preview, which only its own handler gets.
     */
    private static boolean keepsTaskAtRoot() {
        if (Build.VERSION.SDK_INT < 34) return true;
        Intent intent = activity.getIntent();
        if (intent == null || !Intent.ACTION_MAIN.equals(intent.getAction())
                || intent.getCategories() == null || intent.getCategories().size() != 1
                || !intent.hasCategory(Intent.CATEGORY_LAUNCHER)
                || intent.getData() != null || intent.getType() != null) {
            return false;
        }
        Uri referrer = activity.getReferrer();
        if (referrer == null || !"android-app".equals(referrer.getScheme())) return false;
        ResolveInfo home = activity.getPackageManager().resolveActivity(
                new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME),
                PackageManager.MATCH_DEFAULT_ONLY);
        return home != null && home.activityInfo != null
                && home.activityInfo.packageName.equals(referrer.getHost());
    }

    /** On the UI thread: our callback is in place exactly while it must be. */
    private static void applyBack() {
        if (back == null) return;
        boolean on = backWanted || !systemKeepsTask;
        OnBackInvokedDispatcher d = activity.getOnBackInvokedDispatcher();
        if (on && !backRegistered) {
            d.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, back);
            backRegistered = true;
        } else if (!on && backRegistered) {
            d.unregisterOnBackInvokedCallback(back);
            backRegistered = false;
        }
    }

    private static void reportIme(WindowInsets insets) {
        nativeIme(insets.getInsets(WindowInsets.Type.ime()).bottom / density);
    }

    /**
     * What Qt's safe area leaves out at the bottom (API 30+), in dp:
     *
     * - The bottom system gesture zone. Qt's safe area is the system bars and
     *   the cutout; with gesture navigation some phones (Samsung's One UI)
     *   report no navigation bar at all while the home gesture still takes
     *   the bottom strip, so a control at the edge sits under the handle.
     *   Bottom controls keep clear of the larger of the mandatory gesture
     *   inset and the tappable element inset, the areas Android says taps
     *   are not reliably delivered to (Compose's safeGestures).
     * - How far the window Qt draws reaches past the view that holds it. In
     *   a freeform window (Samsung's pop-up view) the caption bar pushes the
     *   content view down by its height, while Qt sizes its window to the
     *   whole task window: the bottom of the page is cut off by the caption's
     *   height. The shell keeps its content above that line (at most the
     *   caption bar's height).
     */
    private static void reportEdges(WindowInsets insets) {
        Insets mandatory = insets.getInsets(WindowInsets.Type.mandatorySystemGestures());
        Insets tappable = insets.getInsets(WindowInsets.Type.tappableElement());
        int gesture = Math.max(mandatory.bottom, tappable.bottom);
        // Only as much as the caption bar took: while the window turns, Qt's
        // window keeps its old size for a frame or two and reaches far
        // below the layout (it has no caption bar then).
        int clipped = Math.min(clippedBottom(), insets.getInsets(WindowInsets.Type.captionBar()).top);
        if (gesture == edgeGesture && clipped == edgeClipped) return;
        edgeGesture = gesture;
        edgeClipped = clipped;
        nativeEdges(gesture / density, clipped / density);
    }

    /** Pixels of Qt's window below the bottom of its parent layout (0: none). */
    private static int clippedBottom() {
        View content = activity.findViewById(android.R.id.content);
        if (!(content instanceof ViewGroup) || ((ViewGroup) content).getChildCount() == 0) return 0;
        View root = ((ViewGroup) content).getChildAt(0);
        if (!(root instanceof ViewGroup)) return 0;
        ViewGroup layout = (ViewGroup) root;
        int clipped = 0;
        for (int i = 0; i < layout.getChildCount(); i++) {
            View child = layout.getChildAt(i);
            if (child.getVisibility() != View.VISIBLE) continue;
            clipped = Math.max(clipped, child.getBottom() - layout.getHeight());
        }
        return clipped;
    }

    /**
     * The IME inset as it is now. Back from another activity (the photo
     * picker) the keyboard may have gone while the listeners were not
     * called, which would leave the composer floating where it was.
     */
    public static void refreshIme() {
        if (activity == null || Build.VERSION.SDK_INT < 30) return;
        activity.runOnUiThread(() -> {
            View decor = activity.getWindow().getDecorView();
            decor.post(() -> {
                WindowInsets insets = decor.getRootWindowInsets();
                if (insets != null) {
                    reportIme(insets);
                    reportEdges(insets);
                }
            });
        });
    }

    /**
     * Takes the back gesture while the app has a page to go back to, and at
     * the root too when the system would finish the activity there
     * (keepsTaskAtRoot).
     */
    public static void setHandlesBack(boolean on) {
        if (activity == null) return;
        activity.runOnUiThread(() -> {
            backWanted = on;
            applyBack();
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

    /**
     * Dark status and navigation bar icons (`light`: the ground under the
     * bars is light), or light ones.
     */
    @SuppressWarnings("deprecation")
    public static void setLightBars(boolean light) {
        if (activity == null) return;
        activity.runOnUiThread(() -> {
            View decor = activity.getWindow().getDecorView();
            if (Build.VERSION.SDK_INT >= 30) {
                WindowInsetsController c = decor.getWindowInsetsController();
                if (c == null) return;
                int mask = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                        | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
                c.setSystemBarsAppearance(light ? mask : 0, mask);
            } else {
                // API 28 and 29: the view flags (status bar 23+, navigation 26+).
                int mask = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                        | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
                int flags = decor.getSystemUiVisibility();
                decor.setSystemUiVisibility(light ? flags | mask : flags & ~mask);
            }
        });
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
    public static Intent pickMediaIntent(long limitBytes) {
        pickLimit = limitBytes > 0 ? limitBytes : Long.MAX_VALUE;
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
     * off the UI thread; the paths go back through nativeMediaPicked, with
     * how many picked items could not be read. An item over the account's
     * limit is not copied: a sparse file of its size stands in, which the
     * core turns into its too-big card without reading it.
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
            nativeMediaPicked(new String[0], 0);
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
            nativeMediaPicked(paths.toArray(new String[0]), uris.size() - paths.size());
        }, "yap-pick-copy").start();
    }

    private static File copyIn(Uri uri, File dir) {
        ContentResolver resolver = activity.getContentResolver();
        if (!dir.mkdirs() && !dir.isDirectory()) return null;
        File out = new File(dir, fileName(resolver, uri, resolver.getType(uri)));
        long size = size(resolver, uri);
        if (size > pickLimit) {
            try (RandomAccessFile stand = new RandomAccessFile(out, "rw")) {
                stand.setLength(size);
                return out;
            } catch (IOException e) {
                Log.w("yap-native", "could not size a picked file", e);
                out.delete();
                return null;
            }
        }
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

    /** The item's size from its provider, or -1 when it does not say. */
    private static long size(ContentResolver resolver, Uri uri) {
        try (Cursor c = resolver.query(uri, new String[] {OpenableColumns.SIZE},
                null, null, null)) {
            if (c != null && c.moveToFirst() && !c.isNull(0)) return c.getLong(0);
        } catch (RuntimeException e) {
            // No size column: copied in full.
        }
        return -1;
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
        // The core types an upload by its extension.
        String ext = mime == null ? null : MimeTypeMap.getSingleton().getExtensionFromMimeType(mime);
        String lower = name.toLowerCase(Locale.ROOT);
        if (ext != null && !lower.endsWith("." + ext)
                && !(ext.equals("jpg") && (lower.endsWith(".jpeg") || lower.endsWith(".jfif")))) {
            name = name + "." + ext;
        }
        // Too long in bytes: whole code points go from the front, so the
        // extension stays.
        while (name.getBytes(StandardCharsets.UTF_8).length > NAME_BYTES) {
            name = name.substring(Character.charCount(name.codePointAt(0)));
        }
        return name;
    }

    /**
     * Where a picture to save is downloaded first: a fresh folder under the
     * cache, the name made safe as a file name. Empty before install.
     */
    public static String saveTarget(String name) {
        if (activity == null) return "";
        String safe = name == null ? "" : name.replaceAll("[\\\\/:*?\"<>|\\x00-\\x1f]", "_")
                .replaceAll("^[.\\s]+", "").trim();
        if (safe.isEmpty()) safe = "image";
        while (safe.getBytes(StandardCharsets.UTF_8).length > NAME_BYTES) {
            safe = safe.substring(Character.charCount(safe.codePointAt(0)));
        }
        File dir = new File(new File(activity.getCacheDir(), SAVES), Long.toString(System.nanoTime()));
        return new File(dir, safe).getAbsolutePath();
    }

    /**
     * Copies a downloaded picture (`saveTarget`'s path) into the gallery,
     * Pictures/Yap, through MediaStore, off the UI thread, then deletes the
     * download; the result goes back through nativeImageSaved. API 29+
     * needs no permission for the app's own entries; API 28 needs the
     * storage permission, asked here the first time (the save is then
     * reported as waiting on it).
     */
    public static void saveToGallery(final String path, final String mime) {
        final File file = new File(path);
        final File folder = file.getParentFile();
        if (activity == null || !file.isFile()) {
            if (folder != null) deleteTree(folder);
            nativeImageSaved(SAVE_FAILED);
            return;
        }
        if (Build.VERSION.SDK_INT < 29
                && activity.checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE)
                        != PackageManager.PERMISSION_GRANTED) {
            if (folder != null) deleteTree(folder);
            activity.runOnUiThread(() -> activity.requestPermissions(
                    new String[] {Manifest.permission.WRITE_EXTERNAL_STORAGE}, SAVE_PERMISSION_REQUEST));
            nativeImageSaved(SAVE_NEEDS_PERMISSION);
            return;
        }
        new Thread(() -> {
            boolean ok = writeToGallery(file, mime);
            if (folder != null) deleteTree(folder);
            nativeImageSaved(ok ? SAVE_OK : SAVE_FAILED);
        }, "yap-save-image").start();
    }

    private static boolean writeToGallery(File file, String mime) {
        ContentResolver resolver = activity.getContentResolver();
        String type = mime;
        if (type == null || !type.startsWith("image/")) {
            String name = file.getName();
            int dot = name.lastIndexOf('.');
            String ext = dot < 0 ? "" : name.substring(dot + 1).toLowerCase(Locale.ROOT);
            type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext);
            if (type == null || !type.startsWith("image/")) type = "image/jpeg";
        }
        ContentValues values = new ContentValues();
        values.put(MediaStore.MediaColumns.DISPLAY_NAME, file.getName());
        values.put(MediaStore.MediaColumns.MIME_TYPE, type);
        Uri collection;
        if (Build.VERSION.SDK_INT >= 29) {
            values.put(MediaStore.MediaColumns.RELATIVE_PATH,
                    Environment.DIRECTORY_PICTURES + "/" + GALLERY_FOLDER);
            values.put(MediaStore.MediaColumns.IS_PENDING, 1);
            collection = MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY);
        } else {
            File dir = new File(Environment.getExternalStoragePublicDirectory(
                    Environment.DIRECTORY_PICTURES), GALLERY_FOLDER);
            if (!dir.mkdirs() && !dir.isDirectory()) return false;
            values.put(MediaStore.MediaColumns.DATA, unusedFile(dir, file.getName()).getAbsolutePath());
            collection = MediaStore.Images.Media.EXTERNAL_CONTENT_URI;
        }
        Uri uri;
        try {
            uri = resolver.insert(collection, values);
        } catch (RuntimeException e) {
            Log.w("yap-native", "could not add a picture to the gallery", e);
            return false;
        }
        if (uri == null) return false;
        try (InputStream in = new FileInputStream(file);
                OutputStream out = resolver.openOutputStream(uri)) {
            if (out == null) throw new IOException("no output stream");
            byte[] buffer = new byte[64 * 1024];
            int n;
            while ((n = in.read(buffer)) > 0) out.write(buffer, 0, n);
        } catch (IOException | RuntimeException e) {
            Log.w("yap-native", "could not write a picture to the gallery", e);
            try {
                resolver.delete(uri, null, null);
            } catch (RuntimeException ignored) {
                // Already gone.
            }
            return false;
        }
        if (Build.VERSION.SDK_INT >= 29) {
            ContentValues done = new ContentValues();
            done.put(MediaStore.MediaColumns.IS_PENDING, 0);
            resolver.update(uri, done, null, null);
        }
        return true;
    }

    /** `name` in `dir`, or "name (2).ext" and so on when it is taken (API 28's file path). */
    private static File unusedFile(File dir, String name) {
        File out = new File(dir, name);
        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String ext = dot > 0 ? name.substring(dot) : "";
        for (int i = 2; out.exists() && i < 1000; i++) {
            out = new File(dir, stem + " (" + i + ")" + ext);
        }
        return out;
    }

    private static void deleteTree(File file) {
        File[] children = file.listFiles();
        if (children != null) {
            for (File child : children) deleteTree(child);
        }
        file.delete();
    }
}
