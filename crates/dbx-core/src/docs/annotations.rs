use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The on-disk notes file. This IS the store — not a cache of anything.
/// It is meant to be committed to a repository and reviewed in pull
/// requests, so it must stay small, readable, and stable in key order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnotationFile {
    pub format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupAnnotation>,
    /// Keyed by `schema.table` (or bare `table` on schema-less engines).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tables: BTreeMap<String, TableAnnotation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAnnotation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Markdown. Becomes the documentation landing page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupAnnotation {
    /// Stable slug, referenced by `TableAnnotation::group`.
    pub id: String,
    pub name: String,
    /// 0..=359. Lightness and chroma are theme-controlled, so any hue is
    /// legible on both light and dark grounds by construction.
    pub hue: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableAnnotation {
    /// References `GroupAnnotation::id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Keyed by bare column name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub columns: BTreeMap<String, ColumnAnnotation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnAnnotation {
    pub note: String,
}

/// The only format version this build understands.
pub const ANNOTATION_FORMAT_VERSION: u32 = 1;

/// Load the notes file.
///
/// An ABSENT file returns `Ok(None)` — that is the normal first-run and
/// first-CI-run state. A MALFORMED file is a hard error: someone's prose is
/// in there, and rendering apparently-complete documentation while silently
/// discarding it is worse than failing.
pub fn load_annotations(path: &Path) -> Result<Option<AnnotationFile>, String> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Failed to read notes file {}: {error}", path.display())),
    };

    let parsed: AnnotationFile = serde_json::from_str(&contents)
        .map_err(|error| format!("Failed to parse notes file {}: {error}", path.display()))?;

    if parsed.format_version != ANNOTATION_FORMAT_VERSION {
        return Err(format!(
            "Notes file {} has formatVersion {}, but this build understands {}.",
            path.display(),
            parsed.format_version,
            ANNOTATION_FORMAT_VERSION
        ));
    }

    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"{
  "formatVersion": 1,
  "project": { "name": "Ecommerce", "note": "# Overview" },
  "groups": [
    { "id": "order-management", "name": "Order Management", "hue": 28, "note": "Checkout to handoff." }
  ],
  "tables": {
    "core.orders": {
      "group": "order-management",
      "note": "One row per checkout.",
      "columns": { "status": { "note": "State machine." } }
    }
  }
}"##;

    #[test]
    fn parses_a_complete_notes_file() {
        let parsed: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        assert_eq!(parsed.format_version, 1);
        assert_eq!(parsed.project.as_ref().unwrap().name.as_deref(), Some("Ecommerce"));
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].hue, 28);
        assert_eq!(parsed.tables.len(), 1);

        let orders = parsed.tables.get("core.orders").expect("orders");
        assert_eq!(orders.group.as_deref(), Some("order-management"));
        assert_eq!(orders.note.as_deref(), Some("One row per checkout."));
        assert_eq!(orders.columns.get("status").unwrap().note, "State machine.");
    }

    #[test]
    fn a_minimal_file_needs_only_the_format_version() {
        let parsed: AnnotationFile = serde_json::from_str(r#"{"formatVersion": 1}"#).expect("parse");
        assert!(parsed.tables.is_empty());
        assert!(parsed.groups.is_empty());
        assert!(parsed.project.is_none());
    }

    #[test]
    fn round_trips_through_json() {
        let parsed: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");
        let written = serde_json::to_string(&parsed).expect("serialize");
        let reparsed: AnnotationFile = serde_json::from_str(&written).expect("reparse");

        // Every field in SAMPLE must survive a write/read cycle. Asserting
        // only a count here would pass against a model that silently drops
        // fields on write via a wrong skip_serializing_if predicate.
        assert_eq!(reparsed.format_version, 1);

        let project = reparsed.project.as_ref().expect("project survived");
        assert_eq!(project.name.as_deref(), Some("Ecommerce"));
        assert_eq!(project.note.as_deref(), Some("# Overview"));

        assert_eq!(reparsed.groups.len(), 1);
        let group = &reparsed.groups[0];
        assert_eq!(group.id, "order-management");
        assert_eq!(group.name, "Order Management");
        assert_eq!(group.hue, 28);
        assert_eq!(group.note.as_deref(), Some("Checkout to handoff."));

        assert_eq!(reparsed.tables.len(), 1);
        let orders = reparsed.tables.get("core.orders").expect("orders survived");
        assert_eq!(orders.group.as_deref(), Some("order-management"));
        assert_eq!(orders.note.as_deref(), Some("One row per checkout."));
        assert_eq!(orders.columns.len(), 1);
        assert_eq!(orders.columns.get("status").expect("column survived").note, "State machine.");
    }

    #[test]
    fn rejects_a_file_with_an_unknown_top_level_field() {
        // Typos in a hand-edited file must not be silently ignored — a
        // misspelled "tabels" key would otherwise discard every note in it.
        let result: Result<AnnotationFile, _> = serde_json::from_str(r#"{"formatVersion": 1, "tabels": {}}"#);
        assert!(result.is_err(), "unknown fields must be rejected");
    }

    fn temp_notes(contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("dbx-notes-test-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, contents).expect("write temp notes file");
        path
    }

    #[test]
    fn an_absent_file_is_not_an_error() {
        let missing = std::path::Path::new("/nonexistent/dbx-notes-does-not-exist.json");
        assert!(matches!(load_annotations(missing), Ok(None)));
    }

    #[test]
    fn a_valid_file_loads() {
        let path = temp_notes(SAMPLE);
        let loaded = load_annotations(&path).expect("load").expect("some");
        assert_eq!(loaded.tables.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_malformed_file_is_a_hard_error_naming_the_path() {
        let path = temp_notes("{ this is not json");
        let error = load_annotations(&path).expect_err("must fail");
        assert!(error.contains(&path.display().to_string()), "error must name the file: {error}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unsupported_format_version_is_rejected() {
        let path = temp_notes(r#"{"formatVersion": 99}"#);
        let error = load_annotations(&path).expect_err("must fail");
        assert!(error.contains("99"), "error must name the version: {error}");
        let _ = std::fs::remove_file(&path);
    }
}
