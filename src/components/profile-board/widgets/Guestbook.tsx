/**
 * Guestbook: the latest notes, and a one-line field to leave one when the
 * server says the viewer may (`can_post`). Posting is optimistic: the note
 * shows at once, is swapped for the server's copy, and comes back out with
 * the server's own words if it fails (the daily limit included). The owner
 * can delete any note and block its author; an author can delete their own.
 */

import { useId, useState, type FormEvent } from "react";

import { Avatar } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { IconButton } from "@/components/ui/IconButton";
import { Num } from "@/components/ui/num";
import {
  blockFromGuestbook,
  deleteGuestbookNote,
  postGuestbookNote,
} from "@/lib/profile-board-api";
import {
  cleanGuestbookBody,
  GUESTBOOK_PREVIEW,
  TEXT_LIMITS,
  textLength,
  type BoardData,
  type BoardUserRef,
  type GuestbookNote,
} from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import { formatAge, guestbookDenialCopy, userName } from "../board-format";
import { useBoardMedia } from "../board-context";
import { Plate, WidgetEmpty, type WidgetProps } from "./shared";

const PENDING = "pending-";

function errorMessage(error: unknown, fallback: string): string {
  const message = (error as { message?: unknown } | null)?.message;
  return typeof message === "string" && message && !/^HTTP Error/.test(message)
    ? message
    : fallback;
}

/**
 * The owner's guestbook before the board's data has it (the plate was just
 * added in the editor, or saved a moment ago and the data is on its way):
 * an open, empty book, never "unavailable".
 */
const OWNER_PREVIEW: NonNullable<BoardData["guestbook"]> = {
  enabled: true,
  notes: [],
  total: 0,
  can_post: false,
  denial: "GUESTBOOK_OWN",
};

