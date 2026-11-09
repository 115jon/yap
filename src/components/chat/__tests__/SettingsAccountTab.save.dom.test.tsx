// @vitest-environment jsdom

/**
 * The profile editor's save round trip against a fake API that stores and
 * reads back the way production does (the real sanitizers from
 * update-profile and users/me), through the real chat store. After a
 * successful save the unsaved bar must go away.
 */

import { TooltipProvider } from "@/components/ui/tooltip";
import {
  normalizeAvatarDisplay,
  serializeAvatarDisplay,
  type AvatarDisplay,
} from "@/lib/avatar-display";
import {
  applyProfileThemeDefaults,
  normalizeDisplayNameStyle,
  normalizeHexColor,
  serializeDisplayNameStyle,
} from "@/lib/profile-customization";
import { useChatStore } from "@/stores/chat-store";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Row = Record<string, string | number | null>;

const server = vi.hoisted(() => ({
  row: {} as Row,
  calls: [] as Array<{ method: string; url: string; body?: unknown }>,
}));

/** update-profile's PATCH: the same normalizers, stored as D1 strings. */
function updateProfile(body: Record<string, unknown>) {
  const row = server.row;
  const color = (value: unknown) =>
    value === undefined
      ? undefined
      : value === null
        ? null
        : normalizeHexColor(value);
  const accent = color(body.profileAccentColor);
  const background = color(body.profileBackgroundColor);
  const banner = color(body.profileBannerColor);
  const resolved = applyProfileThemeDefaults({
    profile_accent_color: accent ?? null,
    profile_background_color: background ?? null,
    profile_banner_color: banner ?? null,
  });
  if (accent !== undefined)
    row.profile_accent_color = resolved.profile_accent_color;
  if (background !== undefined)
    row.profile_background_color = resolved.profile_background_color;
  if (banner !== undefined)
    row.profile_banner_color = resolved.profile_banner_color;
  if (body.displayNameStyle !== undefined) {
    row.display_name_style =
      body.displayNameStyle === null
        ? null
        : serializeDisplayNameStyle(
            normalizeDisplayNameStyle(body.displayNameStyle),
          );
  }
  const text = (value: unknown, max: number) =>
    value === null ? null : (value as string).trim().slice(0, max) || null;
  if (body.bio !== undefined) row.bio = text(body.bio, 190);
  if (body.pronouns !== undefined) row.pronouns = text(body.pronouns, 40);
  if (body.removeAvatar) {
    row.avatar_url = null;
    row.avatar_display =
      body.avatarDisplay !== undefined
        ? serializeAvatarDisplay(normalizeAvatarDisplay(body.avatarDisplay))
        : null;
  } else if (body.avatarDisplay !== undefined) {
    row.avatar_display = serializeAvatarDisplay(
      normalizeAvatarDisplay(body.avatarDisplay),
    );
  }
  if (!body.removeAvatar && body.avatarUrl !== undefined) {
    row.avatar_url = body.avatarUrl as string;
  }
  row.updated_at = new Date().toISOString();
  return { ok: true };
}

/** users/me (getMe): what loadCurrentUser reads back. */
function getMe() {
  const row = server.row;
  const theme = applyProfileThemeDefaults(row as never);
  return {
    ...row,
    ...theme,
    avatar_display: normalizeAvatarDisplay(row.avatar_display),
    display_name_style: normalizeDisplayNameStyle(row.display_name_style),
    share_activity: true,
    entitlement: { tier: "founder", source: "grant" },
    badges: [],
  };
}

const api = vi.hoisted(() => ({
  apiGet: vi.fn(),
  apiPatch: vi.fn(),
  apiPost: vi.fn(),
  apiPut: vi.fn(),
  apiDelete: vi.fn(),
  apiUpload: vi.fn(),
}));

vi.mock("@/lib/api-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api-client")>()),
  ...api,
}));

vi.mock("@kova/react", () => ({
  useUser: () => ({ isLoaded: true, user: null }),
}));

