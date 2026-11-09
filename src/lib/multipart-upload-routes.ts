// Production wiring for the chunked-upload handlers: Kova auth, channel
// membership, server permissions, the global rate limiter and the bound
// D1/R2. The handlers themselves live in services/multipart-upload.service.

import { getBucket, getDB, requireAuth } from "@/lib/api-helpers";
import { checkRateLimitDO, RATE_LIMITS } from "@/lib/rate-limit";
import { requireChannelAccess } from "@/lib/require-channel-access";
import { getUserPermissions } from "@/lib/require-permission";
import {
  attachmentLimitForUser,
  createMultipartUploadHandlers,
  type MultipartRateLimitAction,
  type MultipartRouteParams,
} from "@/services/multipart-upload.service";

const RATE_LIMIT_BY_ACTION = {
  // Shares the single-request route's bucket: one file is one upload.
  create: ["file-upload", RATE_LIMITS.FILE_UPLOAD],
  part: ["upload-part", RATE_LIMITS.UPLOAD_PART],
  control: ["upload-control", RATE_LIMITS.UPLOAD_CONTROL],
} as const satisfies Record<
  MultipartRateLimitAction,
  readonly [string, { limit: number; windowMs: number }]
>;

function handlers() {
  const db = getDB() as unknown as D1Database;
  return createMultipartUploadHandlers({
    db,
    bucket: getBucket() as unknown as R2Bucket,
    authenticate: (request) => requireAuth(request),
    channelAccess: requireChannelAccess,
    serverPermissions: getUserPermissions,
    rateLimit: (userId, action) => {
      const [name, options] = RATE_LIMIT_BY_ACTION[action];
      return checkRateLimitDO(userId, name, options);
    },
    // The uploader's plan as stored, read on create and again on complete;
    // never anything the client sends.
    attachmentLimit: (userId) => attachmentLimitForUser(db, userId),
  });
}

type Handler = keyof ReturnType<typeof handlers>;

/** A TanStack server handler for one chunked-upload action. */
export function multipartRoute(action: Handler) {
  return ({
    request,
    params,
  }: {
    request: Request;
    params: { id: string; uploadId?: string; partNumber?: string };
  }) => {
    const routeParams: MultipartRouteParams = {
      channelId: params.id,
      uploadId: params.uploadId,
      partNumber: params.partNumber,
    };
    return handlers()[action](request, routeParams);
  };
}