export function GuestbookWidget({
  widget,
  data,
  ctx,
}: WidgetProps<"guestbook">) {
  const book = data.guestbook ?? (ctx.isOwner ? OWNER_PREVIEW : undefined);
  const [added, setAdded] = useState<GuestbookNote[]>([]);
  const [removed, setRemoved] = useState<ReadonlySet<string>>(() => new Set());
  const [blocking, setBlocking] = useState<BoardUserRef | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const now = ctx.at;

  const serverNotes = book?.notes ?? [];
  const serverIds = new Set(serverNotes.map((note) => note.id));
  const notes = [
    ...added.filter((note) => !serverIds.has(note.id)),
    ...serverNotes,
  ].filter((note) => !removed.has(note.id));
  const total = Math.max(
    0,
    (book?.total ?? 0) +
      added.filter(
        (note) => !serverIds.has(note.id) && !note.id.startsWith(PENDING),
      ).length -
      [...removed].filter((id) => serverIds.has(id)).length,
  );
  const readout = (
    <Num className="text-ink-3">
      {total} {total === 1 ? "note" : "notes"}
    </Num>
  );

  if (!book) {
    // Only a board fetched without the guestbook's data gets here (a
    // response from before the plate existed); reopening refetches it.
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="rules"
          title="Couldn't load the guestbook"
          hint="Close and reopen the profile to try again."
        />
      </Plate>
    );
  }

  const remove = async (note: GuestbookNote) => {
    setError(null);
    setNotice(null);
    setRemoved((prev) => new Set(prev).add(note.id));
    try {
      await deleteGuestbookNote(ctx.ownerId, note.id);
    } catch (cause) {
      setRemoved((prev) => {
        const next = new Set(prev);
        next.delete(note.id);
        return next;
      });
      setError(errorMessage(cause, "Couldn't delete that note. Try again."));
    }
  };

  const block = async (author: BoardUserRef) => {
    setBlocking(null);
    setError(null);
    const theirs = notes.filter((note) => note.author.id === author.id);
    setRemoved((prev) => {
      const next = new Set(prev);
      for (const note of theirs) next.add(note.id);
      return next;
    });
    try {
      await blockFromGuestbook(author.id);
      setNotice(`${userName(author)} can't sign your guestbook any more.`);
    } catch (cause) {
      setRemoved((prev) => {
        const next = new Set(prev);
        for (const note of theirs) next.delete(note.id);
        return next;
      });
      setError(
        errorMessage(cause, `Couldn't block ${userName(author)}. Try again.`),
      );
    }
  };

  const post = async (body: string): Promise<boolean> => {
    setError(null);
    setNotice(null);
    const tempId = `${PENDING}${Date.now()}`;
    const viewer = ctx.viewer;
    if (viewer) {
      setAdded((prev) => [
        {
          id: tempId,
          author: viewer,
          body,
          created_at: new Date(now).toISOString(),
        },
        ...prev,
      ]);
    }
    try {
      const saved = await postGuestbookNote(ctx.ownerId, body);
      setAdded((prev) => [saved, ...prev.filter((note) => note.id !== tempId)]);
      return true;
    } catch (cause) {
      setAdded((prev) => prev.filter((note) => note.id !== tempId));
      setError(errorMessage(cause, "Couldn't post your note. Try again."));
      return false;
    }
  };

  const canPost = book.enabled && book.can_post && !ctx.isOwner;
  const wide = widget.size === "L";
  const shown = notes.slice(0, wide ? 2 : GUESTBOOK_PREVIEW);

  const list =
    shown.length > 0 ? (
      <ul
        aria-label="Notes"
        className={cn(
          "flex min-w-0 flex-col",
          wide
            ? "gap-2.5 @max-[480px]/cell:[&>li:nth-child(n+2)]:hidden"
            : "gap-3",
        )}
      >
        {shown.map((note) => (
          <NoteRow
            key={note.id}
            note={note}
            now={now}
            canDelete={
              !note.id.startsWith(PENDING) &&
              (ctx.isOwner ||
                (!!ctx.viewer && ctx.viewer.id === note.author.id))
            }
            canBlock={ctx.isOwner && note.author.id !== ctx.ownerId}
            onDelete={() => void remove(note)}
            onBlock={() => setBlocking(note.author)}
          />
        ))}
      </ul>
    ) : (
      <WidgetEmpty
        icon="rules"
        title="No notes yet"
        hint={
          canPost
            ? "Be the first to leave one."
            : ctx.isOwner && book.enabled
              ? "Friends who share a server or a DM with you can sign it."
              : undefined
        }
      />
    );

  const status = blocking ? (
    <div
      role="group"
      aria-label={`Block ${userName(blocking)}`}
      className="flex min-w-0 flex-col gap-2 border-t border-line pt-2.5"
    >
      <p className="text-ui-xs text-pretty text-ink-2">
        Block {userName(blocking)}? Their notes here are deleted and they can't
        sign again.
      </p>
      <div className="flex items-center gap-2">
        <Button size="xs" variant="ghost" onClick={() => setBlocking(null)}>
          Cancel
        </Button>
        <Button
          size="xs"
          variant="destructive"
          onClick={() => void block(blocking)}
        >
          Block
        </Button>
      </div>
    </div>
  ) : error ? (
    <p role="alert" className="text-ui-xs text-pretty text-danger-ink">
      {error}
    </p>
  ) : notice ? (
    <p role="status" className="text-ui-xs text-pretty text-ink-2">
      {notice}
    </p>
  ) : null;

  const foot = !book.enabled ? (
    <p data-slot="guestbook-note" className="text-ui-xs text-pretty text-ink-3">
      {ctx.isOwner
        ? "Your guestbook is closed: nobody can sign it."
        : guestbookDenialCopy("GUESTBOOK_OFF", ctx.ownerName)}
    </p>
  ) : canPost ? (
    <NoteField
      ownerName={ctx.ownerName}
      onPost={post}
      onTyping={() => setError(null)}
    />
  ) : !ctx.isOwner ? (
    <p data-slot="guestbook-note" className="text-ui-xs text-pretty text-ink-3">
      {ctx.viewer
        ? guestbookDenialCopy(book.denial, ctx.ownerName)
        : `Sign in to leave ${ctx.ownerName} a note.`}
    </p>
  ) : null;

  if (wide) {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(0,0.8fr)] gap-5 @max-[480px]/cell:grid-cols-1 @max-[480px]/cell:gap-2.5">
          <div className="min-w-0">{list}</div>
          <div className="flex min-w-0 flex-col justify-end gap-2">
            {status}
            {foot}
          </div>
        </div>
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx} readout={readout}>
      {list}
      <div className="mt-auto flex flex-col gap-2">
        {status}
        {foot}
      </div>
    </Plate>
  );
}

