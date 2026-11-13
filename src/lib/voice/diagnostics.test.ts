import { describe, expect, it } from "vitest";

import { buildVoiceDiagnosticsBundle } from "./diagnostics";

describe("buildVoiceDiagnosticsBundle", () => {
  it("adds support context and strips query strings from page URLs", () => {
    const bundle = buildVoiceDiagnosticsBundle({
      detailedStats: { ping: 42 },
      connectionStats: null,
      channelName: "General",
      locationHref: "https://meet.example/chat?token=secret#frag",
      userAgent: "TestBrowser",
      now: () => new Date("2026-06-30T12:00:00.000Z"),
    });

    expect(bundle).toMatchObject({
      app: "Yap",
      copiedAt: "2026-06-30T12:00:00.000Z",
      channelName: "General",
      page: "https://meet.example/chat",
      userAgent: "TestBrowser",
      detailedStats: { ping: 42 },
    });
  });

  it("redacts token and user id shaped diagnostic fields", () => {
    const bundle = buildVoiceDiagnosticsBundle({
      detailedStats: {
        voiceToken: "secret",
        user_id: "user-123",
        nested: { authorization: "Bearer nope" },
      },
      connectionStats: null,
    });

    expect(bundle.detailedStats).toEqual({
      voiceToken: "[redacted]",
      user_id: "[redacted]",
      nested: { authorization: "[redacted]" },
    });
  });

  it("redacts ICE addresses and IPs inside candidate strings", () => {
    const bundle = buildVoiceDiagnosticsBundle({
      detailedStats: {
        local: {
          address: "192.168.1.20",
          ip: "203.0.113.7",
          relatedAddress: "10.0.0.2",
          url: "stun:stun.example:3478",
          port: 50000,
          candidateType: "srflx",
        },
        candidate:
          "candidate:1 1 udp 2122260223 198.51.100.4 54321 typ srflx raddr 10.0.0.2 rport 9",
        v6: "candidate:2 1 udp 1 2001:db8:85a3::8a2e:370:7334 9 typ host",
        v6full: "fe80:0:0:0:200:f8ff:fe21:67cf",
        codec: "audio/opus",
        note: "decode::failed in std::string",
      },
      connectionStats: null,
    });

    expect(bundle.detailedStats).toEqual({
      local: {
        address: "[redacted]",
        ip: "[redacted]",
        relatedAddress: "[redacted]",
        url: "[redacted]",
        port: 50000,
        candidateType: "srflx",
      },
      candidate:
        "candidate:1 1 udp 2122260223 [ip] 54321 typ srflx raddr [ip] rport 9",
      v6: "candidate:2 1 udp 1 [ip] 9 typ host",
      v6full: "[ip]",
      codec: "audio/opus",
      note: "decode::failed in std::string",
    });
  });
});
