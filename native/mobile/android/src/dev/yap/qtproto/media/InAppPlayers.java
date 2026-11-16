package dev.yap.qtproto.media;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Context;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.KeyEvent;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.webkit.RenderProcessGoneDetail;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

import androidx.media3.common.AudioAttributes;
import androidx.media3.common.C;
import androidx.media3.common.MediaItem;
import androidx.media3.common.PlaybackException;
import androidx.media3.common.Player;
import androidx.media3.datasource.DefaultHttpDataSource;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory;
import androidx.media3.ui.PlayerView;

import org.json.JSONObject;

import java.util.HashMap;
import java.util.regex.Pattern;

/**
 * Players inside the app's window (cpp/inappplayer.cpp; the chat's player
 * sheet, MediaSheet.qml): Android views that Qt shows through a foreign
 * window (QWindow::fromWinId, QML's WindowContainer), so they ride with the
 * sheet, and back and a drag on its handle close it like any other sheet.
 *
 * - {@link #video}: a video file on Media3's ExoPlayer in its PlayerView
 *   (on a TextureView, which moves with the sheet), with the player's own
 *   controls; its full-screen key hands the video to {@link PlayerActivity}
 *   at the same point, and back in the sheet it carries on from where
 *   full screen left it. The URL is one the player loads as it is (public,
 *   or signed with a media cap); nothing is sent with it or kept on disk.
 * - {@link #web}: the web's embed players (YouTube, Spotify, TikTok's own,
 *   X, Instagram) in a WebView, framed as the desktop frames them
 *   (rust/src/webembed.rs): a small page under {@link #BASE} holds the
 *   player in an iframe with the web's sandbox, and only for the hosts the
 *   desktop allows. YouTube's embed refuses to play without an embedding
 *   page (error 153). The page's CSP keeps the frame on those hosts. The
 *   page itself never navigates: a link the user taps opens outside the
 *   app, any other navigation is dropped. The
 *   player's full screen covers the activity until back or its own key.
 *
 * Each player has the id Qt gave it; it is made, paused, resumed and
 * released on the main thread, and what happens goes back to
 * nativeEvent(id, ...).
 */
public final class InAppPlayers {
    private static final String TAG = "yap-media";

    /** cpp/inappplayer.cpp: the view to show (on the main thread). */
    static native void nativeViewMade(long id, View view);
    /** cpp/inappplayer.cpp: READY, FAILED, FULL_SCREEN, WINDOWED. */
    static native void nativeEvent(long id, int event);

    static final int READY = 1;
    static final int FAILED = 2;
    static final int FULL_SCREEN = 3;
    static final int WINDOWED = 4;

    /** The frame page's origin (the desktop's virtual host). */
    static final String BASE = "https://player.yapnative.local/";

    /** The desktop's frame page's rule (rust/src/webembed.rs, PAGE). */
    private static final Pattern ALLOWED = Pattern.compile(
        "^https://((www\\.)?youtube(-nocookie)?\\.com|([a-z0-9-]+\\.)?tiktok\\.com|open\\.spotify\\.com"
            + "|(www\\.)?instagram\\.com|platform\\.twitter\\.com|syndication\\.twitter\\.com)/.*");

    private static final Handler MAIN = new Handler(Looper.getMainLooper());
    // Main thread only.
    private static final HashMap<Long, Holder> HOLDERS = new HashMap<>();

    private InAppPlayers() {}

    /** Whether `url` is a player page the web player frames. */
    public static boolean allowed(String url) {
        return url != null && ALLOWED.matcher(url).matches();
    }

    public static void video(Context context, long id, String url, long positionMs, boolean loop, String title) {
        MAIN.post(() -> {
            if (!isWeb(url)) {
                nativeEvent(id, FAILED);
                return;
            }
            VideoHolder holder = new VideoHolder(context, id, Uri.parse(url), positionMs, loop, title);
            HOLDERS.put(id, holder);
            nativeViewMade(id, holder.root);
            holder.start();
        });
    }

    public static void web(Context context, long id, String url, String title) {
        MAIN.post(() -> {
            if (!allowed(url)) {
                nativeEvent(id, FAILED);
                return;
            }
            WebHolder holder;
            try {
                holder = new WebHolder(context, id, url, title);
            } catch (RuntimeException e) {
                // No WebView on this phone (disabled, or being updated).
                Log.w(TAG, "no web view: " + e.getClass().getSimpleName());
                nativeEvent(id, FAILED);
                return;
            }
            HOLDERS.put(id, holder);
            nativeViewMade(id, holder.root);
        });
    }

