// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";

describe("the export entry point", () => {
  it("says the file is damaged rather than leaving a blank page", async () => {
    // A reader who opens a truncated or hand-edited export gets a white screen
    // and no clue unless this path holds. Importing the entry IS the test:
    // mounting is its module-level side effect.
    document.body.innerHTML = `<div id="app"></div>`;
    await import("../main");
    expect(document.querySelector("#app")?.textContent).toContain("could not be read");
    expect(document.querySelector("#app")?.textContent).toContain("application/dbx-snapshot");
  });
});
