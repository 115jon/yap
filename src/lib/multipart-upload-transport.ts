import { apiDelete, apiGet, apiPost, apiSendBlob } from "@/lib/api-client";
import type {
  MultipartSession,
  MultipartStatus,
  MultipartTransport,
  UploadedAttachment,
} from "@/lib/multipart-upload";

/** The chunked-upload routes for one channel, over the app's API client. */
export function createApiMultipartTransport(
  channelId: string,
): MultipartTransport {
  const base = `/api/channels/${encodeURIComponent(channelId)}/uploads`;
  const one = (uploadId: string) => `${base}/${encodeURIComponent(uploadId)}`;
  return {
    create: (input, signal) =>
      apiPost<MultipartSession>(base, input, { signal }),
    status: (uploadId, signal) =>
      apiGet<MultipartStatus>(one(uploadId), { signal }),
    putPart: async (uploadId, partNumber, body, options) => {
      await apiSendBlob(`${one(uploadId)}/parts/${partNumber}`, body, options);
    },
    complete: (uploadId, signal) =>
      apiPost<UploadedAttachment>(`${one(uploadId)}/complete`, {}, { signal }),
    abort: async (uploadId) => {
      await apiDelete(one(uploadId));
    },
  };
}
