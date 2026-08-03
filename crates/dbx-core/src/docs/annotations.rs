use std::collections::BTreeMap;

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
        assert_eq!(reparsed.tables.len(), 1);
        assert_eq!(reparsed.groups[0].name, "Order Management");
    }

    #[test]
    fn rejects_a_file_with_an_unknown_top_level_field() {
        // Typos in a hand-edited file must not be silently ignored — a
        // misspelled "tabels" key would otherwise discard every note in it.
        let result: Result<AnnotationFile, _> = serde_json::from_str(r#"{"formatVersion": 1, "tabels": {}}"#);
        assert!(result.is_err(), "unknown fields must be rejected");
    }
}
