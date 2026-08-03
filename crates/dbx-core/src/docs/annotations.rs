use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::docs::keys::{column_key, fold_identifier, table_key};
use crate::docs::{ColumnNote, NoteSource, SchemaSnapshot, SnapshotWarning, TableGroup};
use crate::models::connection::DatabaseType;

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

    // Read the version BEFORE full deserialization. `deny_unknown_fields`
    // would otherwise reject a future-format file with a confusing
    // "unknown field" error, never reaching the version check — defeating
    // the entire purpose of having a version field.
    let probe: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("Failed to parse notes file {}: {error}", path.display()))?;

    match probe.get("formatVersion").and_then(serde_json::Value::as_u64) {
        Some(version) if version == u64::from(ANNOTATION_FORMAT_VERSION) => {}
        Some(version) => {
            return Err(format!(
                "Notes file {} has formatVersion {version}, but this build understands {}.",
                path.display(),
                ANNOTATION_FORMAT_VERSION
            ))
        }
        None => return Err(format!("Notes file {} is missing formatVersion.", path.display())),
    }

    let parsed: AnnotationFile = serde_json::from_value(probe)
        .map_err(|error| format!("Failed to parse notes file {}: {error}", path.display()))?;

    Ok(Some(parsed))
}

/// Write the notes file atomically.
///
/// A partial write destroys prose a human typed, and `load_annotations`
/// errors loudly on malformed JSON — so a torn write becomes "your notes file
/// is corrupt" the next time the viewer opens. Write a sibling temp file,
/// flush it to disk, then rename: rename within a directory is atomic on
/// every platform DBX targets.
pub fn save_annotations(path: &Path, annotations: &AnnotationFile) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(annotations).map_err(|error| format!("Failed to serialize notes: {error}"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    }

    let temp = path.with_extension("json.tmp");
    {
        let mut file =
            std::fs::File::create(&temp).map_err(|error| format!("Failed to create {}: {error}", temp.display()))?;
        file.write_all(json.as_bytes()).map_err(|error| format!("Failed to write {}: {error}", temp.display()))?;
        file.sync_all().map_err(|error| format!("Failed to flush {}: {error}", temp.display()))?;
    }

    std::fs::rename(&temp, path).map_err(|error| format!("Failed to replace {}: {error}", path.display()))
}

/// Where a connection's notes file lives.
///
/// An explicit `docs_notes_path` wins — that is the entire point of the field.
/// Pointing it at a file inside a repository is what lets schema documentation
/// be reviewed in pull requests. Otherwise the file lives under the app data
/// directory keyed by connection id, so the feature works with no setup.
///
/// Takes the two fields it needs rather than a whole `ConnectionConfig`:
/// that struct has no `Default` and ~60 fields, so passing it would force
/// every test to build a literal full of values the function never reads.
/// `data_dir` is a parameter because `dbx-core` cannot reach the caller's
/// data directory on its own.
pub fn resolve_notes_path(connection_id: &str, docs_notes_path: Option<&str>, data_dir: &Path) -> PathBuf {
    if let Some(path) = docs_notes_path.map(str::trim).filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    data_dir.join("docs-notes").join(format!("{connection_id}.json"))
}

