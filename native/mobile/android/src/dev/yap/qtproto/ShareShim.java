package dev.yap.qtproto;

import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;
import android.os.Parcelable;

import java.util.ArrayList;
import java.util.List;

/**
 * Shares into the app (ShareActivity's SEND and SEND_MULTIPLE, handed to
 * YapActivity): YapActivity offers each intent, as it does a link. The
 * share waits here, its items still the sender's content:// URIs (read
 * through the grant the activity holds), until the user picks where it
 * goes: `Native` (cpp/links.cpp) hears there is one (nativeShare), holds it
 * for the QML until it is signed in, and asks for the copies once a
 * conversation is picked (copy). One share at a time: the latest wins, as
 * a second link does.
 */
public final class ShareShim {
    private ShareShim() {}

    /** A share's text and how many files it has (and how many it dropped). */
    static native void nativeShare(String text, int files, int dropped);
    /** copy()'s result: the copies' paths, and how many items could not be read. */
    static native void nativeShareCopied(String[] paths, int failed);

    /** The most files one share hands to the tray (the picker's PICK_MAX). */
    private static final int SHARE_MAX = 10;
    /** The server's longest message (messages.ts). */
    private static final int TEXT_MAX = 4000;

    private static final class Share {
        final String text;
        final List<Uri> uris;
        final String mime;
        final int dropped;

        Share(String text, List<Uri> uris, String mime, int dropped) {
            this.text = text;
            this.uris = uris;
            this.mime = mime;
            this.dropped = dropped;
        }
    }

    /** Qt is up and nativeShare is registered. */
    private static volatile boolean running;
    /** The last intent handed over (the activity keeps its intent). */
    private static Intent handled;
    /** The share waiting for copy() or drop(). */
    private static Share share;

    /** After nativeShare is registered: a share from before, if any. */
    public static void install() {
        Share s;
        synchronized (ShareShim.class) {
            running = true;
            s = share;
        }
        if (s != null) nativeShare(s.text, s.uris.size(), s.dropped);
    }

    /** The activity's intent: a SEND or SEND_MULTIPLE is a share. */
    public static boolean offer(Intent intent) {
        if (intent == null) return false;
        String action = intent.getAction();
        boolean multiple = Intent.ACTION_SEND_MULTIPLE.equals(action);
        if (!multiple && !Intent.ACTION_SEND.equals(action)) return false;
        List<Uri> uris = new ArrayList<>();
        int found = 0;
        try {
            if (multiple) {
                ArrayList<Parcelable> streams = intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM);
                for (int i = 0; streams != null && i < streams.size(); i++) {
                    if (streams.get(i) instanceof Uri) found += add(uris, (Uri) streams.get(i));
                }
            } else {
                Parcelable stream = intent.getParcelableExtra(Intent.EXTRA_STREAM);
                if (stream instanceof Uri) found += add(uris, (Uri) stream);
            }
        } catch (RuntimeException e) {
            // A sender's extras that do not unparcel: its ClipData below.
        }
        // The ClipData mirrors EXTRA_STREAM for the grant; a text share's is
        // the sheet's preview (Chrome's page thumbnail), not something to
        // send. Read only when a sender gave no stream and no text type.
        String type = intent.getType();
        boolean textType = type != null && type.toLowerCase(java.util.Locale.ROOT).startsWith("text/");
        ClipData clip = uris.isEmpty() && !textType ? intent.getClipData() : null;
        for (int i = 0; clip != null && i < clip.getItemCount(); i++) {
            Uri uri = clip.getItemAt(i).getUri();
            if (uri != null) found += add(uris, uri);
        }
        String text = text(intent, multiple);
        if (uris.isEmpty() && text.isEmpty()) return true;
        Share s = new Share(text, uris, intent.getType(), Math.max(0, found - uris.size()));
        synchronized (ShareShim.class) {
            if (intent == handled) return true;
            handled = intent;
            share = s;
            if (!running) return true;
        }
        nativeShare(s.text, s.uris.size(), s.dropped);
        return true;
    }

    /** Adds `uri` once, up to SHARE_MAX; 1 when it is new (kept or dropped). */
    private static int add(List<Uri> uris, Uri uri) {
        if (uris.contains(uri)) return 0;
        if (uris.size() < SHARE_MAX) uris.add(uri);
        return 1;
    }

    /** EXTRA_TEXT (every one of SEND_MULTIPLE's), else the subject; capped. */
    private static String text(Intent intent, boolean multiple) {
        StringBuilder out = new StringBuilder();
        try {
            ArrayList<CharSequence> texts = multiple
                    ? intent.getCharSequenceArrayListExtra(Intent.EXTRA_TEXT) : null;
            if (texts != null) {
                for (CharSequence t : texts) {
                    if (t == null || t.toString().trim().isEmpty()) continue;
                    if (out.length() > 0) out.append('\n');
                    out.append(t.toString().trim());
                }
            }
        } catch (RuntimeException e) {
            // A single EXTRA_TEXT under SEND_MULTIPLE: read below.
        }
        if (out.length() == 0) {
            CharSequence t = null;
            try {
                t = intent.getCharSequenceExtra(Intent.EXTRA_TEXT);
            } catch (RuntimeException e) {
                // Not text.
            }
            if (t == null || t.toString().trim().isEmpty()) {
                String subject = intent.getStringExtra(Intent.EXTRA_SUBJECT);
                t = subject;
            }
            if (t != null) out.append(t.toString().trim());
        }
        String text = out.toString();
        if (text.length() > TEXT_MAX) {
            int end = TEXT_MAX;
            // Not half a surrogate pair.
            if (Character.isHighSurrogate(text.charAt(end - 1))) end--;
            text = text.substring(0, end);
        }
        return text;
    }

    /**
     * The waiting share's files copied in (NativeShim.copyShared: other
     * apps' content only, over `limitBytes` a sized stand-in), off the
     * calling thread; the paths come back through nativeShareCopied. The
     * share is done with after this.
     */
    public static void copy(long limitBytes) {
        final Share s;
        synchronized (ShareShim.class) {
            s = share;
            share = null;
        }
        if (s == null || s.uris.isEmpty()) {
            nativeShareCopied(new String[0], 0);
            return;
        }
        new Thread(() -> {
            List<String> paths = NativeShim.copyShared(s.uris, s.mime, limitBytes);
            nativeShareCopied(paths.toArray(new String[0]), s.uris.size() - paths.size());
        }, "yap-share-copy").start();
    }

    /** The share was cancelled, or its account signed out: it goes. */
    public static void drop() {
        synchronized (ShareShim.class) {
            share = null;
        }
    }
}
