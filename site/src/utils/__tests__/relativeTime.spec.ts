// ABOUTME: Tests relative timestamp formatting for admin usage dates.
import { describe, expect, it } from "vitest";
import { formatRelativeUpdatedAt } from "@/utils/relativeTime";

describe("formatRelativeUpdatedAt", () => {
  it("returns Unknown for missing or invalid timestamps", () => {
    expect(formatRelativeUpdatedAt(null)).toBe("Unknown");
    expect(formatRelativeUpdatedAt(undefined)).toBe("Unknown");
    expect(formatRelativeUpdatedAt("not-a-date")).toBe("Unknown");
  });

  it("appends a relative suffix to the absolute date", () => {
    const now = new Date("2026-09-12T18:00:00Z").getTime();
    const fiveMinutesAgo = new Date("2026-09-12T17:55:00Z").toISOString();

    const formatted = formatRelativeUpdatedAt(fiveMinutesAgo, now);
    expect(formatted).toContain("5m ago");
  });
});
