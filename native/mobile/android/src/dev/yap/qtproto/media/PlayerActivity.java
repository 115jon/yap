package dev.yap.qtproto.media;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.Toast;

import androidx.media3.common.AudioAttributes;
import androidx.media3.common.C;
import androidx.media3.common.MediaItem;
import androidx.media3.common.PlaybackException;
import androidx.media3.common.Player;
import androidx.media3.datasource.DefaultHttpDataSource;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory;
import androidx.media3.ui.PlayerView;

/**
 * A video full screen (cpp/videoplayer.cpp starts it through MediaShim):
 * Media3's ExoPlayer in its own PlayerView on black, the bars hidden, the
 * player's own controls (play, seek, time). A GIF (a looping clip) plays
 * on repeat, its controls hidden until a tap. The URL is one the player can
 * load as it is: public, or the API's private file signed with a media
 * cap (src/mediacap.rs); no header, cookie or session goes with it, and
 * nothing is cached on disk.
 *
 * The player lives between onStart and onStop: in the background (Home, the
 * screen off, another app over it) it is released, and back in front it is
 * made again where it was, paused or playing as it was. Turning the phone
 * keeps it (configChanges in the manifest).
 *
 * Started from the chat's player sheet (InAppPlayers), it leaves where it
 * got to and whether it played ({@link #takeHandoff}), so the sheet goes on
 * from there.
 */
public final class PlayerActivity extends Activity {
    static final String EXTRA_URL = "dev.yap.qtproto.media.url";
    static final String EXTRA_POSITION = "dev.yap.qtproto.media.position";
    static final String EXTRA_LOOP = "dev.yap.qtproto.media.loop";
    static final String EXTRA_TITLE = "dev.yap.qtproto.media.title";

    private static final String STATE_POSITION = "position";
    private static final String STATE_PLAYING = "playing";

    /** Where a video full screen got to, for the sheet it came from. */
    static final class Handoff {
        final String url;
        final long position;
        final boolean playing;

        Handoff(String url, long position, boolean playing) {
            this.url = url;
            this.position = position;
            this.playing = playing;
        }
    }

    // Main thread only.
    private static Handoff handoff;

    /** The last full screen's end for `url`, once; null for another video. */
    static Handoff takeHandoff(String url) {
        Handoff last = handoff;
        handoff = null;
        return last != null && last.url.equals(url) ? last : null;
    }

    private PlayerView view;
    private ExoPlayer player;
    private Uri uri;
    private boolean loop;
    private long position;
    private boolean playWhenReady = true;

    static void start(Context context, String url, long positionMs, boolean loop, String title) {
        Intent intent = new Intent(context, PlayerActivity.class)
            .putExtra(EXTRA_URL, url)
            .putExtra(EXTRA_POSITION, Math.max(0, positionMs))
            .putExtra(EXTRA_LOOP, loop)
            .putExtra(EXTRA_TITLE, title);
        if (!(context instanceof Activity)) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        context.startActivity(intent);
    }

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        Intent intent = getIntent();
        String url = intent.getStringExtra(EXTRA_URL);
        if (url == null || !(url.startsWith("https://") || url.startsWith("http://"))) {
            finish();
            return;
        }
        uri = Uri.parse(url);
        loop = intent.getBooleanExtra(EXTRA_LOOP, false);
        position = intent.getLongExtra(EXTRA_POSITION, 0);
        if (saved != null) {
            position = saved.getLong(STATE_POSITION, position);
            playWhenReady = saved.getBoolean(STATE_PLAYING, true);
        }
        String title = intent.getStringExtra(EXTRA_TITLE);
        if (title != null && !title.isEmpty()) setTitle(title);

        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        getWindow().getDecorView().setBackgroundColor(Color.BLACK);
        view = new PlayerView(this);
        view.setLayoutParams(new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        view.setBackgroundColor(Color.BLACK);
        view.setShutterBackgroundColor(Color.BLACK);
        view.setShowBuffering(PlayerView.SHOW_BUFFERING_WHEN_PLAYING);
        view.setKeepContentOnPlayerReset(true);
        if (loop) view.setControllerAutoShow(false);
        if (title != null && !title.isEmpty()) view.setContentDescription(title);
        setContentView(view);
        hideBars();
    }

    // Android 9 and 10: the theme's full screen hides the status bar.
    private void hideBars() {
        if (Build.VERSION.SDK_INT < 30) return;
        WindowInsetsController bars = getWindow().getInsetsController();
        if (bars == null) return;
        bars.hide(WindowInsets.Type.systemBars());
        bars.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) hideBars();
    }

    @Override
    protected void onStart() {
        super.onStart();
        if (uri == null || player != null) return;
        DefaultHttpDataSource.Factory http = new DefaultHttpDataSource.Factory()
            .setUserAgent("YapMobile")
            .setAllowCrossProtocolRedirects(false)
            .setConnectTimeoutMs(10_000)
            .setReadTimeoutMs(20_000);
        player = new ExoPlayer.Builder(this)
            .setMediaSourceFactory(new DefaultMediaSourceFactory(http))
            .setAudioAttributes(new AudioAttributes.Builder()
                .setUsage(C.USAGE_MEDIA)
                .setContentType(C.AUDIO_CONTENT_TYPE_MOVIE)
                .build(), /* handleAudioFocus= */ true)
            .setHandleAudioBecomingNoisy(true)
            .build();
        player.setRepeatMode(loop ? Player.REPEAT_MODE_ONE : Player.REPEAT_MODE_OFF);
        player.addListener(new Player.Listener() {
            @Override
            public void onPlayerError(PlaybackException error) {
                Toast.makeText(PlayerActivity.this, "This video can't play here.", Toast.LENGTH_SHORT).show();
                finish();
            }
        });
        view.setPlayer(player);
        player.setMediaItem(MediaItem.fromUri(uri), position);
        player.setPlayWhenReady(playWhenReady);
        player.prepare();
    }

    // Before the activity under it resumes (onStop comes after).
    @Override
    protected void onPause() {
        super.onPause();
        if (uri == null) return;
        if (player != null) {
            position = Math.max(0, player.getCurrentPosition());
            playWhenReady = player.getPlayWhenReady();
        }
        handoff = new Handoff(uri.toString(), position, playWhenReady);
    }

    @Override
    protected void onStop() {
        super.onStop();
        release();
    }

    @Override
    protected void onDestroy() {
        release();
        super.onDestroy();
    }

    private void release() {
        if (player == null) return;
        position = Math.max(0, player.getCurrentPosition());
        playWhenReady = player.getPlayWhenReady();
        view.setPlayer(null);
        player.release();
        player = null;
    }

    @Override
    protected void onSaveInstanceState(Bundle out) {
        super.onSaveInstanceState(out);
        if (player != null) {
            position = Math.max(0, player.getCurrentPosition());
            playWhenReady = player.getPlayWhenReady();
        }
        out.putLong(STATE_POSITION, position);
        out.putBoolean(STATE_PLAYING, playWhenReady);
    }
}
