import { describe, expect, it } from "vitest";
import type { SchemaSnapshot, SnapshotWarning } from "../types";

describe("snapshot types", () => {
  it("accepts a minimal snapshot shaped like the Rust output", () => {
    const snapshot: SchemaSnapshot = {
      formatVersion: 1,
      project: {
        name: "Ecommerce",
        databaseType: "postgres",
        database: "shop",
        schemas: ["public"],
        generatedAt: "2026-08-03T00:00:00Z",
        note: null,
      },
      tables: [
        {
          schema: "public",
          name: "orders",
          kind: "TABLE",
          columns: [],
          indexes: [],
          foreignKeys: [],
          groupId: null,
          note: "Checkout rows.",
          noteSource: "DATABASE",
          shadowedNote: null,
          columnNotes: {},
          estimatedRows: null,
          viewDefinition: null,
        },
      ],
      relationships: [],
      groups: [],
      enums: [],
      warnings: [],
    };

    expect(snapshot.tables[0].noteSource).toBe("DATABASE");
    expect(snapshot.tables[0].kind).toBe("TABLE");
  });

  it("discriminates warnings on a camelCase kind", () => {
    // Rust: #[serde(rename_all = "camelCase", tag = "kind")] — so the
    // discriminant is camelCase even though sibling enums are SCREAMING_SNAKE.
    const warning: SnapshotWarning = {
      kind: "tableSkipped",
      table: "public.secret",
      reason: "permission denied",
    };
    expect(warning.kind).toBe("tableSkipped");

    const orphans: SnapshotWarning = { kind: "orphanedNotes", count: 3 };
    if (orphans.kind === "orphanedNotes") {
      expect(orphans.count).toBe(3);
    } else {
      throw new Error("discriminated union must narrow on kind");
    }
  });
});
