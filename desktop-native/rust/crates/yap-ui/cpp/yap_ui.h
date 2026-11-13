#pragma once

class QQmlEngine;
class QUrl;

// The shared C++ behind Yap.Ui (both apps): AnimatedPicture and
// ViewportWatch under Yap.Media (GIF, APNG and WebP through yap-anim), and
// the `icon` and `dither` image providers. Call before loading QML.
void yap_ui_register(QQmlEngine& engine);

// Whether a media URL may be loaded: http(s), the app's own resources
// (qrc:, data:, image providers) and files on this device. Media URLs come
// from the server and from other users; a `file:` URL that names a host
// (`file://host/x`, `file:////host/x`) is a UNC path on Windows, which Qt
// opens over SMB or WebDAV, handing that host this device's address and
// possibly an NTLM hash. yap-core empties such values before QML sees them,
// and that is the only guard for QML's Image, which opens `file:` URLs with
// QFile itself. This is a second check where our own code loads: in
// AnimatedPicture, and in both apps' network managers (which `file:` URLs
// from Image never reach).
bool yap_ui_media_url_allowed(const QUrl& url);
