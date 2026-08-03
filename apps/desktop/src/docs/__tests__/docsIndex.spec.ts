import { describe, expect, it } from "vitest";
import { groupBySchema, groupByTableGroup } from "../docsIndex";
import type { DocTable, SchemaSnapshot, TableGroup } from "../types";

function table(schema: string | null, name: string, groupId: string | null = null): DocTable {
  return {
    schema,
    name,
    kind: "TABLE",
    columns: [],
    indexes: [],
    foreignKeys: [],
    groupId,
    note: null,
    noteSource: "NONE",
    shadowedNote: null,
    columnNotes: {},
    estimatedRows: null,
    viewDefinition: null,
  };
}

function snapshot(tables: DocTable[], groups: TableGroup[] = []): SchemaSnapshot {
  return {
    formatVersion: 1,
    project: { name: "p", databaseType: "postgres", database: null, schemas: [], generatedAt: "", note: null },
    tables,
    relationships: [],
    groups,
    enums: [],
    warnings: [],
  };
}

describe("groupBySchema", () => {
  it("groups tables under their schema, sorted by schema then name", () => {
    const sections = groupBySchema(snapshot([table("public", "orders"), table("analytics", "daily_sales"), table("public", "customers")]));

    expect(sections.map((section) => section.key)).toEqual(["analytics", "public"]);
    expect(sections[1].tables.map((t) => t.name)).toEqual(["customers", "orders"]);
  });

  it("puts schema-less tables in a single bare section", () => {
    const sections = groupBySchema(snapshot([table(null, "orders")]));
    expect(sections).toHaveLength(1);
    expect(sections[0].tables[0].name).toBe("orders");
  });
});

describe("groupByTableGroup", () => {
  const groups: TableGroup[] = [
    { id: "order-mgmt", name: "Order Management", hue: 28, note: "Checkout." },
    { id: "product-mgmt", name: "Product Management", hue: 148, note: null },
  ];

  it("groups tables by their group, preserving the snapshot's group order", () => {
    const sections = groupByTableGroup(snapshot([table("product", "products", "product-mgmt"), table("core", "orders", "order-mgmt")], groups));

    expect(sections.map((section) => section.key)).toEqual(["order-mgmt", "product-mgmt"]);
    expect(sections[0].label).toBe("Order Management");
    expect(sections[0].hue).toBe(28);
    expect(sections[0].note).toBe("Checkout.");
  });

  it("collects ungrouped tables into a trailing (no group) section", () => {
    const sections = groupByTableGroup(snapshot([table("core", "orders", "order-mgmt"), table("core", "users", null)], groups));

    const last = sections[sections.length - 1];
    expect(last.key).toBe("");
    expect(last.hue).toBeNull();
    expect(last.tables.map((t) => t.name)).toEqual(["users"]);
  });

  it("omits a group that has no members", () => {
    // render_group in the serializer skips empty groups; the viewer must not
    // show an empty header where the DBML shows nothing.
    const sections = groupByTableGroup(snapshot([table("core", "orders", "order-mgmt")], groups));
    expect(sections.map((section) => section.key)).not.toContain("product-mgmt");
  });

  it("treats a table whose groupId names no group as ungrouped", () => {
    const sections = groupByTableGroup(snapshot([table("core", "orders", "ghost")], groups));
    expect(sections).toHaveLength(1);
    expect(sections[0].key).toBe("");
  });
});
