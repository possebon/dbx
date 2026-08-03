import { describe, expect, it } from "vitest";
import { describeWarning } from "../docsWarnings";
import type { SnapshotWarning } from "../types";

describe("describeWarning", () => {
  it("explains a skipped table as a warning naming the table and reason", () => {
    const notice = describeWarning({ kind: "tableSkipped", table: "public.secret", reason: "permission denied" });
    expect(notice.severity).toBe("warning");
    expect(notice.detail).toContain("public.secret");
    expect(notice.detail).toContain("permission denied");
  });

  it("explains missing foreign-key metadata as an engine limitation, not a fault", () => {
    const notice = describeWarning({ kind: "noForeignKeyMetadata", engine: "ClickHouse" });
    expect(notice.detail).toContain("ClickHouse");
    // The diagram will have no edges — the user must learn why.
    expect(notice.detail.toLowerCase()).toContain("relationship");
  });

  it("explains unsupported comments", () => {
    const notice = describeWarning({ kind: "commentsUnsupported", engine: "SQLite" });
    expect(notice.detail).toContain("SQLite");
  });

  it("reports orphaned notes with the count", () => {
    const notice = describeWarning({ kind: "orphanedNotes", count: 3 });
    expect(notice.detail).toContain("3");
  });

  it("explains a DBML omission naming the item", () => {
    const notice = describeWarning({
      kind: "dbmlOmitted",
      table: "public.orders",
      item: "idx_orders_open",
      reason: "partial index filter has no DBML equivalent",
    });
    expect(notice.severity).toBe("info");
    expect(notice.detail).toContain("idx_orders_open");
  });

  it("never returns an empty or placeholder string for any known kind", () => {
    const samples: SnapshotWarning[] = [
      { kind: "tableSkipped", table: "t", reason: "r" },
      { kind: "noForeignKeyMetadata", engine: "e" },
      { kind: "commentsUnsupported", engine: "e" },
      { kind: "orphanedNotes", count: 1 },
      { kind: "dbmlOmitted", table: "t", item: "i", reason: "r" },
    ];
    for (const sample of samples) {
      const notice = describeWarning(sample);
      expect(notice.title.length, `empty title for ${sample.kind}`).toBeGreaterThan(0);
      expect(notice.detail.length, `empty detail for ${sample.kind}`).toBeGreaterThan(0);
      expect(notice.detail).not.toContain("[object Object]");
      expect(notice.detail).not.toContain("undefined");
    }
  });
});
