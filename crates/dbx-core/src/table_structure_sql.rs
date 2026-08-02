mod column_alter;
mod column_format;
mod columns;
mod comments;
mod create_table;
mod dialect;
mod foreign_keys;
mod indexes;
mod sqlite_rebuild;
mod triggers;
mod types;
mod util;
mod validation;

#[cfg(test)]
mod tests;

pub use column_alter::build_single_column_alter_sql;
pub use create_table::build_create_table_sql;
pub use sqlite_rebuild::{apply_sqlite_table_structure_change, preview_sqlite_table_structure_change};
pub use types::*;

use columns::build_column_sql;
use comments::build_table_comment_sql;
use foreign_keys::build_foreign_key_sql;
use indexes::build_index_sql;
use triggers::build_trigger_sql;
use validation::validate_draft;

pub fn build_table_structure_change_sql(options: TableStructureSqlOptions) -> TableStructureSqlResult {
    let mut warnings = validate_draft(&options);
    let mut statements = build_column_sql(&options, &mut warnings);
    statements.extend(build_index_sql(&options, &mut warnings));
    statements.extend(build_foreign_key_sql(&options, &mut warnings));
    statements.extend(build_trigger_sql(&options, &mut warnings));
    statements.extend(build_table_comment_sql(&options, &mut warnings));
    TableStructureSqlResult { statements, warnings }
}

/// Whether this engine supports COMMENT ON for tables and columns. The
/// documentation collector uses this to warn when DBX's own annotation
/// layer is the only possible source of prose.
pub(crate) fn supports_comments(database_type: crate::models::connection::DatabaseType) -> bool {
    dialect::capabilities_for(Some(database_type)).comment
}

/// Whether this engine reports foreign key metadata at all. Engines like
/// ClickHouse and Doris do not, so their ER diagrams have no edges by
/// necessity rather than by accident.
pub(crate) fn supports_foreign_keys(database_type: crate::models::connection::DatabaseType) -> bool {
    dialect::capabilities_for(Some(database_type)).foreign_key
}

/// Canonical display label for a database engine (e.g. `postgres`,
/// `sqlserver`, `mongodb`) — the same identifier already used throughout
/// this module's own warning prose. The documentation collector uses it for
/// `database_type` and its engine-capability warnings instead of the Rust
/// `Debug` spelling (`Postgres`, `SqlServer`, `MongoDb`), which is an
/// implementation detail, not something a consumer like dbdocs/dbdiagram
/// should key off.
pub(crate) fn database_type_label(database_type: crate::models::connection::DatabaseType) -> String {
    dialect::database_label(Some(database_type))
}
