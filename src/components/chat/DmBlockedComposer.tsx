import { useState } from "react";

import { Button } from "@/components/ui/button";
import { apiDelete } from "@/lib/api-client";
import { describeApiError } from "@/lib/api-errors";
import { DM_BLOCK_COPY, type BlockState } from "@/lib/blocks";
import { useChatActions } from "@/stores/chat-store";
import { useDmBlockStore } from "@/stores/useDmBlockStore";
import { ComposerReadOnly } from "./MessageInput";

/** The read-only DM composer for a block, with Unblock on your own. */
export function DmBlockedComposer({
  block,
  channelId,
  recipientId,
}: {
  block: BlockState;
  channelId: string;
  recipientId: string | null;
}) {
  const { loadRelationships } = useChatActions();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const unblock = async () => {
    if (!recipientId) return;
    setBusy(true);
    setError(null);
    try {
      await apiDelete("/api/friends", { target_user_id: recipientId });
      useDmBlockStore.getState().forget(channelId);
      await loadRelationships();
    } catch (cause) {
      setError(describeApiError(cause, "unblock this user"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div data-testid="dm-blocked-composer" data-block={block}>
      {error ? (
        <p role="alert" className="px-4 pb-1 text-ui-sm text-danger-ink">
          {error}
        </p>
      ) : null}
      <ComposerReadOnly
        message={DM_BLOCK_COPY[block]}
        action={
          block === "you_blocked" && recipientId ? (
            <Button
              variant="secondary"
              size="sm"
              loading={busy}
              disabled={busy}
              onClick={() => void unblock()}
            >
              Unblock
            </Button>
          ) : undefined
        }
      />
    </div>
  );
}
