/**
 * The console's command palette (⌘K / Ctrl+K): jump to a section or an
 * account, or run an action. Built like the app's quick switcher
 * (CommandMenu): cmdk in the C1 menu panel, no open animation (it is a
 * keyboard surface), Escape and focus return owned by the Dialog.
 */

import { Command } from "cmdk";
import { useRef, useState } from "react";

import { Avatar } from "@/components/ui/avatar";
import {
  Dialog,
  DialogOverlay,
  DialogPortal,
  DialogSurface,
} from "@/components/ui/dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { Icon } from "@/components/ui/icon";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import {
  MENU_HINT,
  MENU_ICON,
  MENU_ITEM,
  MENU_PANEL,
} from "@/components/ui/menu";
import { assetUrl } from "./admin-format";
import { cn } from "@/lib/utils";

import { ADMIN_SECTIONS, SECTION_INFO, useAdminShell } from "./AdminConsole";

const ROW = cn(
  MENU_ITEM,
  "group/item h-9 cursor-pointer py-0 text-ink-1 aria-selected:bg-tint-2",
);
const GROUP =
  "not-first:mt-1 [&_[cmdk-group-heading]]:silkscreen [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-ink-3";

export function AdminPalette({
  open,
  onOpenChange,
  onSignOut,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSignOut?: () => void;
}) {
  const { go, openPerson, people } = useAdminShell();
  const [search, setSearch] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);
  const close = () => {
    onOpenChange(false);
    setSearch("");
  };
  const everyone = people.data?.people ?? [];
  const query = search.trim();

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => (next ? onOpenChange(true) : close())}
    >
      <DialogPortal>
        <DialogOverlay className="data-[state=closed]:animate-none! data-[state=open]:animate-none!" />
        <DialogSurface
          showCloseButton={false}
          aria-label="Admin command palette"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            inputRef.current?.focus();
          }}
          className="fixed inset-x-0 top-[max(12dvh,8px)] z-50 mx-auto w-full max-w-[560px] px-3 outline-none max-md:top-2 max-md:px-2"
        >
          <Command
            label="Admin command palette"
            className={cn(MENU_PANEL, "overflow-hidden p-0 shadow-modal")}
            loop
          >
            <div className="flex h-12 items-center gap-2 px-3 shadow-[inset_0_-1px_0_var(--line)] has-[input:focus-visible]:shadow-[inset_0_-2px_0_var(--focus)] max-md:h-14">
              <Icon name="search" size="sm" className="shrink-0 text-ink-3" />
              <Command.Input
                ref={inputRef}
                value={search}
                onValueChange={setSearch}
                placeholder="Jump to a section, a person or an action"
                className="rm-search-input h-full w-full min-w-0 bg-transparent text-ui-row text-ink-1 outline-none placeholder:text-ink-3 max-md:text-ui-md"
              />
              <Kbd className="hidden shrink-0 pointer-fine:inline-flex">
                Esc
              </Kbd>
            </div>
            <Command.List className="max-h-[min(440px,calc(100dvh-160px))] scroll-py-1 overflow-y-auto overscroll-contain p-1 custom-scrollbar">
              <Command.Empty>
                <EmptyState
                  size="sm"
                  art={null}
                  title={`Nothing matches “${query}”`}
                  description="Try a name, a @username or a section."
                />
              </Command.Empty>
              <Command.Group heading="Sections" className={GROUP}>
                {ADMIN_SECTIONS.map((name, index) => (
                  <Command.Item
                    key={name}
                    value={`section ${SECTION_INFO[name].label}`}
                    onSelect={() => {
                      go(name);
                      close();
                    }}
                    className={ROW}
                  >
                    <span className={MENU_ICON}>
                      <Icon name={SECTION_INFO[name].icon} size="sm" />
                    </span>
                    <span className="min-w-0 flex-1 truncate">
                      {SECTION_INFO[name].label}
                    </span>
                    <span className={MENU_HINT}>
                      <KbdGroup>
                        <Kbd size="sm">g</Kbd>
                        <Kbd size="sm">{SECTION_INFO[name].key}</Kbd>
                      </KbdGroup>
                      <span className="sr-only">or {index + 1}</span>
                    </span>
                  </Command.Item>
                ))}
              </Command.Group>
              <Command.Group heading="Actions" className={GROUP}>
                <Command.Item
                  value="action refresh reload people"
                  onSelect={() => {
                    void people.refresh();
                    close();
                  }}
                  className={ROW}
                >
                  <span className={MENU_ICON}>
                    <Icon name="refresh" size="sm" />
                  </span>
                  Refresh the people list
                </Command.Item>
                <Command.Item
                  value="action announcement banner publish"
                  onSelect={() => {
                    go("switches");
                    close();
                  }}
                  className={ROW}
                >
                  <span className={MENU_ICON}>
                    <Icon name="announcement" size="sm" />
                  </span>
                  Write a site banner
                </Command.Item>
                <Command.Item
                  value="action kill switch pause feature uploads embeds signups listen together soundboard"
                  onSelect={() => {
                    go("switches");
                    close();
                  }}
                  className={ROW}
                >
                  <span className={MENU_ICON}>
                    <Icon name="sliders" size="sm" />
                  </span>
                  Pause a feature
                </Command.Item>
                {onSignOut ? (
                  <Command.Item
                    value="action sign out log out"
                    onSelect={() => {
                      close();
                      onSignOut();
                    }}
                    className={ROW}
                  >
                    <span className={MENU_ICON}>
                      <Icon name="logout" size="sm" />
                    </span>
                    Sign out
                  </Command.Item>
                ) : null}
              </Command.Group>
              {query ? (
                <Command.Group heading="People" className={GROUP}>
                  {everyone.slice(0, 400).map((person) => {
                    const name = person.display_name?.trim() || person.username;
                    return (
                      <Command.Item
                        key={person.id}
                        value={`person ${name} ${person.username} ${person.id}`}
                        onSelect={() => {
                          openPerson(person.id);
                          close();
                        }}
                        className={ROW}
                      >
                        <Avatar
                          src={
                            person.avatar_url
                              ? assetUrl(person.avatar_url)
                              : null
                          }
                          name={name}
                          size={20}
                          ring="ply-3"
                        />
                        <span className="min-w-0 flex-1 truncate">{name}</span>
                        <span className={MENU_HINT}>@{person.username}</span>
                      </Command.Item>
                    );
                  })}
                </Command.Group>
              ) : null}
            </Command.List>
            <div className="hidden items-center justify-between gap-4 border-t border-line px-3 py-2 text-ui-xs text-ink-3 pointer-fine:flex">
              <span className="inline-flex items-center gap-1.5">
                <Kbd>/</Kbd>
                search the section
              </span>
              <span className="flex items-center gap-3">
                <span className="inline-flex items-center gap-1.5">
                  <KbdGroup>
                    <Kbd>j</Kbd>
                    <Kbd>k</Kbd>
                  </KbdGroup>
                  rows
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <Kbd>↵</Kbd>
                  open
                </span>
              </span>
            </div>
          </Command>
        </DialogSurface>
      </DialogPortal>
    </Dialog>
  );
}
