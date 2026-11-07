// A captured screen frame (BGRA) to I420 at the size it is sent at, with
// the libyuv that ships inside libwebrtc. Called from src/share.rs, which
// checks the source rows first.

#include <cstdint>
#include <vector>

#include "third_party/libyuv/include/libyuv/convert_from_argb.h"
#include "third_party/libyuv/include/libyuv/scale.h"

// Converts at the capture's size, then (when the share's box is smaller)
// box-filters the planes down: scaling I420 reads 1.5 bytes a pixel where
// scaling the BGRA first would read 4. Returns 0 on success.
extern "C" int yap_screen_to_i420(const uint8_t* bgra,
                                  int stride,
                                  int width,
                                  int height,
                                  uint8_t* dst_y,
                                  int stride_y,
                                  uint8_t* dst_u,
                                  int stride_u,
                                  uint8_t* dst_v,
                                  int stride_v,
                                  int dst_width,
                                  int dst_height)
{
  if (dst_width == width && dst_height == height) {
    return libyuv::ARGBToI420(bgra, stride, dst_y, stride_y, dst_u, stride_u, dst_v, stride_v,
                              width, height);
  }
  // One scratch frame per capture thread, reused while the size holds.
  thread_local std::vector<uint8_t> scratch;
  const int cw = (width + 1) / 2;
  const int ch = (height + 1) / 2;
  const size_t need = static_cast<size_t>(width) * height + 2 * static_cast<size_t>(cw) * ch;
  if (scratch.size() < need) {
    scratch.resize(need);
  }
  uint8_t* y = scratch.data();
  uint8_t* u = y + static_cast<size_t>(width) * height;
  uint8_t* v = u + static_cast<size_t>(cw) * ch;
  const int converted =
    libyuv::ARGBToI420(bgra, stride, y, width, u, cw, v, cw, width, height);
  if (converted != 0) {
    return converted;
  }
  return libyuv::I420Scale(y, width, u, cw, v, cw, width, height, dst_y, stride_y, dst_u,
                           stride_u, dst_v, stride_v, dst_width, dst_height,
                           libyuv::kFilterBox);
}

#include "third_party/libyuv/include/libyuv/convert_from.h"

// The share's I420 frame as NV12 (Y, then interleaved U/V), the layout the
// Media Foundation H.264 encoders take. Returns 0 on success.
extern "C" int yap_i420_to_nv12(const uint8_t* src_y,
                                int stride_y,
                                const uint8_t* src_u,
                                int stride_u,
                                const uint8_t* src_v,
                                int stride_v,
                                uint8_t* dst_y,
                                int dst_stride_y,
                                uint8_t* dst_uv,
                                int dst_stride_uv,
                                int width,
                                int height)
{
  return libyuv::I420ToNV12(src_y, stride_y, src_u, stride_u, src_v, stride_v, dst_y, dst_stride_y,
                            dst_uv, dst_stride_uv, width, height);
}
