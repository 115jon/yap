// A captured camera frame to I420 in one pass, with the libyuv that ships
// inside libwebrtc (the crate's yuv_helper lacks the packed 4:2:2 formats).
// Called from src/camera.rs, which checks every plane's size first.

#include <cstdint>

#include "third_party/libyuv/include/libyuv/convert.h"
#include "third_party/libyuv/include/libyuv/convert_from_argb.h"
#include "third_party/libyuv/include/libyuv/planar_functions.h"

// format: 0 I420, 1 NV12, 2 NV21, 3 YUY2, 4 UYVY, 5 BGRA (libyuv "ARGB"),
// 6 RGBA (libyuv "ABGR"). Returns 0 on success.
extern "C" int yap_camera_to_i420(int format,
                                  const uint8_t* src0,
                                  int stride0,
                                  const uint8_t* src1,
                                  int stride1,
                                  const uint8_t* src2,
                                  int stride2,
                                  int width,
                                  int height,
                                  uint8_t* dst_y,
                                  int stride_y,
                                  uint8_t* dst_u,
                                  int stride_u,
                                  uint8_t* dst_v,
                                  int stride_v)
{
  switch (format) {
    case 0:
      return libyuv::I420Copy(src0, stride0, src1, stride1, src2, stride2, dst_y, stride_y, dst_u,
                              stride_u, dst_v, stride_v, width, height);
    case 1:
      return libyuv::NV12ToI420(src0, stride0, src1, stride1, dst_y, stride_y, dst_u, stride_u,
                                dst_v, stride_v, width, height);
    case 2:
      return libyuv::NV21ToI420(src0, stride0, src1, stride1, dst_y, stride_y, dst_u, stride_u,
                                dst_v, stride_v, width, height);
    case 3:
      return libyuv::YUY2ToI420(src0, stride0, dst_y, stride_y, dst_u, stride_u, dst_v, stride_v,
                                width, height);
    case 4:
      return libyuv::UYVYToI420(src0, stride0, dst_y, stride_y, dst_u, stride_u, dst_v, stride_v,
                                width, height);
    case 5:
      return libyuv::ARGBToI420(src0, stride0, dst_y, stride_y, dst_u, stride_u, dst_v, stride_v,
                                width, height);
    case 6:
      return libyuv::ABGRToI420(src0, stride0, dst_y, stride_y, dst_u, stride_u, dst_v, stride_v,
                                width, height);
    default:
      return -1;
  }
}