    /** The app left the front: a video lets its player go, a page pauses. */
    public static void background(long id) {
        MAIN.post(() -> {
            Holder holder = HOLDERS.get(id);
            if (holder != null) holder.background();
        });
    }

    /** Back in front. */
    public static void foreground(long id) {
        MAIN.post(() -> {
            Holder holder = HOLDERS.get(id);
            if (holder != null) holder.foreground();
        });
    }

    /** Leave the web player's full screen (back while it is up). */
    public static void windowed(long id) {
        MAIN.post(() -> {
            Holder holder = HOLDERS.get(id);
            if (holder != null) holder.windowed();
        });
    }

    public static void release(long id) {
        MAIN.post(() -> {
            Holder holder = HOLDERS.remove(id);
            if (holder != null) holder.release();
        });
    }

    static boolean isWeb(String url) {
        return url != null && (url.startsWith("https://") || url.startsWith("http://"));
    }

    static void openOutside(Context context, String url) {
        if (!isWeb(url)) return;
        try {
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
            if (!(context instanceof Activity)) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            context.startActivity(intent);
        } catch (ActivityNotFoundException e) {
            Log.w(TAG, "nothing opens the link");
        }
    }

    private abstract static class Holder {
        final long id;
        View root;

        Holder(long id) {
            this.id = id;
        }

        abstract void background();
        abstract void foreground();
        void windowed() {}
        abstract void release();

        /** Off its parent (Qt removes it first; this is the backstop). */
        void detach() {
            if (root != null && root.getParent() instanceof ViewGroup) {
                ((ViewGroup) root.getParent()).removeView(root);
            }
        }
    }

    // ---- Video ----------------------------------------------------------------

    private static final class VideoHolder extends Holder {
        private final Context context;
        private final Uri uri;
        private final boolean loop;
        private final String title;
        private final PlayerView view;
        private ExoPlayer player;
        private long position;
        private boolean playWhenReady = true;
        private boolean ready;
        private boolean handedOff;

        VideoHolder(Context context, long id, Uri uri, long positionMs, boolean loop, String title) {
            super(id);
            this.context = context;
            this.uri = uri;
            this.loop = loop;
            this.title = title;
            this.position = Math.max(0, positionMs);
            // PlayerView on a TextureView (res/layout/yap_sheet_player.xml):
            // a SurfaceView would sit behind Qt's own surface.
            int layout = context.getResources().getIdentifier("yap_sheet_player", "layout", context.getPackageName());
            view = (PlayerView) LayoutInflater.from(context).inflate(layout, null, false);
            view.setBackgroundColor(Color.BLACK);
            view.setShutterBackgroundColor(Color.BLACK);
            view.setShowBuffering(PlayerView.SHOW_BUFFERING_WHEN_PLAYING);
            view.setKeepContentOnPlayerReset(true);
            if (loop) view.setControllerAutoShow(false);
            if (title != null && !title.isEmpty()) view.setContentDescription(title);
            // Full screen: the same video in PlayerActivity, from here.
            view.setFullscreenButtonClickListener(on -> fullScreen());
            root = view;
        }

        void start() {
            if (player != null) return;
            DefaultHttpDataSource.Factory http = new DefaultHttpDataSource.Factory()
                .setUserAgent("YapMobile")
                .setAllowCrossProtocolRedirects(false)
                .setConnectTimeoutMs(10_000)
                .setReadTimeoutMs(20_000);
            ExoPlayer made = new ExoPlayer.Builder(context)
                .setMediaSourceFactory(new DefaultMediaSourceFactory(http))
                .setAudioAttributes(new AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MOVIE)
                    .build(), /* handleAudioFocus= */ true)
                .setHandleAudioBecomingNoisy(true)
                .build();
            player = made;
            made.setRepeatMode(loop ? Player.REPEAT_MODE_ONE : Player.REPEAT_MODE_OFF);
            made.addListener(new Player.Listener() {
                @Override
                public void onPlaybackStateChanged(int state) {
                    if (player != made || ready || state != Player.STATE_READY) return;
                    ready = true;
                    nativeEvent(id, READY);
                }

                @Override
                public void onPlayerError(PlaybackException error) {
                    if (player != made) return;
                    Log.w(TAG, "in-app video failed: " + error.getErrorCodeName());
                    stop();
                    nativeEvent(id, FAILED);
                }
            });
            view.setPlayer(made);
            made.setMediaItem(MediaItem.fromUri(uri), position);
            made.setPlayWhenReady(playWhenReady);
            made.prepare();
        }