/// Merge a notes file into a collected snapshot.
///
/// Precedence is `local ?? database_comment`. When a local note shadows a
/// database comment the comment is kept in `shadowed_note`, so a later
/// `COMMENT ON` improvement stays visible rather than being silently hidden.
pub fn apply_annotations(snapshot: &mut SchemaSnapshot, annotations: &AnnotationFile, db_type: DatabaseType) {
    if let Some(project) = annotations.project.as_ref() {
        if let Some(name) = project.name.as_deref().filter(|value| !value.trim().is_empty()) {
            snapshot.project.name = name.to_string();
        }
        if project.note.is_some() {
            snapshot.project.note = project.note.clone();
        }
    }

    let known_groups: std::collections::HashSet<&str> =
        annotations.groups.iter().map(|group| group.id.as_str()).collect();

    let mut seen_group_ids = std::collections::HashSet::new();
    snapshot.groups = annotations
        .groups
        .iter()
        // A duplicate id would emit two TableGroup blocks with the same
        // name, which is invalid DBML. First occurrence wins.
        .filter(|group| seen_group_ids.insert(group.id.as_str()))
        .map(|group| TableGroup {
            id: group.id.clone(),
            name: group.name.clone(),
            hue: group.hue,
            note: group.note.clone(),
        })
        .collect();

    for table in &mut snapshot.tables {
        let key = table_key(db_type, table.schema.as_deref(), &table.name);
        let Some(annotation) = annotations.tables.get(&key) else { continue };

        if let Some(note) = annotation.note.as_deref().filter(|value| !value.trim().is_empty()) {
            // Preserve whatever the database said before overwriting it.
            if matches!(table.note_source, NoteSource::Database) {
                table.shadowed_note = table.note.clone();
            }
            table.note = Some(note.to_string());
            table.note_source = NoteSource::Local;
        }

        // A group reference that names no defined group is dropped rather
        // than assigned — a dangling id would render an empty group header.
        table.group_id = annotation.group.as_deref().filter(|id| known_groups.contains(id)).map(ToOwned::to_owned);

        for column in &table.columns {
            let column_fold = fold_identifier(db_type, &column.name);
            let annotated = annotation.columns.iter().find(|(name, _)| fold_identifier(db_type, name) == column_fold);

            let Some((_, column_annotation)) = annotated else { continue };
            if column_annotation.note.trim().is_empty() {
                continue;
            }

            table.column_notes.insert(
                column.name.clone(),
                ColumnNote {
                    note: column_annotation.note.clone(),
                    source: NoteSource::Local,
                    shadowed: column.comment.clone().filter(|value| !value.trim().is_empty()),
                },
            );
        }
    }

    let orphans = detect_orphans(snapshot, annotations, db_type);
    if !orphans.is_empty() {
        snapshot.warnings.push(SnapshotWarning::OrphanedNotes { count: orphans.len() });
    }
}

/// Annotation keys whose target no longer exists in the collected schema.
///
/// Returns fully-qualified keys, sorted, so the caller can list them for a
/// human to re-map. This function NEVER mutates the notes file — user prose
/// is only ever removed by an explicit human action.
///
/// Suggestions for where a renamed target went are deliberately absent:
/// producing them requires the OLD schema to diff against, and the notes
/// file stores prose only. That becomes possible once snapshot history
/// exists (see the spec's deferred versioning seam).
pub fn detect_orphans(snapshot: &SchemaSnapshot, annotations: &AnnotationFile, db_type: DatabaseType) -> Vec<String> {
    use std::collections::HashSet;

    let live_tables: HashSet<String> =
        snapshot.tables.iter().map(|table| table_key(db_type, table.schema.as_deref(), &table.name)).collect();

    let live_columns: HashSet<String> = snapshot
        .tables
        .iter()
        .flat_map(|table| {
            table
                .columns
                .iter()
                .map(move |column| column_key(db_type, table.schema.as_deref(), &table.name, &column.name))
        })
        .collect();

    let mut orphans = Vec::new();

    for (key, annotation) in &annotations.tables {
        let folded_table = fold_key(db_type, key);
        if !live_tables.contains(&folded_table) {
            orphans.push(folded_table);
            continue;
        }
        for column in annotation.columns.keys() {
            let folded_column = format!("{folded_table}.{}", fold_identifier(db_type, column));
            if !live_columns.contains(&folded_column) {
                orphans.push(folded_column);
            }
        }
    }

    orphans.sort();
    orphans
}

