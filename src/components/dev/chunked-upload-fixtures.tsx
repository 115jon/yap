/**
 * Dev-only fixture for chunked (multipart) attachment uploads, mounted by
 * UiFixtures as `?surface=chunked-upload`. The top block is the composer
 * tray at fixed points of a 150 MB upload; the bottom one runs the real
 * `uploadInParts` engine against an in-memory transport (no network), so
 * the progress tile moves and its cancel really aborts. Blocks carry
 * `data-testid`s for the captures.
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";

import AttachmentList from "@/components/chat/AttachmentList";
import type { PendingUpload } from "@/components/chat/useMessageInput";
import { Button } from "@/components/ui/button";
import {
  memoryResumeStore,
  uploadInParts,
  type MultipartTransport,
} from "@/lib/multipart-upload";
import { MB, MULTIPART_PART_SIZE } from "@/lib/upload-limits";

const noop = () => {};

/** A File-shaped stand-in: the tray reads name, size and type only. */
function fakeFile(name: string, size: number, type: string): File {
  return {
    name,
    size,
    type,
    lastModified: 1,
    slice: () => new Blob(),
  } as unknown as File;
}

const VOD = fakeFile("raid-vod.mp4", 150 * MB, "video/mp4");

function pending(
  tempId: string,
  file: File,
  loaded: number | undefined,
): PendingUpload {
  return {
    tempId,
    file,
    loaded,
    progress: loaded === undefined ? 0 : loaded / file.size,
    abortController: new AbortController(),
  };
}

function Section({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section data-testid={`cu-${id}`} className="flex flex-col gap-3 p-4">
      <h2 className="silkscreen text-ink-3">{title}</h2>
      {children}
    </section>
  );
}

function Composer({
  pendingUploads,
  onCancel = noop,
}: {
  pendingUploads: PendingUpload[];
  onCancel?: (tempId: string) => void;
}) {
  return (
    <div className="max-w-[720px] rounded-sm border border-line-strong bg-ply-2">
      <AttachmentList
        uploadedFiles={[]}
        pendingUploads={pendingUploads}
        onRemove={noop}
        onToggleSensitive={noop}
        onCancel={onCancel}
        onRetry={noop}
      />
      <div className="px-4 py-3 text-ui-base text-ink-3">Message #general</div>
    </div>
  );
}

/** Each part "uploads" over `partMs`, reporting progress as it goes. */
function fakeTransport(
  partMs: number,
  onAbortCall: () => void,
): MultipartTransport {
  return {
    create: async ({ size }) => ({
      upload_id: "fixture-upload",
      part_size: MULTIPART_PART_SIZE,
      part_count: Math.ceil(size / MULTIPART_PART_SIZE),
      size,
      expires_at: new Date(Date.now() + 86_400_000).toISOString(),
    }),
    status: async () => {
      throw Object.assign(new Error("gone"), { status: 404 });
    },
    putPart: (_id, partNumber, _body, { signal, onProgress }) =>
      new Promise<void>((resolve, reject) => {
        const bytes = Math.min(
          MULTIPART_PART_SIZE,
          VOD.size - (partNumber - 1) * MULTIPART_PART_SIZE,
        );
        const steps = 8;
        let step = 0;
        const timer = setInterval(() => {
          step += 1;
          onProgress?.(Math.round((bytes * step) / steps));
          if (step === steps) {
            clearInterval(timer);
            resolve();
          }
        }, partMs / steps);
        signal?.addEventListener(
          "abort",
          () => {
            clearInterval(timer);
            reject(new DOMException("cancelled", "AbortError"));
          },
          { once: true },
        );
      }),
    complete: async () => ({
      id: "fixture-attachment",
      file_url: "/fixture/raid-vod.mp4",
      file_name: VOD.name,
      file_size: VOD.size,
      content_type: VOD.type,
    }),
    abort: async () => onAbortCall(),
  };
}

type LiveState = "uploading" | "cancelled" | "done";

/** The real engine and tray, against the fake transport. */
function LiveUpload() {
  // Server render has no URL; the run only starts in the effect anyway.
  const partMs =
    typeof window === "undefined"
      ? 1500
      : Number(
          new URLSearchParams(window.location.search).get("partMs") ?? 1500,
        );
  const [upload, setUpload] = useState<PendingUpload | null>(null);
  const [state, setState] = useState<LiveState>("uploading");
  const [serverAborted, setServerAborted] = useState(false);
  const runRef = useRef(0);

  const start = useCallback(() => {
    const run = ++runRef.current;
    const controller = new AbortController();
    setServerAborted(false);
    setState("uploading");
    setUpload({ ...pending("live", VOD, 0), abortController: controller });
    uploadInParts(VOD, {
      transport: fakeTransport(partMs, () => setServerAborted(true)),
      resumeKey: "fixture",
      store: memoryResumeStore(),
      signal: controller.signal,
      onProgress: ({ loaded, total }) => {
        if (run !== runRef.current) return;
        setUpload((prev) =>
          prev ? { ...prev, loaded, progress: loaded / total } : prev,
        );
      },
    }).then(
      () => {
        if (run !== runRef.current) return;
        setUpload(null);
        setState("done");
      },
      () => {
        if (run !== runRef.current) return;
        setUpload(null);
        setState("cancelled");
      },
    );
  }, [partMs]);

  // Kick off from a timer, not the effect body, so the first state update
  // isn't a synchronous cascade.
  useEffect(() => {
    const timer = setTimeout(start, 0);
    return () => {
      clearTimeout(timer);
      runRef.current += 1;
    };
  }, [start]);

  const cancel = (tempId: string) => {
    if (upload?.tempId === tempId) upload.abortController.abort();
  };

  return (
    <div className="flex flex-col gap-3">
      <Composer pendingUploads={upload ? [upload] : []} onCancel={cancel} />
      <div className="flex items-center gap-3 text-ui-sm text-ink-2">
        <span data-testid="cu-live-status" data-state={state}>
          {state === "uploading"
            ? `Uploading: ${Math.round((upload?.progress ?? 0) * 100)}%`
            : state === "done"
              ? "Uploaded: the tile became an attachment"
              : `Cancelled${serverAborted ? ": server upload aborted" : ""}`}
        </span>
        <Button type="button" size="xs" variant="secondary" onClick={start}>
          Restart
        </Button>
      </div>
    </div>
  );
}

export function ChunkedUploadFixture() {
  return (
    <div
      data-testid="chunked-upload"
      className="h-full overflow-y-auto bg-ply-0 text-ink-1"
    >
      <div className="mx-auto flex max-w-[1100px] flex-col gap-2 py-6">
        <Section id="static" title="Composer tray · 150 MB in parts">
          <Composer
            pendingUploads={[
              pending("p-0", VOD, 0),
              pending(
                "p-38",
                fakeFile("boss-fight.mkv", 150 * MB, "video/x-matroska"),
                57 * MB,
              ),
              pending("p-97", fakeFile("replay.dem", 40 * MB, ""), 38.8 * MB),
              pending(
                "p-small",
                fakeFile("notes.pdf", 2 * MB, "application/pdf"),
                undefined,
              ),
            ]}
          />
        </Section>
        <Section id="live" title="Live · real engine, fake transport">
          <LiveUpload />
        </Section>
      </div>
    </div>
  );
}
