import { describe, expect, it } from "vitest";

import { klipyCustomerId } from "@/lib/klipy-customer";

describe("klipyCustomerId", () => {
  it("is a stable pseudonym that does not contain the user id", async () => {
    const first = await klipyCustomerId("user-123", "secret-a");
    const again = await klipyCustomerId("user-123", "secret-a");

    expect(first).toMatch(/^[0-9a-f]{32}$/);
    expect(again).toBe(first);
    expect(first).not.toContain("user-123");
  });

  it("differs per user and per secret", async () => {
    const a = await klipyCustomerId("user-1", "secret-a");
    const b = await klipyCustomerId("user-2", "secret-a");
    const c = await klipyCustomerId("user-1", "secret-b");

    expect(new Set([a, b, c]).size).toBe(3);
  });

  it("gives nothing without a secret or a user", async () => {
    expect(await klipyCustomerId("user-1", undefined)).toBeUndefined();
    expect(await klipyCustomerId("user-1", "")).toBeUndefined();
    expect(await klipyCustomerId("", "secret-a")).toBeUndefined();
  });
});
