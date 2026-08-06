import { createApp } from "vue";
import ExportApp from "./ExportApp.vue";
import type { ExportPayload } from "./exportPayload";
import "./export.css";

/**
 * Read the snapshot the exporter embedded in this document.
 *
 * The payload is `{ snapshot, annotations, lang }` as JSON, UTF-8, base64, in
 * the text of `<script type="application/dbx-snapshot">`. Task 6's Rust side
 * emits exactly that shape; base64 is what keeps a note containing `</script>`
 * from ending the tag early.
 */
function readPayload(): ExportPayload {
  const node = document.querySelector("script[type='application/dbx-snapshot']");
  if (node === null) throw new Error("no <script type='application/dbx-snapshot'> in this document");
  // `atob` yields one byte per character; the payload is UTF-8, so it must be
  // widened before decoding or every non-ASCII table name and note is mangled.
  const binary = atob((node.textContent ?? "").trim());
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return JSON.parse(new TextDecoder().decode(bytes)) as ExportPayload;
}

try {
  createApp(ExportApp, { payload: readPayload() }).mount("#app");
} catch (error) {
  // A blank page would be the reader's only clue that the file is damaged.
  // English is not a choice here: `lang` lives inside the payload that just
  // failed to parse, so there is no locale to translate into.
  const message = document.createElement("p");
  message.style.cssText = "margin:2rem;font-family:system-ui,sans-serif";
  message.textContent = `This documentation file could not be read: ${error instanceof Error ? error.message : String(error)}`;
  document.querySelector("#app")?.replaceChildren(message);
}
