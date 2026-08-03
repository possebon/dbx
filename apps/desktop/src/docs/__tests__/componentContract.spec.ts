import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { parse } from "vue/compiler-sfc";

const docsRoot = path.resolve(__dirname, "..");

function vueFiles(): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory() && entry.name !== "__tests__" && entry.name !== "fixtures") {
        walk(full);
      } else if (entry.isFile() && entry.name.endsWith(".vue")) {
        found.push(full);
      }
    }
  };
  walk(docsRoot);
  return found;
}

function scriptOf(file: string): string {
  const { descriptor } = parse(readFileSync(file, "utf8"), { filename: file });
  return `${descriptor.script?.content ?? ""}\n${descriptor.scriptSetup?.content ?? ""}`;
}

const EXPECTED = ["ColumnTable.vue", "DocsApp.vue", "DocsSearch.vue", "DocsSidebar.vue", "RelationshipList.vue", "TablePage.vue", "WarningBanner.vue", "WikiIndex.vue"];

describe("docs viewer component contract", () => {
  it("finds every expected component", () => {
    expect(
      vueFiles()
        .map((file) => path.basename(file))
        .sort(),
    ).toEqual(EXPECTED);
  });

  // Every test below loops over vueFiles(). On an empty set those loops run zero
  // assertions and pass while proving nothing, so each one asserts the set is
  // populated first. Without this, deleting every component turns the whole
  // contract green.
  it("makes no backend calls", () => {
    const files = vueFiles();
    expect(files.length).toBe(EXPECTED.length);
    const forbidden = ["@/lib/backend", "@tauri-apps", "invoke(", "useConnectionStore", "useQueryStore", "useSettingsStore", "fetch(", "axios"];
    for (const file of files) {
      const script = scriptOf(file);
      for (const needle of forbidden) {
        expect(script.includes(needle), `${path.basename(file)} must not reference ${needle}`).toBe(false);
      }
    }
  });

  it("keeps colour decisions out of templates", () => {
    const files = vueFiles();
    expect(files.length).toBe(EXPECTED.length);
    for (const file of files) {
      const source = readFileSync(file, "utf8");
      expect(source.includes("oklch("), `${path.basename(file)} must not compute colour`).toBe(false);
      expect(/#[0-9a-fA-F]{6}\b/.test(source), `${path.basename(file)} must not hardcode a hex colour`).toBe(false);
    }
  });

  it("only ever feeds renderNote output to v-html", () => {
    // The single most dangerous thing a template here can do. Task 7 escapes
    // author HTML, but `v-html="table.note"` bypasses it entirely and hands a
    // COMMENT ON value straight to the DOM. Every v-html binding must name
    // renderNote, and no component may build HTML any other way.
    const files = vueFiles();
    expect(files.length).toBe(EXPECTED.length);
    for (const file of files) {
      const source = readFileSync(file, "utf8");
      for (const match of source.matchAll(/v-html\s*=\s*"([^"]*)"/g)) {
        expect(match[1], `${path.basename(file)}: v-html must render renderNote output`).toContain("renderNote");
      }
      expect(source.includes("innerHTML"), `${path.basename(file)} must not touch innerHTML`).toBe(false);
    }
  });

  it('uses <script setup lang="ts">', () => {
    const files = vueFiles();
    expect(files.length).toBe(EXPECTED.length);
    for (const file of files) {
      const { descriptor } = parse(readFileSync(file, "utf8"), { filename: file });
      expect(descriptor.scriptSetup, `${path.basename(file)} must use <script setup>`).toBeTruthy();
      expect(descriptor.scriptSetup?.lang, `${path.basename(file)} must be TypeScript`).toBe("ts");
    }
  });

  it("defines the group colour tokens for WebViews without oklch", () => {
    // DBX supports legacy WebViews with no oklch (globals.css carries an
    // `@supports not (color: oklch(...))` block). The repo's convention is
    // progressive enhancement: a legacy-safe base value first, then the same
    // token redefined inside `@supports (color: oklch(1 0 0))`. Without the
    // base, every table group renders colourless on those WebViews.
    //
    // Assert each selector's base block separately. An ordering check like
    // `indexOf(hsl) < indexOf(@supports)` quantifies over ANY occurrence, so
    // deleting the light block leaves the dark block's hsl satisfying it — the
    // test passes while light-theme legacy WebViews render every group
    // colourless.
    const css = readFileSync(path.join(docsRoot, "docs.css"), "utf8");
    const enhanced = css.indexOf("@supports (color: oklch(1 0 0))");
    expect(enhanced).toBeGreaterThan(-1);
    const legacyBase = css.slice(0, enhanced);

    for (const selector of [".docs-group", ".dark .docs-group"]) {
      // `^` with the m flag anchors to a line start, so `.docs-group` cannot
      // match inside `.dark .docs-group`, and neither matches the indented
      // copies inside the @supports block.
      const pattern = new RegExp(`^${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`, "m");
      const block = pattern.exec(legacyBase)?.[1];
      expect(block, `${selector} needs a legacy-safe base block before @supports`).toBeTruthy();
      expect(block, `${selector} must define --group-c without oklch`).toContain("--group-c: hsl(");
      expect(block, `${selector} must define --group-tint without oklch`).toContain("--group-tint: hsl(");
    }
  });
});
