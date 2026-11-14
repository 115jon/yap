package dev.yap.qtproto.media;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Matrix;
import android.media.MediaMetadataRetriever;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import androidx.media3.common.AudioAttributes;
import androidx.media3.common.C;
import androidx.media3.common.MediaItem;
import androidx.media3.common.PlaybackException;
import androidx.media3.common.Player;
import androidx.media3.datasource.DefaultHttpDataSource;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory;

import java.io.ByteArrayOutputStream;
import java.util.HashMap;

/**
 * Video and sound for the Qt app (cpp/videoplayer.cpp): every URL here is
 * one a player loads as it is (public, or signed with a media cap by the
 * Rust side); no session, header or cookie is ever sent, and nothing is
 * written to disk.
 *
 * - {@link #play}: a video full screen in {@link PlayerActivity}.
 * - {@link #playAudio} / {@link #stopAudio}: one sound preview at a time (a
 *   TikTok post's sound), on a player made for it and released when it
 *   stops, ends or fails; its state goes to nativeAudioState.
 * - {@link #poster}: a video's first frame, scaled, as JPEG bytes, for the
 *   poster in the chat (from Qt's image loader thread; blocks).
 */
public final class MediaShim {
    private static final String TAG = "yap-media";

    /** cpp/videoplayer.cpp: 0 idle, 1 loading, 2 playing, 3 failed. */
    static native void nativeAudioState(int state);

    private static final int IDLE = 0;
    private static final int LOADING = 1;
    private static final int PLAYING = 2;
    private static final int FAILED = 3;

    private static final Handler MAIN = new Handler(Looper.getMainLooper());
    // Main thread only.
    private static ExoPlayer audio;

    private MediaShim() {}

    public static void play(Context context, String url, long positionMs, boolean loop, String title) {
        MAIN.post(() -> PlayerActivity.start(context, url, positionMs, loop, title));
    }

    public static void playAudio(Context context, String url) {
        MAIN.post(() -> {
            releaseAudio();
            DefaultHttpDataSource.Factory http = new DefaultHttpDataSource.Factory()
                .setUserAgent("YapMobile")
                .setAllowCrossProtocolRedirects(false);
            ExoPlayer player = new ExoPlayer.Builder(context.getApplicationContext())
                .setMediaSourceFactory(new DefaultMediaSourceFactory(http))
                .setAudioAttributes(new AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                    .build(), /* handleAudioFocus= */ true)
                .setHandleAudioBecomingNoisy(true)
                .build();
            audio = player;
            player.addListener(new Player.Listener() {
                @Override
                public void onPlaybackStateChanged(int state) {
                    if (audio != player) return;
                    if (state == Player.STATE_ENDED) {
                        releaseAudio();
                        nativeAudioState(IDLE);
                    } else if (state == Player.STATE_READY && player.getPlayWhenReady()) {
                        nativeAudioState(PLAYING);
                    }
                }

                @Override
                public void onPlayWhenReadyChanged(boolean playWhenReady, int reason) {
                    // Audio focus lost for good, or headphones out: it stops.
                    if (audio == player && !playWhenReady) {
                        releaseAudio();
                        nativeAudioState(IDLE);
                    }
                }

                @Override
                public void onPlayerError(PlaybackException error) {
                    if (audio != player) return;
                    Log.w(TAG, "sound preview failed: " + error.getErrorCodeName());
                    releaseAudio();
                    nativeAudioState(FAILED);
                }
            });
            nativeAudioState(LOADING);
            player.setMediaItem(MediaItem.fromUri(url));
            player.setPlayWhenReady(true);
            player.prepare();
        });
    }

    public static void stopAudio() {
        MAIN.post(() -> {
            if (audio == null) return;
            releaseAudio();
            nativeAudioState(IDLE);
        });
    }

    private static void releaseAudio() {
        if (audio == null) return;
        ExoPlayer player = audio;
        audio = null;
        player.release();
    }

    /**
     * The first frame of the video at `url`, scaled to fit `maxWidth` x
     * `maxHeight` and turned upright, as JPEG; null when it cannot be read.
     * The retriever reads only what it needs over HTTP (the index and the
     * first key frame) and holds one decoder while it runs.
     */
    public static byte[] poster(String url, int maxWidth, int maxHeight) {
        MediaMetadataRetriever retriever = new MediaMetadataRetriever();
        Bitmap frame = null;
        try {
            HashMap<String, String> headers = new HashMap<>();
            headers.put("User-Agent", "YapMobile");
            retriever.setDataSource(url, headers);
            int width = parse(retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH));
            int height = parse(retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT));
            int rotation = parse(retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION));
            boolean turned = rotation == 90 || rotation == 270;
            int shownW = turned ? height : width;
            int shownH = turned ? width : height;
            int w = Math.max(1, maxWidth);
            int h = Math.max(1, maxHeight);
            if (shownW > 0 && shownH > 0) {
                float scale = Math.min(1f, Math.min(w / (float) shownW, h / (float) shownH));
                w = Math.max(1, Math.round(shownW * scale));
                h = Math.max(1, Math.round(shownH * scale));
            }
            // In the file's own axes: the retriever scales before it turns.
            frame = retriever.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC,
                turned ? h : w, turned ? w : h);
            if (frame == null) return null;
            // Some decoders hand the frame over unturned: turn it here.
            if (turned && shownW > 0 && (frame.getWidth() > frame.getHeight()) != (shownW > shownH)) {
                Matrix turn = new Matrix();
                turn.postRotate(rotation);
                Bitmap upright = Bitmap.createBitmap(frame, 0, 0, frame.getWidth(), frame.getHeight(), turn, true);
                if (upright != frame) {
                    frame.recycle();
                    frame = upright;
                }
            }
            ByteArrayOutputStream out = new ByteArrayOutputStream(64 * 1024);
            frame.compress(Bitmap.CompressFormat.JPEG, 85, out);
            return out.toByteArray();
        } catch (Exception | OutOfMemoryError e) {
            Log.w(TAG, "no poster frame: " + e.getClass().getSimpleName());
            return null;
        } finally {
            if (frame != null) frame.recycle();
            try {
                retriever.release();
            } catch (Exception ignored) {
                // Released either way.
            }
        }
    }

    private static int parse(String value) {
        if (value == null) return 0;
        try {
            return Integer.parseInt(value.trim());
        } catch (NumberFormatException e) {
            return 0;
        }
    }
}
