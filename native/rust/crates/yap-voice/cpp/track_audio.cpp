// Two WebRTC audio track calls the libwebrtc crate (0.3.55) does not
// expose: the playout volume of a remote track
// (AudioSourceInterface::SetVolume, which the receiver applies as the
// receive stream's gain) and the capture processing of the local mic
// (LiveKit's AudioTrackInterface::SetAudioProcessingOptions).
//
// Rust hands over the address of the livekit_ffi::AudioTrack it holds
// (engine/webrtc.rs). That class and its base livekit_ffi::MediaStreamTrack
// (webrtc-sys 0.3.52, include/livekit/media_stream_track.h) have no
// virtual functions, so the object starts with MediaStreamTrack's data: a
// shared_ptr to the runtime, then the WebRTC track. The guard is the
// static_assert on the layout plus the exact pins of webrtc-sys and
// libwebrtc in Cargo.toml. Before the track is used, kind() and then id()
// (both virtual calls on the WebRTC track: nothing about the livekit
// object can be checked without going through that pointer) must match
// what Rust read through the crate; a null livekit object or track is
// refused first.
//
// Thread: Rust calls set_track_gain from a tokio worker, not WebRTC's
// signaling thread. SetVolume and SetAudioProcessingOptions are only
// correct there on the release prebuilt, where RTC_DCHECK(signaling
// thread) compiles away; a debug WebRTC build would trip it.

#include <cstddef>
#include <memory>
#include <string_view>

#include "api/audio/audio_processing_options_result.h"
#include "api/audio_options.h"
#include "api/media_stream_interface.h"
#include "api/scoped_refptr.h"

namespace {

struct LivekitTrack {
  std::shared_ptr<void> runtime;
  webrtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track;
};
static_assert(sizeof(LivekitTrack) == 3 * sizeof(void*),
              "livekit_ffi::MediaStreamTrack layout changed");

webrtc::AudioTrackInterface* audio_track(const void* livekit_track,
                                         const char* id,
                                         std::size_t id_len) {
  if (livekit_track == nullptr) {
    return nullptr;
  }
  webrtc::MediaStreamTrackInterface* track =
      static_cast<const LivekitTrack*>(livekit_track)->track.get();
  if (track == nullptr ||
      track->kind() != webrtc::MediaStreamTrackInterface::kAudioKind ||
      track->id() != std::string_view(id, id_len)) {
    return nullptr;
  }
  return static_cast<webrtc::AudioTrackInterface*>(track);
}

}  // namespace

// 1 if the volume was set (0..10, 1 is as received), 0 if the track did
// not match.
extern "C" int yap_audio_track_set_volume(const void* livekit_track,
                                          const char* id,
                                          std::size_t id_len,
                                          double volume) {
  webrtc::AudioTrackInterface* track = audio_track(livekit_track, id, id_len);
  if (track == nullptr) {
    return 0;
  }
  track->SetVolume(volume);
  return 1;
}

// The AudioProcessingOptionsResultCode (0 applied, 1 stored, more is a
// refusal), or -1 if the track did not match.
extern "C" int yap_audio_track_set_processing(const void* livekit_track,
                                              const char* id,
                                              std::size_t id_len,
                                              bool echo_cancellation,
                                              bool noise_suppression,
                                              bool auto_gain_control) {
  webrtc::AudioTrackInterface* track = audio_track(livekit_track, id, id_len);
  if (track == nullptr) {
    return -1;
  }
  webrtc::AudioOptions options;
  options.echo_cancellation = echo_cancellation;
  options.noise_suppression = noise_suppression;
  options.auto_gain_control = auto_gain_control;
  return static_cast<int>(track->SetAudioProcessingOptions(options).code);
}
