import type { AnnotationFile, SchemaSnapshot } from "@/docs/types";
import type { ExportLocale } from "./exportTranslate";

/**
 * The contract between the exporter and this bundle.
 *
 * Task 6's Rust side serialises exactly this object as JSON, encodes it UTF-8
 * then base64, and writes it as the text of
 * `<script type="application/dbx-snapshot">`. Nothing else in the emitted
 * document is read by the bundle.
 *
 * `lang` picks the starting locale only. The reader can change it — the
 * person who exported the file is rarely the person who opens it.
 */
export interface ExportPayload {
  snapshot: SchemaSnapshot;
  annotations: AnnotationFile;
  lang: ExportLocale;
}
