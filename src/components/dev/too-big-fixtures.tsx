/**
 * Dev-only fixture for the "too big" upload cards (mounted by UiFixtures as
 * `?surface=too-big`). Every situation in the toobig spec's copy table, on
 * the ground it really sits on: the drop zone over the chat column, the
 * composer's attachment tray, a picker dialog (ply-3) and a settings row
 * (ply-0). Each block carries a `data-testid` for the captures.
 */

import type { ReactNode } from "react";

import AttachmentList from "@/components/chat/AttachmentList";
import { DragDropOverlay } from "@/components/chat/DragDropOverlay";
import type {
  OversizedBatch,
  PendingUpload,
  UploadedFile,
} from "@/components/chat/useMessageInput";
import { TooBigCard } from "@/components/ui/too-big-card";
import { YappyPose } from "@/components/ui/yappy-pose";
import type { TooBigSituation } from "@/lib/too-big-copy";
import { MB, UPLOAD_LIMITS } from "@/lib/upload-limits";

const noop = () => {};

const size = (mb: number) => Math.round(mb * MB);

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
    <section data-testid={`tb-${id}`} className="flex flex-col gap-3 p-4">
      <h2 className="silkscreen text-ink-3">{title}</h2>
      {children}
    </section>
  );
}

/** A slice of the chat column with the real drop overlay over it. */
function DropZone({ oversize }: { oversize: TooBigSituation }) {
  return (
    <div className="relative h-[300px] overflow-hidden rounded-sm border border-line bg-ply-0">
      <div className="flex flex-col gap-3 p-4 opacity-35" aria-hidden="true">
        {[70, 55].map((w) => (
          <div key={w} className="flex gap-2.5">
            <div className="size-8 rounded-avatar bg-ply-3" />
            <div className="flex-1">
              <div className="my-1 h-2.5 w-[30%] rounded-xs bg-ply-3" />
              <div
                className="my-1 h-2.5 rounded-xs bg-ply-3"
                style={{ width: `${w}%` }}
              />
            </div>
          </div>
        ))}
      </div>
      <DragDropOverlay isDragging oversize={oversize} />
    </div>
  );
}

const SMALL_FILES: UploadedFile[] = [
  {
    id: "f-notes",
    url: "/fixture/notes.pdf",
    filename: "raid-notes.pdf",
    content_type: "application/pdf",
    size: size(0.4),
  },
  {
    id: "f-map",
    url: "/fixture/map.zip",
    filename: "route-map.zip",
    content_type: "application/zip",
    size: size(2.3),
  },
];

/** The composer plate with the real attachment tray in it. */
function Composer({
  oversized = null,
  uploadedFiles = [],
  pendingUploads = [],
}: {
  oversized?: OversizedBatch | null;
  uploadedFiles?: UploadedFile[];
  pendingUploads?: PendingUpload[];
}) {
  return (
    <div className="max-w-[720px] rounded-sm border border-line-strong bg-ply-2">
      <AttachmentList
        uploadedFiles={uploadedFiles}
        pendingUploads={pendingUploads}
        oversized={oversized}
        onRemove={noop}
        onToggleSensitive={noop}
        onCancel={noop}
        onRetry={noop}
        onRemoveOversized={noop}
        onClearOversized={noop}
        onReplaceOversized={noop}
      />
      <div className="px-4 py-3 text-ui-base text-ink-3">Message #general</div>
    </div>
  );
}

/** A picker dialog's plate (ply-3) with its title, as in "Choose an image". */
function DialogPlate({
  title,
  sub,
  children,
}: {
  title: string;
  sub?: string;
  children: ReactNode;
}) {
  return (
    <div className="w-full max-w-[480px] rounded-sm border border-line-strong bg-ply-3 p-6 shadow-modal">
      <p className="font-display text-ui-2xl font-extrabold text-ink-1">
        {title}
      </p>
      {sub ? <p className="mt-1 text-ui-base text-ink-2">{sub}</p> : null}
      <div className="mt-4">{children}</div>
    </div>
  );
}

