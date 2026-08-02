use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use tokio::sync::Semaphore;

use crate::connection::AppState;
use crate::docs::{
    build_relationships, DocEnum, DocTable, NoteSource, ProjectMeta, SchemaSnapshot, SnapshotWarning, TableKind,
};
use crate::models::connection::ConnectionConfig;
use crate::schema;
use crate::table_structure_sql::{supports_comments, supports_foreign_keys};
use crate::types::ColumnInfo;

/// Concurrent per-table metadata fetches. Bounded so documenting a large
/// schema cannot starve the connection pool the UI is also using.
const MAX_CONCURRENT_TABLES: usize = 8;

#[derive(Debug, Clone)]
pub struct CollectOptions {
    pub database: String,
    pub schemas: Vec<String>,
    /// Empty means every table. Entries may be bare (`orders`) or
    /// qualified (`analytics.daily_sales`).
    pub tables: Vec<String>,
    pub project_name: String,
}

impl CollectOptions {
    pub fn includes_table(&self, schema: &str, table: &str) -> bool {
        if self.tables.is_empty() {
            return true;
        }
        let qualified = format!("{schema}.{table}");
        self.tables.iter().any(|wanted| wanted == table || wanted == &qualified)
    }
}

#[derive(Debug, Clone)]
pub struct CollectProgress {
    pub completed: usize,
    pub total: usize,
    pub current: String,
}

fn table_kind_from(table_type: &str) -> TableKind {
    let normalized = table_type.trim().to_ascii_uppercase().replace('_', " ");
    match normalized.as_str() {
        "VIEW" => TableKind::View,
        "MATERIALIZED VIEW" => TableKind::MaterializedView,
        _ => TableKind::Table,
    }
}

/// MySQL reports `ENUM('a','b')` inline rather than as a named type.
/// DBML needs a named enum, so synthesize one per table+column.
fn synthesize_enum(schema: Option<&str>, table: &str, column: &ColumnInfo) -> Option<DocEnum> {
    let values = column.enum_values.as_ref().filter(|values| !values.is_empty())?;
    Some(DocEnum {
        schema: schema.map(ToOwned::to_owned),
        name: format!("{table}_{}", column.name),
        values: values.clone(),
        note: None,
        synthesized: true,
    })
}

fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

