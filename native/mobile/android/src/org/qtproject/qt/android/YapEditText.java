package org.qtproject.qt.android;

import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.ContentResolver;
import android.content.Context;
import android.content.pm.ProviderInfo;
import android.net.Uri;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputConnectionWrapper;
import android.view.inputmethod.InputContentInfo;
import android.view.inputmethod.InputMethodManager;

import java.lang.reflect.Field;

/**
 * Qt's text view for the soft keyboard (QtEditText), taught to take
 * pictures from the keyboard: Gboard's and Samsung Keyboard's image, GIF
 * and sticker insertion, and their clipboard chips (the commitContent API).
 *
 * Qt's own view offers the keyboard no content types (EditorInfo
 * contentMimeTypes stays null; the platform never fills it from
 * setOnReceiveContentListener for a plain View), so keyboards insert a link
 * or nothing. Each Qt window makes its QtEditText in its constructor and
 * keeps it in a final field; `install` swaps this subclass in for it (same
 * listener, same place in the window's layout) and points Qt's input
 * delegate at it. It lives in Qt's package for the package-private
 * constructor and fields. Anything unexpected (a Qt that renamed them)
 * leaves Qt's view in place: the keyboard then works as before, without
 * pictures.
 *
 * While `accepting` (the composer's field has focus, Native.acceptContent),
 * the view offers images and videos; what the keyboard commits goes to
 * `sink` as a content URI, read while the keyboard's grant lasts. And Qt's
 * selection bar offers Paste only when the clipboard holds text: with a
 * copied picture or file on it, ours puts Paste back, and Qt's Paste (the
 * Ctrl+V shortcut) reaches the composer, which attaches it.
 */
public class YapEditText extends QtEditText {
    private static final String TAG = "yap-native";
    private static final String[] TYPES = {"image/*", "video/*"};
    /** Qt's selection bar: its Paste button, and the "show the bar" mode bit. */
    private static final int PASTE_BUTTON = 4;
    private static final int SHOW_EDIT_POPUP = 256;
    private static final int SELECTION_MODE = 2;

    /** Where committed content goes (NativeShim copies it in). */
    public interface Sink {
        /** Reads `uri` (off the UI thread); `done` runs once it was read. */
        void receive(Uri uri, String mime, Runnable done);
    }

    private static volatile boolean accepting;
    private static volatile Sink sink;

    YapEditText(Context context, QtInputConnection.QtInputConnectionListener listener) {
        super(context, listener);
    }

    public static void setSink(Sink s) {
        sink = s;
    }

    /**
     * Offer pictures to the keyboard (or stop). A keyboard already connected
     * to this view is restarted so it sees the change. UI thread.
     */
    public static void setAccepting(View root, boolean on) {
        if (accepting == on) return;
        accepting = on;
        YapEditText view = find(root);
        if (view == null) return;
        InputMethodManager imm = (InputMethodManager)
                view.getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
        if (imm != null && imm.isActive(view)) imm.restartInput(view);
    }

    @Override
    void updateHandles(int mode, int editX, int editY, int editButtons, int x1, int y1, int x2,
                       int y2, boolean rtl) {
        super.updateHandles(mode, editX, editY, editButtons, x1, y1, x2, y2, rtl);
        boolean bar = (mode & 0xff) == SELECTION_MODE || (mode & SHOW_EDIT_POPUP) != 0;
        if (!accepting || !bar || (editButtons & PASTE_BUTTON) == 0
                || QtClipboardManager.hasClipboardText(getContext()) || !clipboardHasContent()) {
            return;
        }
        try {
            Field menu = QtEditText.class.getDeclaredField("m_editPopupMenu");
            menu.setAccessible(true);
            ((EditPopupMenu) menu.get(this)).setPosition(editX, editY, editButtons);
        } catch (ReflectiveOperationException | RuntimeException e) {
            Log.w(TAG, "selection bar: no Paste for a picture", e);
        }
    }

    /**
     * A copied picture or file (a non-text type) is on the clipboard. Only
     * its description: reading the clip itself would grant us its URIs and
     * show Android's "pasted from your clipboard" before the user pastes.
     */
    private boolean clipboardHasContent() {
        ClipboardManager clipboard = (ClipboardManager)
                getContext().getSystemService(Context.CLIPBOARD_SERVICE);
        try {
            ClipDescription description = clipboard == null ? null : clipboard.getPrimaryClipDescription();
            for (int i = 0; description != null && i < description.getMimeTypeCount(); i++) {
                String type = description.getMimeType(i);
                if (type != null && !ClipDescription.compareMimeTypes(type, "text/*")) return true;
            }
        } catch (RuntimeException e) {
            // No access: no Paste.
        }
        return false;
    }