const PICKERS: {
  id: string;
  title: string;
  sub?: string;
  situation: TooBigSituation;
}[] = [
  {
    id: "avatar-gif",
    title: "Choose an image",
    sub: "Upload a new avatar or reuse one of your recent uploads.",
    situation: {
      kind: "avatar",
      size: size(20.4),
      limit: UPLOAD_LIMITS.avatar,
      isGif: true,
    },
  },
  {
    id: "avatar-image",
    title: "Choose an image",
    sub: "Upload a new avatar or reuse one of your recent uploads.",
    situation: {
      kind: "avatar",
      size: size(9.6),
      limit: UPLOAD_LIMITS.avatar,
      isGif: false,
    },
  },
  {
    id: "banner",
    title: "Select a banner",
    sub: "Choose a new banner image or GIF for your profile.",
    situation: { kind: "banner", size: size(12), limit: UPLOAD_LIMITS.banner },
  },
  {
    id: "soundboard",
    title: "Upload a sound",
    situation: {
      kind: "soundboard",
      size: size(63),
      limit: UPLOAD_LIMITS.soundboard,
    },
  },
  {
    id: "gif",
    title: "GIFs",
    situation: { kind: "gif", size: size(31.7), limit: UPLOAD_LIMITS.gif },
  },
  {
    id: "vibe",
    title: "Channel vibe",
    sub: "Pick something and it will be set right away.",
    situation: { kind: "vibe", size: size(40.2), limit: UPLOAD_LIMITS.vibe },
  },
  {
    id: "server-picker",
    title: "Choose an image",
    sub: "Upload a new avatar or reuse one of your recent uploads.",
    situation: {
      kind: "server",
      serverLimit: size(6),
      action: "pick-another",
    },
  },
];

export function TooBigFixture() {
  return (
    <div
      data-testid="too-big"
      className="h-full overflow-y-auto bg-ply-0 text-ink-1"
    >
      <div className="mx-auto flex max-w-[1100px] flex-col gap-2 py-6">
        <Section id="poses" title="Yappy · stuffed · sad · squashed">
          <div className="flex flex-wrap items-end gap-8 rounded-sm border border-line bg-ply-0 p-6">
            <YappyPose pose="stuffed" cell={5} />
            <YappyPose pose="sad" cell={5} />
            <YappyPose pose="squashed" cell={5} />
          </div>
        </Section>

        <Section id="drag-one" title="Composer drag-over · one file over">
          <DropZone
            oversize={{
              kind: "drag-one",
              name: "raid-clip.mp4",
              size: size(31.2),
              limit: UPLOAD_LIMITS.attachment,
            }}
          />
        </Section>

        <Section id="drag-some" title="Composer drag-over · several, some over">
          <DropZone
            oversize={{
              kind: "drag-some",
              over: 3,
              fits: 2,
              limit: UPLOAD_LIMITS.attachment,
            }}
          />
        </Section>

        <Section id="tray-one" title="Composer tray · one file over">
          <Composer
            uploadedFiles={SMALL_FILES}
            oversized={{
              files: [{ id: "o-1", name: "raid-clip.mp4", size: size(31.2) }],
              fits: 2,
            }}
          />
        </Section>

        <Section id="tray-some" title="Composer tray · several over">
          <Composer
            uploadedFiles={SMALL_FILES}
            oversized={{
              files: [
                { id: "o-1", name: "raid-clip.mp4", size: size(31.2) },
                { id: "o-2", name: "boss-fight.mkv", size: size(88.1) },
                { id: "o-3", name: "replay.dem", size: size(26) },
              ],
              fits: 2,
            }}
          />
        </Section>

        <Section id="server-tray" title="Composer tray · the server said no">
          <Composer
            pendingUploads={[
              {
                tempId: "p-1",
                file: new File([], "patch-notes.mov"),
                progress: 0,
                abortController: new AbortController(),
                error: "Unable to upload that file.",
                serverLimit: size(20),
              },
            ]}
          />
        </Section>

        <div className="grid grid-cols-1 gap-2 lg:grid-cols-2">
          {PICKERS.map(({ id, title, sub, situation }) => (
            <Section key={id} id={id} title={id.replace("-", " · ")}>
              <DialogPlate title={title} sub={sub}>
                <TooBigCard
                  variant="inline"
                  situation={situation}
                  onAction={noop}
                />
              </DialogPlate>
            </Section>
          ))}
        </div>

        <Section id="camera-background" title="Settings · camera background">
          <div className="max-w-[640px] bg-ply-0">
            <p className="font-display text-ui-base font-bold text-ink-1">
              Background
            </p>
            <TooBigCard
              variant="inline"
              className="mt-3"
              situation={{
                kind: "camera-background",
                size: size(27.9),
                limit: UPLOAD_LIMITS.cameraBackground,
              }}
              onAction={noop}
            />
          </div>
        </Section>
      </div>
    </div>
  );
}
