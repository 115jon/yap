#pragma once

// Media Foundation is optional (docs/LIBRARIES.md, "Compatibility").
//
// Windows N and KN ship without it until the Media Feature Pack is
// installed: mfplat.dll, mf.dll, mfreadwrite.dll and evr.dll are missing.
// The exe delay-loads those (and dxva2.dll and msdmo.dll, which only Media
// Foundation code calls; CMakeLists.txt), so it starts there, and probe()
// decides once, first thing in main, whether anything may call them:
//
//   - Qt Multimedia's media player (mp4 clips, videos, audio previews):
//     QML checks Compat.mediaPlayback and shows a note instead. WebM keeps
//     playing (libvpx), and call video keeps drawing (VideoOutput only).
//   - the H.264 decoder (patches/webrtc-sys/0006): not offered, so calls
//     use VP8/VP9 on libvpx; no zero-copy decode.
//   - the hardware H.264 share encoder: YAP_SHARE_ENCODER=software (VP8).
//   - cameras (Qt's Media Foundation capture): none listed; the test
//     pattern (YAP_FAKE_CAMERA=1) still works.
//
// Any call that still reaches a missing DLL gets a stub that answers
// E_NOTIMPL (every function the exe imports from those DLLs returns an
// HRESULT) instead of the delay-load helper's 0xC06D007E exception.
//
// YAP_TEST_NO_MF=1 takes the same path on a PC that has Media Foundation:
// the delay-loaded imports resolve to the stubs, as on N. (Qt's own,
// LoadLibrary-based Media Foundation use in QtMultimedia, its camera list,
// still finds the real DLLs then; the camera list is emptied here for it.)

namespace mediafoundation {

// Once, before Qt and before anything makes a peer connection.
void probe();

// Whether Media Foundation may be called (true before probe()).
bool available();

// YAP_TEST_NO_MF=1 made it unavailable.
bool simulated();

} // namespace mediafoundation