/// Collect a documentation snapshot.
///
/// A per-table failure is recorded as a `TableSkipped` warning and does not
/// abort the run — a permissions gap on one table must not kill a
/// 400-table documentation build.
pub async fn collect_snapshot(
    state: &AppState,
    connection: &ConnectionConfig,
    options: &CollectOptions,
    progress: &(dyn Fn(CollectProgress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<SchemaSnapshot, String> {
    let mut warnings: Vec<SnapshotWarning> = Vec::new();
    let engine = format!("{:?}", connection.db_type);
    let connection_id = connection.id.as_str();

    if !supports_comments(connection.db_type) {
        warnings.push(SnapshotWarning::CommentsUnsupported { engine: engine.clone() });
    }
    if !supports_foreign_keys(connection.db_type) {
        warnings.push(SnapshotWarning::NoForeignKeyMetadata { engine: engine.clone() });
    }

    let schemas = if options.schemas.is_empty() {
        schema::list_schemas_core(state, connection_id, &options.database).await.unwrap_or_default()
    } else {
        options.schemas.clone()
    };
    let effective_schemas = if schemas.is_empty() { vec![String::new()] } else { schemas };

    // Enumerate every table first so progress has a real total.
    let mut targets: Vec<(String, crate::types::TableInfo)> = Vec::new();
    for schema_name in &effective_schemas {
        match schema::list_tables_core(
            state,
            connection_id,
            &options.database,
            schema_name,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        {
            Ok(tables) => {
                for info in tables {
                    if options.includes_table(schema_name, &info.name) {
                        targets.push((schema_name.clone(), info));
                    }
                }
            }
            Err(error) => {
                warnings.push(SnapshotWarning::TableSkipped { table: format!("{schema_name}.*"), reason: error })
            }
        }
    }

    let total = targets.len();
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_TABLES));

    let collected: Vec<Result<(DocTable, Vec<SnapshotWarning>), SnapshotWarning>> =
        stream::iter(targets.into_iter().enumerate())
            .map(|(index, (schema_name, info))| {
                let semaphore = Arc::clone(&semaphore);
                let database = options.database.clone();
                async move {
                    let _permit = semaphore.acquire().await.map_err(|_| SnapshotWarning::TableSkipped {
                        table: info.name.clone(),
                        reason: "collection cancelled".to_string(),
                    })?;

                    if cancelled(cancel) {
                        return Err(SnapshotWarning::TableSkipped {
                            table: info.name.clone(),
                            reason: "cancelled".to_string(),
                        });
                    }

                    progress(CollectProgress {
                        completed: index,
                        total,
                        current: format!("{schema_name}.{}", info.name),
                    });

                    let columns = schema::get_columns_core(state, connection_id, &database, &schema_name, &info.name)
                        .await
                        .map_err(|error| SnapshotWarning::TableSkipped {
                            table: format!("{schema_name}.{}", info.name),
                            reason: error,
                        })?;

                    // Indexes degrade to empty rather than failing the table.
                    let indexes = schema::list_indexes_core(state, connection_id, &database, &schema_name, &info.name)
                        .await
                        .unwrap_or_default();

                    // Foreign keys also degrade to empty rather than failing the
                    // table, but a real query failure must not look identical to
                    // "this table genuinely has no foreign keys" — it is reported
                    // as its own warning instead of being silently discarded.
                    let mut table_warnings = Vec::new();
                    let foreign_keys =
                        match schema::list_foreign_keys_core(state, connection_id, &database, &schema_name, &info.name)
                            .await
                        {
                            Ok(keys) => keys,
                            Err(error) => {
                                table_warnings.push(SnapshotWarning::TableSkipped {
                                    table: format!("{schema_name}.{}", info.name),
                                    reason: format!("foreign keys unavailable: {error}"),
                                });
                                Vec::new()
                            }
                        };

                    Ok((
                        DocTable {
                            schema: (!schema_name.is_empty()).then(|| schema_name.clone()),
                            name: info.name.clone(),
                            kind: table_kind_from(&info.table_type),
                            columns,
                            indexes,
                            foreign_keys,
                            group_id: None,
                            note: info.comment.clone().filter(|value| !value.trim().is_empty()),
                            note_source: if info.comment.as_deref().is_some_and(|v| !v.trim().is_empty()) {
                                NoteSource::Database
                            } else {
                                NoteSource::None
                            },
                            shadowed_note: None,
                            column_notes: BTreeMap::new(),
                            estimated_rows: None,
                            view_definition: None,
                        },
                        table_warnings,
                    ))
                }
            })
            .buffer_unordered(MAX_CONCURRENT_TABLES)
            .collect()
            .await;

    let mut tables = Vec::new();
    for outcome in collected {
        match outcome {
            Ok((table, table_warnings)) => {
                tables.push(table);
                warnings.extend(table_warnings);
            }
            Err(warning) => warnings.push(warning),
        }
    }

    tables.sort_by(|a, b| a.qualified_name().cmp(&b.qualified_name()));

    let mut enums: Vec<DocEnum> = Vec::new();
    for table in &tables {
        for column in &table.columns {
            if let Some(value) = synthesize_enum(table.schema.as_deref(), &table.name, column) {
                enums.push(value);
            }
        }
    }

    let relationships = build_relationships(&tables);

    progress(CollectProgress { completed: total, total, current: String::new() });

    Ok(SchemaSnapshot {
        format_version: 1,
        project: ProjectMeta {
            name: options.project_name.clone(),
            database_type: engine,
            database: (!options.database.is_empty()).then(|| options.database.clone()),
            schemas: effective_schemas.into_iter().filter(|s| !s.is_empty()).collect(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            note: None,
        },
        tables,
        relationships,
        groups: Vec::new(),
        enums,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_kind_maps_from_the_engine_reported_type() {
        assert_eq!(table_kind_from("TABLE"), TableKind::Table);
        assert_eq!(table_kind_from("BASE TABLE"), TableKind::Table);
        assert_eq!(table_kind_from("VIEW"), TableKind::View);
        assert_eq!(table_kind_from("MATERIALIZED VIEW"), TableKind::MaterializedView);
        assert_eq!(table_kind_from("materialized_view"), TableKind::MaterializedView);
        assert_eq!(table_kind_from("something else"), TableKind::Table);
    }

    #[test]
    fn table_filter_is_empty_means_include_everything() {
        let options = CollectOptions {
            database: "shop".to_string(),
            schemas: vec!["public".to_string()],
            tables: vec![],
            project_name: "Ecommerce".to_string(),
        };
        assert!(options.includes_table("public", "orders"));
        assert!(options.includes_table("public", "anything"));
    }

    #[test]
    fn table_filter_matches_bare_and_qualified_names() {
        let options = CollectOptions {
            database: "shop".to_string(),
            schemas: vec!["public".to_string()],
            tables: vec!["orders".to_string(), "analytics.daily_sales".to_string()],
            project_name: "Ecommerce".to_string(),
        };
        assert!(options.includes_table("public", "orders"));
        assert!(options.includes_table("analytics", "daily_sales"));
        assert!(!options.includes_table("public", "users"));
        assert!(!options.includes_table("public", "daily_sales"));
    }

    #[test]
    fn synthesises_a_named_enum_from_an_inline_enum_column() {
        let mut column = crate::types::ColumnInfo {
            name: "status".to_string(),
            data_type: "enum".to_string(),
            ..Default::default()
        };
        column.enum_values = Some(vec!["pending".to_string(), "shipped".to_string()]);

        let synthesized = synthesize_enum(Some("public"), "orders", &column).expect("enum");

        assert_eq!(synthesized.name, "orders_status");
        assert_eq!(synthesized.values, vec!["pending", "shipped"]);
        assert!(synthesized.synthesized);
    }

    #[test]
    fn a_column_without_enum_values_synthesises_nothing() {
        let column = crate::types::ColumnInfo {
            name: "status".to_string(),
            data_type: "text".to_string(),
            ..Default::default()
        };
        assert!(synthesize_enum(Some("public"), "orders", &column).is_none());
    }

    #[test]
    fn foreign_key_warning_depends_on_engine_capability_not_on_relationship_count() {
        // A capable engine must NOT be reported as lacking FK metadata just
        // because the schema happens to have no relationships.
        assert!(supports_foreign_keys(crate::models::connection::DatabaseType::Postgres));
    }
}