        /** Lets the player go, keeping where it was and whether it played. */
        void stop() {
            if (player == null) return;
            position = Math.max(0, player.getCurrentPosition());
            playWhenReady = player.getPlayWhenReady();
            view.setPlayer(null);
            player.release();
            player = null;
        }

        private void fullScreen() {
            if (player != null) {
                position = Math.max(0, player.getCurrentPosition());
                player.pause();
            }
            handedOff = true;
            // The key toggles its own icon; the sheet is never full screen.
            view.setFullscreenButtonState(false);
            PlayerActivity.start(context, uri.toString(), position, loop, title);
        }

        @Override
        void background() {
            // Home or the screen off: paused when it comes back. Full
            // screen: it goes on from where PlayerActivity leaves it.
            stop();
            if (!handedOff) playWhenReady = false;
        }

        @Override
        void foreground() {
            if (handedOff) {
                handedOff = false;
                PlayerActivity.Handoff back = PlayerActivity.takeHandoff(uri.toString());
                if (back != null) {
                    position = back.position;
                    playWhenReady = back.playing;
                } else {
                    playWhenReady = false;
                }
            }
            start();
        }

        @Override
        void release() {
            stop();
            detach();
        }
    }

    // ---- Web --------------------------------------------------------------------

    private static final class WebHolder extends Holder {
        private final Context context;
        private final WebView web;
        private View custom;
        private WebChromeClient.CustomViewCallback customDone;
        private OnBackInvokedCallback customBack;
        private boolean released;

        WebHolder(Context context, long id, String url, String title) {
            super(id);
            this.context = context;
            web = new WebView(context);
            web.setBackgroundColor(Color.BLACK);
            if (title != null && !title.isEmpty()) web.setContentDescription(title);
            WebSettings settings = web.getSettings();
            settings.setJavaScriptEnabled(true);
            settings.setDomStorageEnabled(true);
            // The tap that opened the sheet was the gesture.
            settings.setMediaPlaybackRequiresUserGesture(false);
            // A new window loads in place, where the client sends it out.
            settings.setSupportMultipleWindows(false);
            settings.setAllowFileAccess(false);
            settings.setAllowContentAccess(false);
            settings.setGeolocationEnabled(false);
            settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
            web.setWebViewClient(new WebViewClient() {
                @Override
                public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                    if (!request.isForMainFrame()) return false;
                    String target = request.getUrl().toString();
                    if (target.startsWith(BASE)) return false;
                    // Only a tap leaves the app (the web's rule).
                    if (request.hasGesture()) openOutside(context, target);
                    return true;
                }

                @Override
                public void onPageFinished(WebView view, String loaded) {
                    if (!released) nativeEvent(id, READY);
                }

                @Override
                public boolean onRenderProcessGone(WebView view, RenderProcessGoneDetail detail) {
                    // The renderer died (or was killed for memory): this
                    // view is dead, the sheet goes back to the card.
                    Log.w(TAG, "web player's renderer gone");
                    if (!released) {
                        release();
                        HOLDERS.remove(id);
                        nativeEvent(id, FAILED);
                    }
                    return true;
                }
            });
            web.setWebChromeClient(new WebChromeClient() {
                @Override
                public void onShowCustomView(View view, CustomViewCallback callback) {
                    showCustom(view, callback);
                }

                @Override
                public void onHideCustomView() {
                    hideCustom();
                }

                // No grey placeholder before the first frame.
                @Override
                public Bitmap getDefaultVideoPoster() {
                    return Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888);
                }
            });
            web.loadDataWithBaseURL(BASE, page(url), "text/html", "utf-8", null);
            root = web;
        }

        /**
         * Where the player may go once it is framed: the hosts the desktop
         * lets a player navigate to in place (rust/src/webembed.rs,
         * allowed(); YouTube's consent and TikTok's player subdomains
         * included). The page's CSP holds the frame to them through its
         * own navigations and redirects, as the desktop's frame guard does.
         */
        private static final String FRAME_HOSTS = "https://youtube.com https://*.youtube.com"
            + " https://youtube-nocookie.com https://*.youtube-nocookie.com"
            + " https://tiktok.com https://*.tiktok.com https://open.spotify.com"
            + " https://instagram.com https://www.instagram.com"
            + " https://platform.twitter.com https://syndication.twitter.com";

        /** The desktop's frame page, with the player's URL as a JS string. */
        private static String page(String url) {
            // JSONObject.quote escapes "/" too, so "</script>" cannot occur.
            return "<!doctype html><html><head><meta charset=\"utf-8\">"
                + "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none';"
                + " script-src 'unsafe-inline'; style-src 'unsafe-inline'; frame-src " + FRAME_HOSTS + "\">"
                + "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Player</title>"
                + "<style>html,body{margin:0;height:100%;background:#000;overflow:hidden}"
                + "iframe{border:0;width:100%;height:100%;display:block}</style></head><body><script>"
                + "const src=" + JSONObject.quote(url) + ";"
                + "const f=document.createElement('iframe');"
                + "const widget=src.startsWith('https://open.spotify.com/');"
                + "f.setAttribute('sandbox',widget"
                + "?'allow-forms allow-modals allow-popups allow-popups-to-escape-sandbox allow-presentation allow-scripts'"
                + ":'allow-presentation allow-same-origin allow-scripts');"
                + "f.setAttribute('allow','accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope;"
                + " picture-in-picture; web-share; fullscreen');"
                + "f.allowFullscreen=true;f.src=src;document.body.appendChild(f);"
                + "</script></body></html>";
        }

        private void showCustom(View view, WebChromeClient.CustomViewCallback callback) {
            if (custom != null || !(context instanceof Activity)) {
                callback.onCustomViewHidden();
                return;
            }
            Activity activity = (Activity) context;
            FrameLayout frame = new FrameLayout(activity);
            frame.setBackgroundColor(Color.BLACK);
            frame.addView(view, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
            // Back leaves full screen, not the sheet: below Android 13 as a
            // key on the focused frame, from 13 on as a callback above the
            // app's own (NativeShim's).
            frame.setFocusable(true);
            frame.setFocusableInTouchMode(true);
            frame.setOnKeyListener((v, code, event) -> {
                if (code != KeyEvent.KEYCODE_BACK) return false;
                if (event.getAction() == KeyEvent.ACTION_UP) hideCustom();
                return true;
            });
            ((ViewGroup) activity.getWindow().getDecorView()).addView(frame, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
            frame.requestFocus();
            if (Build.VERSION.SDK_INT >= 33) {
                customBack = this::hideCustom;
                activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                    OnBackInvokedDispatcher.PRIORITY_OVERLAY, customBack);
            }
            if (Build.VERSION.SDK_INT >= 30) {
                WindowInsetsController bars = activity.getWindow().getInsetsController();
                if (bars != null) {
                    bars.hide(WindowInsets.Type.systemBars());
                    bars.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                }
            }
            custom = frame;
            customDone = callback;
            nativeEvent(id, FULL_SCREEN);
        }

        private void hideCustom() {
            if (custom == null) return;
            Activity activity = (Activity) context;
            if (customBack != null && Build.VERSION.SDK_INT >= 33) {
                activity.getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback(customBack);
            }
            customBack = null;
            if (Build.VERSION.SDK_INT >= 30) {
                WindowInsetsController bars = activity.getWindow().getInsetsController();
                if (bars != null) bars.show(WindowInsets.Type.systemBars());
            }
            View frame = custom;
            WebChromeClient.CustomViewCallback done = customDone;
            custom = null;
            customDone = null;
            if (frame.getParent() instanceof ViewGroup) ((ViewGroup) frame.getParent()).removeView(frame);
            if (done != null) done.onCustomViewHidden();
            if (!released) nativeEvent(id, WINDOWED);
        }

        @Override
        void windowed() {
            hideCustom();
        }

        @Override
        void background() {
            web.onPause();
        }

        @Override
        void foreground() {
            web.onResume();
        }

        @Override
        void release() {
            if (released) return;
            released = true;
            hideCustom();
            detach();
            web.stopLoading();
            web.loadUrl("about:blank");
            web.destroy();
        }
    }
}