/// Fold an already-dotted key (e.g. `Core.Orders`) segment by segment.
fn fold_key(db_type: DatabaseType, key: &str) -> String {
    key.split('.').map(|segment| fold_identifier(db_type, segment)).collect::<Vec<_>>().join(".")
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

    fn temp_case_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dbx-notes-{label}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_case_dir("round-trip");
        let path = dir.join("notes.json");
        let file = AnnotationFile {
            format_version: 1,
            project: Some(ProjectAnnotation { name: Some("P".into()), note: Some("hello".into()) }),
            groups: vec![GroupAnnotation { id: "g".into(), name: "G".into(), hue: 200, note: None }],
            tables: BTreeMap::from([(
                "public.t".to_string(),
                TableAnnotation { group: Some("g".into()), note: Some("n".into()), columns: BTreeMap::new() },
            )]),
        };

        save_annotations(&path, &file).expect("save");
        let loaded = load_annotations(&path).expect("load").expect("present");

        assert_eq!(loaded.format_version, 1);
        assert_eq!(loaded.groups[0].hue, 200);
        assert_eq!(loaded.tables["public.t"].note.as_deref(), Some("n"));
        assert_eq!(loaded.project.and_then(|p| p.note).as_deref(), Some("hello"));
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = temp_case_dir("nested");
        let path = dir.join("nested").join("deeper").join("notes.json");
        let file = AnnotationFile { format_version: 1, project: None, groups: Vec::new(), tables: BTreeMap::new() };
        save_annotations(&path, &file).expect("save into a new directory");
        assert!(path.exists());
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        // A stray notes.json.tmp would be picked up by nothing, but it means
        // the rename did not happen and the write was not atomic.
        let dir = temp_case_dir("no-temp-left");
        let path = dir.join("notes.json");
        let file = AnnotationFile { format_version: 1, project: None, groups: Vec::new(), tables: BTreeMap::new() };
        save_annotations(&path, &file).expect("save");

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
    }

    #[test]
    fn a_failed_save_leaves_the_previous_file_intact() {
        // The reason for writing to a temp file and renaming. If the write
        // fails partway, the reader must still find the last good notes —
        // load_annotations errors loudly on malformed JSON, so a torn write
        // would read as "your notes file is corrupt".
        let dir = temp_case_dir("failed-save");
        let path = dir.join("notes.json");
        let good = AnnotationFile {
            format_version: 1,
            project: None,
            groups: Vec::new(),
            tables: BTreeMap::from([(
                "public.keep".to_string(),
                TableAnnotation { group: None, note: Some("survives".into()), columns: BTreeMap::new() },
            )]),
        };
        save_annotations(&path, &good).expect("first save");

        // A directory where the temp file wants to be makes File::create fail.
        std::fs::create_dir(path.with_extension("json.tmp")).expect("block the temp path");
        let result = save_annotations(&path, &good);

        assert!(result.is_err(), "save must fail when the temp path is unusable");
        let loaded = load_annotations(&path).expect("load").expect("present");
        assert_eq!(loaded.tables["public.keep"].note.as_deref(), Some("survives"));
    }

    #[test]
    fn an_explicit_notes_path_wins_over_the_default() {
        let resolved = resolve_notes_path("conn-1", Some("/tmp/team/schema-notes.json"), std::path::Path::new("/data"));
        assert_eq!(resolved, std::path::PathBuf::from("/tmp/team/schema-notes.json"));
    }

    #[test]
    fn the_default_notes_path_is_keyed_by_connection_id() {
        let resolved = resolve_notes_path("conn-1", None, std::path::Path::new("/data"));
        assert_eq!(resolved, std::path::PathBuf::from("/data/docs-notes/conn-1.json"));
    }

    #[test]
    fn a_blank_notes_path_falls_back_to_the_default() {
        // An empty string in the config is a cleared field, not a path to a
        // file named "". Treating it as explicit would resolve to garbage.
        let resolved = resolve_notes_path("conn-1", Some("   "), std::path::Path::new("/data"));
        assert_eq!(resolved, std::path::PathBuf::from("/data/docs-notes/conn-1.json"));
    }

    #[test]
    fn an_absent_file_is_not_an_error() {
        let missing = std::path::Path::new("/nonexistent/dbx-notes-does-not-exist.json");
        assert!(matches!(load_annotations(missing), Ok(None)));
    }

    #[test]
    fn a_valid_file_loads() {
        let path = temp_notes(SAMPLE);
        let loaded = load_annotations(&path);
        let _ = std::fs::remove_file(&path);
        let loaded = loaded.expect("load").expect("some");
        assert_eq!(loaded.tables.len(), 1);
    }

    #[test]
    fn a_malformed_file_is_a_hard_error_naming_the_path() {
        let path = temp_notes("{ this is not json");
        let error = load_annotations(&path);
        let _ = std::fs::remove_file(&path);
        let error = error.expect_err("must fail");
        assert!(error.contains(&path.display().to_string()), "error must name the file: {error}");
    }

    #[test]
    fn an_unsupported_format_version_is_rejected() {
        let path = temp_notes(r#"{"formatVersion": 99}"#);
        let error = load_annotations(&path);
        let _ = std::fs::remove_file(&path);
        let error = error.expect_err("must fail");
        assert!(error.contains("99"), "error must name the version: {error}");
    }

    #[test]
    fn a_future_format_version_reports_the_version_not_an_unknown_field() {
        // deny_unknown_fields must NOT pre-empt the version check — a v1
        // build reading a v2 file has to say so, or the version field is
        // useless exactly when it is needed.
        let path = temp_notes(r#"{"formatVersion": 2, "someNewField": {"a": 1}}"#);
        let error = load_annotations(&path);
        let _ = std::fs::remove_file(&path);
        let error = error.expect_err("must fail");

        assert!(error.contains("formatVersion 2"), "should name the version, got: {error}");
        assert!(!error.contains("unknown field"), "should not surface a raw serde error, got: {error}");
    }

    use crate::docs::{DocTable, NoteSource, ProjectMeta, SchemaSnapshot, TableKind};
    use crate::models::connection::DatabaseType;

    fn snapshot_with(tables: Vec<DocTable>) -> SchemaSnapshot {
        SchemaSnapshot {
            format_version: 1,
            project: ProjectMeta {
                name: "conn".to_string(),
                database_type: "postgres".to_string(),
                database: None,
                schemas: vec!["core".to_string()],
                generated_at: String::new(),
                note: None,
            },
            tables,
            relationships: vec![],
            groups: vec![],
            enums: vec![],
            warnings: vec![],
        }
    }

    fn table_named(schema: &str, name: &str, comment: Option<&str>) -> DocTable {
        DocTable {
            schema: Some(schema.to_string()),
            name: name.to_string(),
            kind: TableKind::Table,
            columns: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            group_id: None,
            note: comment.map(ToOwned::to_owned),
            note_source: if comment.is_some() { NoteSource::Database } else { NoteSource::None },
            shadowed_note: None,
            column_notes: BTreeMap::new(),
            estimated_rows: None,
            view_definition: None,
        }
    }

    #[test]
    fn a_local_note_shadows_the_database_comment_and_preserves_it() {
        let mut snapshot = snapshot_with(vec![table_named("core", "orders", Some("Old DB comment."))]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        let table = &snapshot.tables[0];
        assert_eq!(table.note.as_deref(), Some("One row per checkout."));
        assert_eq!(table.note_source, NoteSource::Local);
        assert_eq!(table.shadowed_note.as_deref(), Some("Old DB comment."));
    }

    #[test]
    fn a_database_comment_survives_when_there_is_no_local_note() {
        let mut snapshot = snapshot_with(vec![table_named("core", "users", Some("From the database."))]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        let table = &snapshot.tables[0];
        assert_eq!(table.note.as_deref(), Some("From the database."));
        assert_eq!(table.note_source, NoteSource::Database);
        assert_eq!(table.shadowed_note, None);
    }

    #[test]
    fn keys_match_case_insensitively_on_postgres() {
        // The notes file says "core.orders"; the live schema reports "Core"/"Orders".
        let mut snapshot = snapshot_with(vec![table_named("Core", "Orders", None)]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        assert_eq!(snapshot.tables[0].note.as_deref(), Some("One row per checkout."));
    }

    #[test]
    fn column_notes_are_applied_and_marked_local() {
        let mut table = table_named("core", "orders", None);
        table.columns.push(crate::types::ColumnInfo {
            name: "status".to_string(),
            data_type: "text".to_string(),
            ..Default::default()
        });
        let mut snapshot = snapshot_with(vec![table]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        let note = snapshot.tables[0].column_notes.get("status").expect("column note");
        assert_eq!(note.note, "State machine.");
        assert_eq!(note.source, NoteSource::Local);
    }

    #[test]
    fn the_project_note_and_name_are_applied() {
        let mut snapshot = snapshot_with(vec![]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        assert_eq!(snapshot.project.name, "Ecommerce");
        assert_eq!(snapshot.project.note.as_deref(), Some("# Overview"));
    }

    #[test]
    fn groups_are_copied_and_membership_is_assigned() {
        let mut snapshot = snapshot_with(vec![table_named("core", "orders", None)]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        assert_eq!(snapshot.groups.len(), 1);
        assert_eq!(snapshot.groups[0].id, "order-management");
        assert_eq!(snapshot.groups[0].hue, 28);
        assert_eq!(snapshot.tables[0].group_id.as_deref(), Some("order-management"));
    }

    #[test]
    fn a_table_referencing_an_undefined_group_is_left_ungrouped() {
        let mut snapshot = snapshot_with(vec![table_named("core", "orders", None)]);
        let mut annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");
        annotations.groups.clear();

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        assert_eq!(snapshot.tables[0].group_id, None, "a dangling group reference must not be assigned");
    }

    #[test]
    fn duplicate_group_ids_collapse_to_one_entry() {
        // Two TableGroup blocks sharing an id is invalid DBML.
        let mut snapshot = snapshot_with(vec![table_named("core", "orders", None)]);
        let mut annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");
        let mut dup = annotations.groups[0].clone();
        dup.name = "Duplicate".to_string();
        annotations.groups.push(dup);

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);

        assert_eq!(snapshot.groups.len(), 1, "duplicate ids must collapse");
        assert_eq!(snapshot.groups[0].name, "Order Management", "first occurrence wins");
    }

    use crate::docs::SnapshotWarning;

    #[test]
    fn a_note_for_a_missing_table_is_reported_as_orphaned() {
        // The notes file describes core.orders; the schema no longer has it.
        let mut snapshot = snapshot_with(vec![table_named("core", "customers", None)]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        let orphans = detect_orphans(&snapshot, &annotations, DatabaseType::Postgres);
        assert_eq!(orphans, vec!["core.orders".to_string()]);

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);
        let orphan_warnings: Vec<&SnapshotWarning> = snapshot
            .warnings
            .iter()
            .filter(|warning| matches!(warning, SnapshotWarning::OrphanedNotes { .. }))
            .collect();
        assert_eq!(orphan_warnings.len(), 1);
        match orphan_warnings[0] {
            SnapshotWarning::OrphanedNotes { count } => assert_eq!(*count, 1),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn a_note_for_a_missing_column_is_reported_as_orphaned() {
        // The table exists but no longer has the annotated column.
        let mut table = table_named("core", "orders", None);
        table.columns.push(crate::types::ColumnInfo {
            name: "id".to_string(),
            data_type: "integer".to_string(),
            ..Default::default()
        });
        let snapshot = snapshot_with(vec![table]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        let orphans = detect_orphans(&snapshot, &annotations, DatabaseType::Postgres);
        assert_eq!(orphans, vec!["core.orders.status".to_string()]);
    }

    #[test]
    fn nothing_is_orphaned_when_everything_matches() {
        let mut table = table_named("core", "orders", None);
        table.columns.push(crate::types::ColumnInfo {
            name: "status".to_string(),
            data_type: "text".to_string(),
            ..Default::default()
        });
        let mut snapshot = snapshot_with(vec![table]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");

        assert!(detect_orphans(&snapshot, &annotations, DatabaseType::Postgres).is_empty());

        apply_annotations(&mut snapshot, &annotations, DatabaseType::Postgres);
        assert!(
            !snapshot.warnings.iter().any(|w| matches!(w, SnapshotWarning::OrphanedNotes { .. })),
            "no orphan warning when everything matches"
        );
    }

    #[test]
    fn orphan_detection_never_removes_anything_from_the_file() {
        let snapshot = snapshot_with(vec![]);
        let annotations: AnnotationFile = serde_json::from_str(SAMPLE).expect("parse");
        let before = annotations.tables.len();

        let _ = detect_orphans(&snapshot, &annotations, DatabaseType::Postgres);

        assert_eq!(annotations.tables.len(), before, "detection must not mutate the file");
    }
}
