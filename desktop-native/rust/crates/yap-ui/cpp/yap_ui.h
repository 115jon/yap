#pragma once

class QQmlEngine;

// The shared C++ behind Yap.Ui (both apps): AnimatedPicture and
// ViewportWatch under Yap.Media (GIF, APNG and WebP through yap-anim), and
// the `icon` and `dither` image providers. Call before loading QML.
void yap_ui_register(QQmlEngine& engine);
