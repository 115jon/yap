package dev.yap.qtproto;

import androidx.core.content.FileProvider;

/**
 * Where the camera writes the composer's photo (NativeShim.takePhotoIntent):
 * a content:// URI on the app's cache/capture folder, writable by the camera
 * app for that one capture. Its own class and authority beside Qt's
 * FileProvider (the manifest can name a class only once); its paths are the
 * manifest entry's meta-data (res/xml/yap_capture_paths.xml).
 */
public final class CaptureProvider extends FileProvider {}