vi.mock("../GifPickerModal", () => ({
  default: ({
    open,
    onSelect,
  }: {
    open: boolean;
    onSelect: (gif: unknown) => Promise<void>;
  }) =>
    open ? (
      <button
        type="button"
        onClick={() =>
          void onSelect({
            id: "gif-1",
            title: "Party",
            provider: "tenor",
            preview: {
              url: "https://media.tenor.com/p.gif",
              contentType: "image/gif",
            },
            send: {
              url: "https://media.tenor.com/s.gif",
              contentType: "image/gif",
            },
            sourceUrl: "https://tenor.com/view/party",
            aspectRatio: 1,
          })
        }
      >
        Pick test GIF
      </button>
    ) : null,
}));

// The real frame editor needs a laid-out image; confirm the way it does.
vi.mock("../AvatarFrameEditor", () => ({
  AvatarFrameEditor: ({
    initialDisplay,
    onConfirm,
  }: {
    initialDisplay: unknown;
    onConfirm: (display: AvatarDisplay) => void;
  }) => (
    <button
      type="button"
      onClick={() =>
        onConfirm({
          version: 1,
          crop: { x: 12.5, y: 0, width: 75, height: 100 },
          collectibles: normalizeAvatarDisplay(initialDisplay)?.collectibles,
        })
      }
    >
      Apply test frame
    </button>
  ),
}));

vi.mock("../collectibles/CollectiblePicker", () => ({
  CollectiblePicker: () => null,
}));

import SettingsAccountTab from "../SettingsAccountTab";

const SEED_DISPLAY: AvatarDisplay = {
  version: 1,
  crop: { x: 10, y: 5, width: 80, height: 80 },
  collectibles: {
    avatarDecoration: {
      skuId: "deco-1",
      name: "Halo",
      asset: "a_halo",
      imageUrl:
        "https://cdn.discordapp.com/avatar-decoration-presets/a_halo.png",
    },
    profileEffect: {
      skuId: "effect-1",
      name: "Sparkle",
      effectUrls: ["https://cdn.discordapp.com/assets/effect.png"],
    },
    nameplate: {
      skuId: "plate-1",
      name: "Night",
      staticUrl: "https://cdn.discordapp.com/assets/plate.png",
    },
    profileFrame: {
      skuId: "frame-1",
      name: "Gold",
      innerWidth: 300,
      overflowTop: 10,
      overflowBottom: 10,
      overflowHorizontal: 10,
      layers: [
        {
          id: "front",
          src: "https://cdn.yapper.shop/frames/gold.png",
          type: "image",
          order: "front",
          anchor: "center",
          responsive: true,
        },
      ],
    },
  },
};

function seedRow() {
  return {
    id: "user-1",
    username: "jon",
    display_name: "Jon",
    avatar_url: "/api/avatars/user-1/a.png",
    avatar_display: serializeAvatarDisplay(SEED_DISPLAY),
    banner_url: "/api/profile-assets/banner/user-1/old.gif",
    banner_content_type: "image/gif",
    // Applying a nameplate collectible stores its art here too.
    nameplate_url: "https://cdn.discordapp.com/assets/plate.png",
    nameplate_content_type: "image/png",
    profile_accent_color: "#4B53AA",
    profile_background_color: "#161A22",
    profile_banner_color: "#60A5FA",
    display_name_style: JSON.stringify({
      font: "silkscreen",
      effect: "pixel",
      primaryColor: "#38BDF8",
      secondaryColor: "#FB7185",
    }),
    theme_preference: null,
    theme_sync_enabled: 0,
    media_content_filter: "blur_nsfw",
    share_activity: 1,
    updated_at: "2026-11-01T00:00:00.000Z",
    created_at: "2025-01-01T00:00:00.000Z",
    bio: "Building Yap.",
    pronouns: "he/him",
    status: "online",
    custom_status: null,
  } satisfies Row;
}

