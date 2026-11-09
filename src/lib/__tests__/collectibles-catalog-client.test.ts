import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGet: vi.fn(),
}));

vi.mock("@/lib/api-client", () => ({
  apiGet: mocks.apiGet,
}));

import * as client from "@/lib/collectibles-catalog-client";
import {
  loadCollectiblesCatalog,
  resetCollectiblesCatalogClient,
} from "@/lib/collectibles-catalog-client";

function catalog(stale: boolean) {
  return {
    version: 1,
    source: "yapper",
    syncedAt: new Date().toISOString(),
    stale,
    categories: [],
    items: [],
    counts: {},
  };
}

beforeEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  resetCollectiblesCatalogClient();
});

describe("loadCollectiblesCatalog", () => {
  it("does not refetch a stale catalog more than once per 10 minutes", async () => {
    vi.useFakeTimers();
    mocks.apiGet.mockResolvedValue({ catalog: catalog(true) });

    await loadCollectiblesCatalog();
    await loadCollectiblesCatalog();
    expect(mocks.apiGet).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(11 * 60 * 1000);
    await loadCollectiblesCatalog();
    expect(mocks.apiGet).toHaveBeenCalledTimes(2);
  });

  it("keeps a fresh catalog for 30 minutes", async () => {
    vi.useFakeTimers();
    mocks.apiGet.mockResolvedValue({ catalog: catalog(false) });

    await loadCollectiblesCatalog();
    vi.advanceTimersByTime(20 * 60 * 1000);
    await loadCollectiblesCatalog();
    expect(mocks.apiGet).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(11 * 60 * 1000);
    await loadCollectiblesCatalog();
    expect(mocks.apiGet).toHaveBeenCalledTimes(2);
  });
});

describe("refreshing", () => {
  it("is not something the app can ask for", async () => {
    // The admin console refreshes the catalog; the app only reads it.
    expect(Object.keys(client)).not.toContain("syncCollectiblesCatalogClient");
    mocks.apiGet.mockResolvedValue({ catalog: catalog(true) });
    await (loadCollectiblesCatalog as (options?: unknown) => Promise<unknown>)({
      forceRefresh: true,
    });
    expect(mocks.apiGet).toHaveBeenCalledWith("/api/collectibles/catalog");
  });
});
