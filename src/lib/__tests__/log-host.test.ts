import { describe, expect, it } from "vitest";

import { logHost } from "@/lib/log-host";

describe("logHost", () => {
  it("keeps only the host of a URL", () => {
    expect(logHost("https://x.com/someone/status/123?token=abc#frag")).toBe(
      "x.com",
    );
    expect(logHost(new URL("https://user:pw@example.com:8443/a?b=c"))).toBe(
      "example.com",
    );
  });

  it("never echoes a string that is not a URL", () => {
    expect(logHost("not a url with a secret")).toBe("invalid-url");
    expect(logHost("")).toBeNull();
    expect(logHost(null)).toBeNull();
  });
});
