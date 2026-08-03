import type { SnapshotWarning } from "./types";

export interface WarningNotice {
  severity: "info" | "warning";
  title: string;
  detail: string;
}

/**
 * Turn a snapshot warning into something a reader can act on.
 *
 * This is where "degrade visibly, never silently" becomes literal: if a table
 * could not be read, or an engine cannot report relationships, the reader has
 * to learn that from the page rather than infer it from an absence.
 */
export function describeWarning(warning: SnapshotWarning): WarningNotice {
  switch (warning.kind) {
    case "tableSkipped":
      return {
        severity: "warning",
        title: "A table could not be documented",
        detail: `${warning.table} was skipped: ${warning.reason}. It is missing from this documentation.`,
      };
    case "noForeignKeyMetadata":
      return {
        severity: "info",
        title: "No relationships available",
        detail: `${warning.engine} does not report foreign key metadata, so no relationship edges could be derived. The diagram is complete for this engine.`,
      };
    case "commentsUnsupported":
      return {
        severity: "info",
        title: "Database comments unavailable",
        detail: `${warning.engine} does not support table or column comments, so every description here comes from this project's own notes.`,
      };
    case "orphanedNotes":
      return {
        severity: "warning",
        title: "Some notes no longer match anything",
        detail: `${warning.count} note(s) refer to a table or column that no longer exists. Nothing was deleted — re-map or remove them in the notes file.`,
      };
    case "dbmlOmitted":
      return {
        severity: "info",
        title: "Not representable in DBML",
        detail: `${warning.item} on ${warning.table} is documented here but omitted from the exported DBML: ${warning.reason}.`,
      };
  }
}
