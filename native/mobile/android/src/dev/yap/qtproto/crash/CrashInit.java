package dev.yap.qtproto.crash;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.net.Uri;

/**
 * Starts crash reporting in every process start, the push service's and
 * the receivers' with the app killed included: Android creates the
 * manifest's providers before any of them runs. It serves nothing.
 */
public final class CrashInit extends ContentProvider {
    @Override
    public boolean onCreate() {
        if (getContext() != null) CrashReports.init(getContext());
        return true;
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
        return null;
    }

    @Override
    public String getType(Uri uri) {
        return null;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        return null;
    }

    @Override
    public int delete(Uri uri, String selection, String[] args) {
        return 0;
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] args) {
        return 0;
    }
}
