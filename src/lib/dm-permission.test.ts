import { describe, expect, it } from "vitest";

import { describeApiError } from "./api-errors";
import {
  DM_PERMISSION_OPTIONS,
  DM_PERMISSIONS,
  dmNotAcceptingCall,
  dmNotAcceptingErrorMessage,
  dmNotAcceptingMessage,
  dmPermissionOf,
  isDmPermission,
} from "./dm-permission";

const refusal = (message: string) =>
  Object.assign(new Error(message), { status: 403, code: "NOT_ACCEPTING_DMS" });

describe("dm permission", () => {
  it("reads a stored value or falls back to everyone", () => {
    expect(dmPermissionOf("nobody")).toBe("nobody");
    expect(dmPermissionOf("friends_and_servers")).toBe("friends_and_servers");
    expect(dmPermissionOf("admins")).toBe("everyone");
    expect(dmPermissionOf(undefined)).toBe("everyone");
    expect(isDmPermission("Everyone")).toBe(false);
  });

  it("lists one option per value, in order", () => {
    expect(DM_PERMISSION_OPTIONS.map((option) => option.value)).toEqual([
      ...DM_PERMISSIONS,
    ]);
  });

  it("names the person, or says this person", () => {
    expect(dmNotAcceptingMessage("Alex")).toBe(
      "Alex isn't accepting messages from you.",
    );
    expect(dmNotAcceptingCall(" ")).toBe(
      "This person isn't accepting calls from you.",
    );
  });

  it("shows the server's sentence for its refusal code only", () => {
    const error = refusal("Alex isn't accepting messages from you.");
    expect(dmNotAcceptingErrorMessage(error)).toBe(
      "Alex isn't accepting messages from you.",
    );
    expect(describeApiError(error, "open this DM")).toBe(
      "Alex isn't accepting messages from you.",
    );
    expect(
      dmNotAcceptingErrorMessage(
        Object.assign(new Error("Forbidden"), { status: 403 }),
      ),
    ).toBeNull();
  });
});