function route(method: string, url: string, body?: unknown) {
  server.calls.push({ method, url, body });
  if (method === "GET" && url === "/api/users/me") return getMe();
  if (method === "GET" && url === "/api/avatars/history") {
    return {
      items: [
        {
          id: "recent-1",
          avatar_url: "/api/avatars/user-1/recent.gif",
          content_type: "image/gif",
          created_at: "2026-10-01T00:00:00.000Z",
        },
      ],
    };
  }
  if (method === "PATCH" && url === "/api/update-profile") {
    return updateProfile(body as Record<string, unknown>);
  }
  if (method === "UPLOAD" && url === "/api/profile-assets/manage") {
    const form = body as FormData;
    const file = form.get("file") as File;
    server.row.banner_url = "/api/profile-assets/banner/user-1/new.gif";
    server.row.banner_content_type = file.type;
    return { ok: true, user: { ...server.row } };
  }
  if (method === "UPLOAD" && url === "/api/avatar-upload") {
    const form = body as FormData;
    const display = normalizeAvatarDisplay(form.get("avatar_display"));
    server.row.avatar_url = "/api/avatars/user-1/new.gif";
    server.row.avatar_display = serializeAvatarDisplay(display);
    return { url: server.row.avatar_url, avatar_display: display };
  }
  if (method === "DELETE" && url === "/api/profile-assets/manage") {
    server.row.banner_url = null;
    server.row.banner_content_type = null;
    return { ok: true };
  }
  if (method === "PATCH" && url === "/api/collectibles/apply") {
    const { kind, skuId } = body as { kind: string; skuId: string | null };
    const display = normalizeAvatarDisplay(server.row.avatar_display);
    if (kind === "nameplate" && skuId === null && display?.collectibles) {
      delete display.collectibles.nameplate;
    }
    server.row.avatar_display = serializeAvatarDisplay(display);
    server.row.nameplate_url = null;
    server.row.nameplate_content_type = null;
    return {
      ok: true,
      user: {
        avatar_display: normalizeAvatarDisplay(server.row.avatar_display),
        nameplate_url: null,
        nameplate_content_type: null,
      },
    };
  }
  if (method === "POST" && url === "/api/presence") {
    const { status, custom_status } = body as {
      status: string;
      custom_status?: string | null;
    };
    server.row.status = status;
    server.row.custom_status = custom_status ?? null;
    return { status, custom_status: custom_status ?? null };
  }
  // Anything else the store asks for on the side.
  return {};
}

async function seedStore() {
  server.row = seedRow();
  server.calls = [];
  await act(async () => {
    await useChatStore.getState().actions.loadCurrentUser();
  });
  expect(useChatStore.getState().user?.id).toBe("user-1");
}

function renderEditor() {
  return render(
    <TooltipProvider>
      <SettingsAccountTab authUserLoaded onClose={() => {}} />
    </TooltipProvider>,
  );
}

const bar = () => screen.queryByRole("region", { name: "Unsaved changes" });

async function save() {
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() =>
    expect(
      server.calls.filter((c) => c.url === "/api/users/me").length,
    ).toBeGreaterThan(1),
  );
  // Let handleSave run to its end (the Save button stops loading).
  await waitFor(() =>
    expect(
      screen.queryByRole("button", { name: "Save changes", busy: true }),
    ).not.toBeInTheDocument(),
  );
  await act(async () => {});
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
}

function gifFile(name = "party.gif") {
  return new File([new Uint8Array([71, 73, 70, 56, 57, 97])], name, {
    type: "image/gif",
  });
}

async function openPicker(label: "Change banner" | "Change avatar") {
  fireEvent.click(screen.getByRole("button", { name: label }));
  return screen.findByRole("dialog", {
    name: label === "Change banner" ? "Select a banner" : "Choose an image",
  });
}