    /**
     * Content from outside the app: a content:// URI whose provider is not
     * one of ours. A keyboard or the clipboard naming our own providers
     * (the capture or Qt's file providers) could make us upload a private
     * file; file:// and other schemes are refused too.
     */
    public static boolean isForeignContent(Context context, Uri uri) {
        if (context == null || uri == null || !ContentResolver.SCHEME_CONTENT.equals(uri.getScheme())) {
            return false;
        }
        String authority = uri.getAuthority();
        if (authority == null || authority.isEmpty()) return false;
        // "<user>@<authority>" reaches the same provider in that user.
        authority = authority.substring(authority.lastIndexOf('@') + 1);
        try {
            ProviderInfo provider = context.getPackageManager().resolveContentProvider(authority, 0);
            return provider == null || !context.getPackageName().equals(provider.packageName);
        } catch (RuntimeException e) {
            return false;
        }
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
        InputConnection ic = super.onCreateInputConnection(outAttrs);
        if (!accepting || ic == null) return ic;
        outAttrs.contentMimeTypes = TYPES;
        return new InputConnectionWrapper(ic, false) {
            @Override
            public boolean commitContent(InputContentInfo info, int flags, Bundle opts) {
                return YapEditText.commit(getContext(), info, flags);
            }
        };
    }

    private static boolean commit(Context context, InputContentInfo info, int flags) {
        Sink s = sink;
        if (!accepting || s == null || info == null) return false;
        if (!isForeignContent(context, info.getContentUri())) {
            Log.w(TAG, "keyboard content: refused a URI that is not another app's content");
            return false;
        }
        ClipDescription description = info.getDescription();
        String mime = null;
        for (int i = 0; description != null && i < description.getMimeTypeCount(); i++) {
            String type = description.getMimeType(i);
            if (ClipDescription.compareMimeTypes(type, "image/*")
                    || ClipDescription.compareMimeTypes(type, "video/*")) {
                mime = type;
                break;
            }
        }
        if (mime == null) return false;
        boolean granted = false;
        if ((flags & InputConnection.INPUT_CONTENT_GRANT_READ_URI_PERMISSION) != 0) {
            try {
                info.requestPermission();
                granted = true;
            } catch (Exception e) {
                Log.w(TAG, "keyboard content: no read grant", e);
                return false;
            }
        }
        final boolean release = granted;
        s.receive(info.getContentUri(), mime, () -> {
            if (release) info.releasePermission();
        });
        return true;
    }

    // ---- The swap ------------------------------------------------------------

    /** Swaps this view in for each Qt window's QtEditText under `root`. UI thread. */
    public static void install(View root) {
        try {
            Field editText = QtWindow.class.getDeclaredField("m_editText");
            Field listener = QtWindow.class.getDeclaredField("m_inputConnectionListener");
            editText.setAccessible(true);
            listener.setAccessible(true);
            swap(root, editText, listener);
        } catch (ReflectiveOperationException | RuntimeException e) {
            Log.w(TAG, "keyboard content: Qt's text view stays as it is", e);
        }
    }

    private static void swap(View view, Field editText, Field listener)
            throws ReflectiveOperationException {
        if (view instanceof QtWindow) {
            QtWindow window = (QtWindow) view;
            Object old = editText.get(window);
            Object l = listener.get(window);
            if (old instanceof QtEditText && !(old instanceof YapEditText)
                    && l instanceof QtInputConnection.QtInputConnectionListener) {
                QtEditText was = (QtEditText) old;
                int at = window.indexOfChild(was);
                if (at >= 0) {
                    YapEditText now = new YapEditText(window.getContext(),
                            (QtInputConnection.QtInputConnectionListener) l);
                    now.setFocusable(was.isFocusable());
                    now.setFocusableInTouchMode(was.isFocusableInTouchMode());
                    now.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
                    boolean focused = was.hasFocus();
                    ViewGroup.LayoutParams params = was.getLayoutParams();
                    editText.set(window, now);
                    window.removeView(was);
                    window.addView(now, at, params);
                    // Qt's input delegate follows the window's view.
                    window.updateFocusedEditText();
                    if (focused) now.requestFocus();
                }
            }
        }
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) {
                View child = group.getChildAt(i);
                if (!(child instanceof YapEditText)) swap(child, editText, listener);
            }
        }
    }

    private static YapEditText find(View view) {
        if (view instanceof YapEditText) return (YapEditText) view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) {
                YapEditText found = find(group.getChildAt(i));
                if (found != null) return found;
            }
        }
        return null;
    }
}