function NoteRow({
  note,
  now,
  canDelete,
  canBlock,
  onDelete,
  onBlock,
}: {
  note: GuestbookNote;
  now: number;
  canDelete: boolean;
  canBlock: boolean;
  onDelete(): void;
  onBlock(): void;
}) {
  const media = useBoardMedia();
  const name = userName(note.author);
  const pending = note.id.startsWith(PENDING);
  return (
    <li
      data-note={note.id}
      data-pending={pending || undefined}
      className={cn(
        "group/note relative flex min-w-0 gap-2.5",
        pending && "opacity-60",
      )}
    >
      <Avatar
        name={name}
        src={note.author.avatar_url ? media.url(note.author.avatar_url) : null}
        size={24}
        className="mt-0.5 shrink-0"
      />
      <div className="min-w-0 flex-1">
        <p className="flex min-w-0 items-baseline gap-1.5">
          <span className="truncate font-display text-[13px] leading-[18px] font-bold text-ink-1">
            {name}
          </span>
          <Num className="shrink-0 text-ink-3">
            {pending ? "now" : formatAge(note.created_at, now)}
          </Num>
        </p>
        <p className="line-clamp-2 text-ui-sm break-words text-ink-1">
          {note.body}
        </p>
      </div>
      {canDelete || canBlock ? (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <IconButton
                icon="more"
                size={24}
                aria-label={`Note from ${name}: more`}
                className="-mt-0.5 shrink-0 opacity-0 group-hover/note:opacity-100 focus-visible:opacity-100 data-[popup-open]:opacity-100 pointer-coarse:opacity-100"
              />
            }
          />
          <DropdownMenuContent align="end">
            {canDelete ? (
              <DropdownMenuItem
                variant="danger"
                icon="trash"
                onClick={onDelete}
              >
                Delete note
              </DropdownMenuItem>
            ) : null}
            {canBlock ? (
              <DropdownMenuItem icon="ban" onClick={onBlock}>
                Block from guestbook
              </DropdownMenuItem>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </li>
  );
}

function NoteField({
  ownerName,
  onPost,
  onTyping,
}: {
  ownerName: string;
  onPost(body: string): Promise<boolean>;
  onTyping(): void;
}) {
  const id = useId();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const length = textLength(value);
  const over = length > TEXT_LIMITS.guestbook;
  const body = cleanGuestbookBody(value);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!body || over || busy) return;
    setBusy(true);
    const kept = value;
    setValue("");
    const ok = await onPost(body);
    if (!ok) setValue(kept);
    setBusy(false);
  };

  return (
    <form onSubmit={submit} className="flex min-w-0 flex-col gap-1">
      <div className="flex h-9 min-w-0 items-center gap-1 rounded-[2px] border border-line-field bg-ply-2 pr-1 pl-2.5 transition-colors duration-(--dur-1) focus-within:border-ink-1 pointer-fine:hover:border-ink-2">
        <label htmlFor={id} className="sr-only">
          Leave {ownerName} a note
        </label>
        <input
          id={id}
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            onTyping();
          }}
          placeholder={`Leave ${ownerName} a note…`}
          autoComplete="off"
          aria-invalid={over || undefined}
          className="h-full min-w-0 flex-1 bg-transparent text-ui-sm text-ink-1 outline-none placeholder:text-ink-3"
        />
        {length > TEXT_LIMITS.guestbook - 40 ? (
          <Num
            className={cn("shrink-0", over ? "text-danger-ink" : "text-ink-3")}
          >
            {TEXT_LIMITS.guestbook - length}
          </Num>
        ) : null}
        <IconButton
          type="submit"
          icon="send"
          size={28}
          aria-label="Post note"
          disabled={!body || over || busy}
          className="shrink-0 disabled:opacity-45"
        />
      </div>
    </form>
  );
}