async function uploadThroughPicker(label: "Change banner" | "Change avatar") {
  const dialog = await openPicker(label);
  const input = dialog.ownerDocument.querySelector(
    'input[type="file"][aria-label="Upload avatar image"]',
  ) as HTMLInputElement;
  fireEvent.change(input, { target: { files: [gifFile()] } });
}

async function gifThroughPicker(label: "Change banner" | "Change avatar") {
  const dialog = await openPicker(label);
  fireEvent.click(within(dialog).getByRole("button", { name: /Choose GIF/ }));
  fireEvent.click(await screen.findByText("Pick test GIF"));
}

describe("ProfileEditor save round trip", () => {
  beforeEach(async () => {
    let blobId = 0;
    URL.createObjectURL = vi.fn(() => `blob:test/${++blobId}`);
    URL.revokeObjectURL = vi.fn();
    // gifToFile downloads the GIF; answer with a jsdom Blob so File accepts it.
    globalThis.fetch = vi.fn(async () => ({
      ok: true,
      blob: async () =>
        new Blob([new Uint8Array([71, 73, 70])], { type: "image/gif" }),
    })) as unknown as typeof fetch;
    api.apiGet.mockImplementation(async (url: string) => route("GET", url));
    api.apiPatch.mockImplementation(async (url: string, body: unknown) =>
      route("PATCH", url, body),
    );
    api.apiUpload.mockImplementation(async (url: string, body: unknown) =>
      route("UPLOAD", url, body),
    );
    api.apiPost.mockImplementation(async (url: string, body: unknown) =>
      route("POST", url, body),
    );
    api.apiDelete.mockImplementation(async (url: string, body: unknown) =>
      route("DELETE", url, body),
    );
    await seedStore();
  });

  afterEach(() => {
    vi.clearAllMocks();
    useChatStore.setState({ user: null } as never);
  });

  it("opens clean for the seeded user", () => {
    renderEditor();
    expect(bar()).not.toBeInTheDocument();
  });

  it("is clean after saving a GIF banner picked from a file", async () => {
    renderEditor();
    await uploadThroughPicker("Change banner");
    await waitFor(() => expect(bar()).toBeInTheDocument());
    await save();
    expect(server.row.banner_content_type).toBe("image/gif");
    await waitFor(() => expect(bar()).not.toBeInTheDocument());
  });

  it("is clean after saving a GIF banner from the GIF picker", async () => {
    renderEditor();
    await gifThroughPicker("Change banner");
    await waitFor(() => expect(bar()).toBeInTheDocument());
    await save();
    expect(server.row.banner_url).toContain("new.gif");
    await waitFor(() => expect(bar()).not.toBeInTheDocument());
  });

  it("is clean after saving a GIF avatar picked from a file", async () => {
    renderEditor();
    await uploadThroughPicker("Change avatar");
    fireEvent.click(
      await screen.findByRole("button", { name: "Apply test frame" }),
    );
    await waitFor(() => expect(bar()).toBeInTheDocument());
    await save();
    expect(server.row.avatar_url).toContain("new.gif");
    await waitFor(() => expect(bar()).not.toBeInTheDocument());
  });

  it("is clean after saving a GIF avatar from the GIF picker", async () => {
    renderEditor();
    await gifThroughPicker("Change avatar");
    fireEvent.click(
      await screen.findByRole("button", { name: "Apply test frame" }),
    );
    await waitFor(() => expect(bar()).toBeInTheDocument());
    await save();
    await waitFor(() => expect(bar()).not.toBeInTheDocument());
  });

  it("is clean after saving a recent GIF avatar", async () => {
    renderEditor();
    const dialog = await openPicker("Change avatar");
    fireEvent.click(
      await within(dialog).findByRole("button", {
        name: "Use this recent avatar",
      }),
    );
    await waitFor(() => expect(bar()).toBeInTheDocument());
    await save();
    expect(server.row.avatar_url).toBe("/api/avatars/user-1/recent.gif");
    await waitFor(() => expect(bar()).not.toBeInTheDocument());
  });

  describe("other save paths", () => {
    async function editAndSave(edit: () => void | Promise<void>) {
      renderEditor();
      await edit();
      await waitFor(() => expect(bar()).toBeInTheDocument());
      await save();
      await waitFor(() => expect(bar()).not.toBeInTheDocument());
    }

    it("pronouns with a trailing space", () =>
      editAndSave(() => {
        fireEvent.change(screen.getByRole("textbox", { name: "Pronouns" }), {
          target: { value: "she/her " },
        });
      }));

    it("cleared pronouns", () =>
      editAndSave(() => {
        fireEvent.change(screen.getByRole("textbox", { name: "Pronouns" }), {
          target: { value: "" },
        });
      }));

    it("cleared bio", () =>
      editAndSave(() => {
        fireEvent.change(screen.getByRole("textbox", { name: "About me" }), {
          target: { value: "" },
        });
      }));

    it("custom status", () =>
      editAndSave(() => {
        fireEvent.change(
          screen.getByRole("textbox", { name: "Custom status" }),
          { target: { value: "Shipping" } },
        );
      }));

    it("custom status when presence lands after the reload", async () => {
      // updateStatus does not wait for POST /api/presence; hold it back.
      let release = () => {};
      api.apiPost.mockImplementation(
        (url: string, body: unknown) =>
          new Promise((resolve) => {
            release = () => resolve(route("POST", url, body));
          }),
      );
      await editAndSave(() => {
        fireEvent.change(
          screen.getByRole("textbox", { name: "Custom status" }),
          { target: { value: "Shipping" } },
        );
      });
      release();
    });

    it("accent preset", () =>
      editAndSave(() => {
        const group = screen.getByRole("group", { name: "Accent" });
        fireEvent.click(within(group).getByRole("button", { name: "#FFFFFF" }));
      }));

    it("background back to Auto when it already is the default", () => {
      // The accent is custom, so an Auto background saves as the default
      // background the user already has: there is nothing to save.
      renderEditor();
      fireEvent.click(
        screen.getByRole("button", {
          name: "Use the default background color",
        }),
      );
      expect(bar()).not.toBeInTheDocument();
    });

    it("custom background back to Auto", async () => {
      server.row.profile_background_color = "#222222";
      await act(async () => {
        await useChatStore.getState().actions.loadCurrentUser();
      });
      await editAndSave(() => {
        fireEvent.click(
          screen.getByRole("button", {
            name: "Use the default background color",
          }),
        );
      });
      expect(server.row.profile_background_color).toBe("#161A22");
    });

    it("accent back to Auto", () =>
      editAndSave(() => {
        fireEvent.click(
          screen.getByRole("button", { name: "Use the default accent color" }),
        );
      }));

    it("banner color back to Auto", () =>
      editAndSave(() => {
        fireEvent.click(
          screen.getByRole("button", { name: "Use the default banner color" }),
        );
      }));

    it("cleared name style", () =>
      editAndSave(() => {
        fireEvent.click(
          screen.getByRole("button", { name: "Clear display name style" }),
        );
      }));

    it("removed banner", () =>
      editAndSave(() => {
        fireEvent.click(screen.getByRole("button", { name: "Remove banner" }));
      }));

    it("removed avatar decoration", () =>
      editAndSave(() => {
        fireEvent.click(
          screen.getByRole("button", { name: "Remove avatar decoration" }),
        );
      }));

    it("removed profile frame", () =>
      editAndSave(() => {
        fireEvent.click(
          screen.getByRole("button", { name: "Remove profile frame" }),
        );
      }));

    it("removed nameplate", () =>
      editAndSave(() => {
        fireEvent.click(
          screen.getByRole("button", { name: "Remove nameplate" }),
        );
      }));

    it("removed avatar", () =>
      editAndSave(() => {
        fireEvent.click(screen.getByRole("button", { name: "Remove avatar" }));
      }));
  });
});
