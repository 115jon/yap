import { formatUploadLimit, UPLOAD_LIMITS } from "@/lib/upload-limits";

export const CAMERA_BACKGROUND_UPLOAD_LIMIT_BYTES =
  UPLOAD_LIMITS.cameraBackground;

const SUPPORTED_BACKGROUND_MIME_TYPES = new Set([
  "image/gif",
  "image/webp",
  "image/png",
  "image/jpeg",
  "image/avif",
]);

const SUPPORTED_BACKGROUND_EXTENSIONS = new Set([
  "gif",
  "webp",
  "png",
  "jpg",
  "jpeg",
  "avif",
]);

export const CAMERA_BACKGROUND_ACCEPT = [
  ...SUPPORTED_BACKGROUND_MIME_TYPES,
].join(",");

function isSupportedCameraBackgroundMimeType(
  type: string | undefined | null,
): boolean {
  return SUPPORTED_BACKGROUND_MIME_TYPES.has(
    (type ?? "").toLowerCase().split(";")[0].trim(),
  );
}

function getFileExtension(name: string): string {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

export function getCameraBackgroundValidationError(
  file: Pick<File, "name" | "size" | "type">,
): string | null {
  if (file.size > CAMERA_BACKGROUND_UPLOAD_LIMIT_BYTES) {
    return `Images must be ${formatUploadLimit(CAMERA_BACKGROUND_UPLOAD_LIMIT_BYTES)} or smaller.`;
  }

  const normalizedType = file.type.toLowerCase().split(";")[0].trim();
  if (isSupportedCameraBackgroundMimeType(normalizedType)) return null;

  const canTrustExtension =
    normalizedType === "" || normalizedType === "application/octet-stream";
  if (
    canTrustExtension &&
    SUPPORTED_BACKGROUND_EXTENSIONS.has(getFileExtension(file.name))
  )
    return null;

  return "Choose a GIF, WebP, PNG, JPEG, or AVIF image.";
}
