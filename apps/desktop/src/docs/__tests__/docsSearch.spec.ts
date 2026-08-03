import { describe, expect, it } from "vitest";
import { searchDocs } from "../docsSearch";
import type { DocTable, SchemaSnapshot } from "../types";

function column(name: string) {
  return {
    name,
    data_type: "text",
    is_nullable: true,
    column_default: null,
    is_primary_key: false,
    extra: null,
  };
}

function table(schema: string, name: string, columns: string[] = []): DocTable {
  return {
    schema,
    name,
    kind: "TABLE",
    columns: columns.map(column),
    indexes: [],
    foreignKeys: [],
    groupId: null,
    note: null,
    noteSource: "NONE",
    shadowedNote: null,
    columnNotes: {},
    estimatedRows: null,
    viewDefinition: null,
  };
}

const snapshot: SchemaSnapshot = {
  formatVersion: 1,
  project: { name: "p", databaseType: "postgres", database: null, schemas: [], generatedAt: "", note: null },
  tables: [table("public", "orders", ["status", "total"]), table("public", "customers", ["status"])],
  relationships: [],
  groups: [{ id: "g1", name: "Order Management", hue: 28, note: null }],
  enums: [{ schema: "public", name: "order_status", values: ["pending"], note: null, synthesized: false }],
  warnings: [],
};

describe("searchDocs", () => {
  it("returns nothing for an empty query", () => {
    expect(searchDocs(snapshot, "")).toEqual([]);
    expect(searchDocs(snapshot, "   ")).toEqual([]);
  });

  it("matches table names case-insensitively", () => {
    const hits = searchDocs(snapshot, "ORD");
    expect(hits.some((hit) => hit.kind === "table" && hit.label === "orders")).toBe(true);
  });

  it("matches columns and reports which table they belong to", () => {
    const hits = searchDocs(snapshot, "total");
    const hit = hits.find((candidate) => candidate.kind === "column");
    expect(hit).toBeDefined();
    expect(hit!.label).toBe("total");
    expect(hit!.context).toContain("orders");
  });

  it("returns one hit per table for a column name shared by several tables", () => {
    const hits = searchDocs(snapshot, "status").filter((hit) => hit.kind === "column");
    expect(hits).toHaveLength(2);
    expect(hits.map((hit) => hit.context).sort()).toEqual(["public.customers", "public.orders"]);
  });

  it("matches groups and enums", () => {
    expect(searchDocs(snapshot, "Order Man").some((hit) => hit.kind === "group")).toBe(true);
    expect(searchDocs(snapshot, "order_status").some((hit) => hit.kind === "enum")).toBe(true);
  });

  it("ranks table matches above column matches for the same term", () => {
    // Someone typing "orders" almost always wants the table.
    const hits = searchDocs(snapshot, "orders");
    expect(hits[0].kind).toBe("table");
  });

  it("carries a tableKey so a hit can navigate", () => {
    const hit = searchDocs(snapshot, "total").find((candidate) => candidate.kind === "column");
    expect(hit!.tableKey).toBe("public.orders");
  });
});
