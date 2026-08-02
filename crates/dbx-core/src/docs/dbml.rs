use crate::docs::{DocTable, SnapshotWarning};
use crate::types::{ColumnInfo, IndexInfo};

/// DBML accepts bare identifiers matching `[A-Za-z_][A-Za-z0-9_]*`;
/// everything else needs double quotes.
pub(crate) fn quote_identifier(value: &str) -> String {
    let plain = !value.is_empty()
        && value.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');

    if plain {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

/// `schema.name` when qualifying, bare `name` otherwise. Both parts quoted
/// independently so an irregular schema or table name is handled correctly.
pub(crate) fn qualified(schema: Option<&str>, name: &str, qualify: bool) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(schema) if qualify => format!("{}.{}", quote_identifier(schema), quote_identifier(name)),
        _ => quote_identifier(name),
    }
}

/// Notes always use triple quotes so an apostrophe in prose can never break
/// the file.
///
/// Two things need escaping. A literal `'''` inside the prose, obviously —
/// and also a SINGLE trailing quote, which is subtler: the note `he said '`
/// would otherwise render as `'''he said ''''`, four quotes in a row, and a
/// parser scanning for the closing `'''` would close early and strand the
/// fourth. DBML honours `\'` inside triple quotes.
pub(crate) fn render_note(value: &str) -> String {
    let escaped = value.replace("'''", "\\'''");
    let escaped = match escaped.strip_suffix('\'') {
        Some(head) => format!("{head}\\'"),
        None => escaped,
    };
    format!("'''{escaped}'''")
}

fn looks_like_expression(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2 {
        return false;
    }
    if trimmed.parse::<f64>().is_ok() {
        return false;
    }
    if matches!(trimmed.to_ascii_lowercase().as_str(), "true" | "false" | "null") {
        return false;
    }
    true
}

/// `default: …` for a column, or None when it has no default.
/// Expressions go in backticks, literals stay as written.
pub(crate) fn render_default(column: &ColumnInfo) -> Option<String> {
    let value = column.column_default.as_deref()?.trim();
    if value.is_empty() {
        return None;
    }

    if looks_like_expression(value) {
        Some(format!("default: `{value}`"))
    } else {
        Some(format!("default: {value}"))
    }
}

/// DBML does not validate type names, so native types pass through intact.
/// Precision is reconstructed only when the engine reported a bare type.
pub(crate) fn render_type(column: &ColumnInfo) -> String {
    let base = column.data_type.trim();
    if base.contains('(') {
        return base.to_string();
    }

    if let Some(length) = column.character_maximum_length.filter(|value| *value > 0) {
        return format!("{base}({length})");
    }

    if let Some(precision) = column.numeric_precision.filter(|value| *value > 0) {
        return match column.numeric_scale {
            Some(scale) if scale > 0 => format!("{base}({precision},{scale})"),
            _ => format!("{base}({precision})"),
        };
    }

    base.to_string()
}

fn column_settings(column: &ColumnInfo, table: &DocTable) -> Vec<String> {
    let mut settings = Vec::new();

    if column.is_primary_key {
        settings.push("pk".to_string());
    }

    let extra = column.extra.as_deref().unwrap_or("").to_ascii_lowercase();
    if extra.contains("auto_increment") || extra.contains("identity") {
        settings.push("increment".to_string());
    }

    if !column.is_nullable && !column.is_primary_key {
        settings.push("not null".to_string());
    }

    if let Some(default) = render_default(column) {
        settings.push(default);
    }

    if let Some(note) = table.column_notes.get(&column.name) {
        settings.push(format!("note: {}", render_note(&note.note)));
    } else if let Some(comment) = column.comment.as_deref().filter(|value| !value.trim().is_empty()) {
        settings.push(format!("note: {}", render_note(comment)));
    }

    settings
}

fn render_index(index: &IndexInfo) -> String {
    let columns = index.columns.iter().map(|c| quote_identifier(c)).collect::<Vec<_>>().join(", ");

    let mut settings = vec![format!("name: '{}'", index.name.replace('\'', "\\'"))];
    if index.is_unique {
        settings.push("unique".to_string());
    }

    format!("    ({columns}) [{}]\n", settings.join(", "))
}

/// Render one `Table` block.
///
/// The primary-key index is skipped because columns already carry `pk`.
/// Indexes DBML cannot express are skipped and recorded in `warnings`
/// rather than dropped silently.
pub(crate) fn render_table(table: &DocTable, qualify: bool, warnings: &mut Vec<SnapshotWarning>) -> String {
    let name = qualified(table.schema.as_deref(), &table.name, qualify);

    let mut out = format!("Table {name} {{\n");

    for column in &table.columns {
        let settings = column_settings(column, table);
        let rendered_settings = if settings.is_empty() { String::new() } else { format!(" [{}]", settings.join(", ")) };
        out.push_str(&format!("  {} {}{}\n", quote_identifier(&column.name), render_type(column), rendered_settings));
    }

    let emittable: Vec<&IndexInfo> = table
        .indexes
        .iter()
        .filter(|index| {
            if index.is_primary {
                return false;
            }
            if index.filter.as_deref().is_some_and(|f| !f.trim().is_empty()) {
                warnings.push(SnapshotWarning::DbmlOmitted {
                    table: table.qualified_name(),
                    item: index.name.clone(),
                    reason: "partial index filter has no DBML equivalent".to_string(),
                });
                return false;
            }
            if index.included_columns.as_ref().is_some_and(|columns| !columns.is_empty()) {
                warnings.push(SnapshotWarning::DbmlOmitted {
                    table: table.qualified_name(),
                    item: index.name.clone(),
                    reason: "included columns have no DBML equivalent".to_string(),
                });
                return false;
            }
            !index.columns.is_empty()
        })
        .collect();

    if !emittable.is_empty() {
        out.push_str("\n  Indexes {\n");
        for index in emittable {
            out.push_str(&render_index(index));
        }
        out.push_str("  }\n");
    }

    if let Some(note) = table.note.as_deref().filter(|value| !value.trim().is_empty()) {
        out.push_str(&format!("\n  Note: {}\n", render_note(note)));
    }

    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ColumnInfo;

    fn col(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo { name: name.to_string(), data_type: data_type.to_string(), ..ColumnInfo::default() }
    }

    #[test]
    fn plain_identifiers_are_not_quoted() {
        assert_eq!(quote_identifier("orders"), "orders");
        assert_eq!(quote_identifier("order_items"), "order_items");
        assert_eq!(quote_identifier("_private"), "_private");
        assert_eq!(quote_identifier("t2"), "t2");
    }

    #[test]
    fn irregular_identifiers_are_quoted() {
        assert_eq!(quote_identifier("order items"), "\"order items\"");
        assert_eq!(quote_identifier("2fast"), "\"2fast\"");
        assert_eq!(quote_identifier("user-profile"), "\"user-profile\"");
        assert_eq!(quote_identifier(""), "\"\"");
    }

    #[test]
    fn embedded_double_quotes_are_escaped() {
        assert_eq!(quote_identifier("we\"ird"), "\"we\\\"ird\"");
    }

    #[test]
    fn notes_always_use_triple_quotes_so_apostrophes_are_safe() {
        assert_eq!(render_note("Bob's orders"), "'''Bob's orders'''");
    }

    #[test]
    fn notes_escape_a_literal_triple_quote() {
        assert_eq!(render_note("a ''' b"), "'''a \\''' b'''");
    }

    #[test]
    fn multiline_notes_are_preserved() {
        assert_eq!(render_note("line one\nline two"), "'''line one\nline two'''");
    }

    #[test]
    fn a_note_ending_in_a_single_quote_does_not_break_the_delimiter() {
        // Without escaping this yields four quotes in a row and a parser
        // closes the string early.
        assert_eq!(render_note("Deprecated in '24'"), "'''Deprecated in '24\\''''");
    }

    #[test]
    fn expression_defaults_use_backticks_and_literals_use_quotes() {
        let mut expression = col("created_at", "timestamptz");
        expression.column_default = Some("now()".to_string());
        assert_eq!(render_default(&expression).as_deref(), Some("default: `now()`"));

        let mut sequence = col("id", "integer");
        sequence.column_default = Some("nextval('orders_id_seq'::regclass)".to_string());
        assert_eq!(render_default(&sequence).as_deref(), Some("default: `nextval('orders_id_seq'::regclass)`"));

        let mut literal = col("status", "text");
        literal.column_default = Some("'pending'".to_string());
        assert_eq!(render_default(&literal).as_deref(), Some("default: 'pending'"));

        let mut number = col("qty", "integer");
        number.column_default = Some("0".to_string());
        assert_eq!(render_default(&number).as_deref(), Some("default: 0"));

        let mut boolean = col("active", "boolean");
        boolean.column_default = Some("true".to_string());
        assert_eq!(render_default(&boolean).as_deref(), Some("default: true"));

        assert_eq!(render_default(&col("plain", "text")), None);
    }

    #[test]
    fn types_pass_through_verbatim_when_already_parameterised() {
        assert_eq!(render_type(&col("total", "numeric(10,2)")), "numeric(10,2)");
        assert_eq!(render_type(&col("meta", "jsonb")), "jsonb");
        assert_eq!(render_type(&col("at", "timestamp with time zone")), "timestamp with time zone");
    }

    #[test]
    fn bare_types_are_reconstructed_from_precision_metadata() {
        let mut varchar = col("email", "character varying");
        varchar.character_maximum_length = Some(255);
        assert_eq!(render_type(&varchar), "character varying(255)");

        let mut decimal = col("total", "numeric");
        decimal.numeric_precision = Some(10);
        decimal.numeric_scale = Some(2);
        assert_eq!(render_type(&decimal), "numeric(10,2)");

        let mut integer = col("count", "numeric");
        integer.numeric_precision = Some(8);
        integer.numeric_scale = Some(0);
        assert_eq!(render_type(&integer), "numeric(8)");
    }

    use crate::docs::{ColumnNote, DocTable, NoteSource, SnapshotWarning, TableKind};
    use crate::types::IndexInfo;
    use std::collections::BTreeMap;

    fn doc_table(name: &str, columns: Vec<ColumnInfo>, indexes: Vec<IndexInfo>) -> DocTable {
        DocTable {
            schema: Some("public".to_string()),
            name: name.to_string(),
            kind: TableKind::Table,
            columns,
            indexes,
            foreign_keys: vec![],
            group_id: None,
            note: None,
            note_source: NoteSource::None,
            shadowed_note: None,
            column_notes: BTreeMap::new(),
            estimated_rows: None,
            view_definition: None,
        }
    }

    #[test]
    fn renders_a_table_with_columns_and_settings() {
        let mut id = col("id", "integer");
        id.is_primary_key = true;
        id.extra = Some("auto_increment".to_string());

        let mut user_id = col("user_id", "integer");
        user_id.is_nullable = false;

        // ColumnInfo::default() gives is_nullable=false (i.e. NOT NULL), so a
        // genuinely nullable column has to say so explicitly.
        let mut nullable = col("shipped_at", "timestamptz");
        nullable.is_nullable = true;

        let mut table = doc_table("orders", vec![id, user_id, nullable], vec![]);
        table.note = Some("Checkout rows.".to_string());
        table.column_notes.insert(
            "user_id".to_string(),
            ColumnNote { note: "Owning customer".to_string(), source: NoteSource::Local, shadowed: None },
        );

        let mut warnings = Vec::new();
        let out = render_table(&table, false, &mut warnings);

        assert!(out.starts_with("Table orders {\n"), "got:\n{out}");
        assert!(out.contains("id integer [pk, increment]"), "got:\n{out}");
        assert!(out.contains("user_id integer [not null, note: '''Owning customer''']"), "got:\n{out}");
        assert!(out.contains("shipped_at timestamptz\n"), "got:\n{out}");
        assert!(out.contains("Note: '''Checkout rows.'''"), "got:\n{out}");
        assert!(out.ends_with("}\n"), "got:\n{out}");
    }

    #[test]
    fn qualifies_the_table_name_when_requested() {
        let table = doc_table("orders", vec![col("id", "integer")], vec![]);
        let mut warnings = Vec::new();
        assert!(render_table(&table, true, &mut warnings).starts_with("Table public.orders {"));
    }

    #[test]
    fn renders_an_indexes_block() {
        let index = IndexInfo {
            name: "idx_orders_user_placed".to_string(),
            columns: vec!["user_id".to_string(), "placed_at".to_string()],
            is_unique: false,
            is_primary: false,
            filter: None,
            index_type: None,
            included_columns: None,
            comment: None,
        };
        let table = doc_table("orders", vec![col("user_id", "integer")], vec![index]);

        let mut warnings = Vec::new();
        let out = render_table(&table, false, &mut warnings);

        assert!(out.contains("Indexes {"), "got:\n{out}");
        assert!(out.contains("(user_id, placed_at) [name: 'idx_orders_user_placed']"), "got:\n{out}");
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_unique_index_carries_the_unique_setting() {
        let index = IndexInfo {
            name: "uq_orders_ref".to_string(),
            columns: vec!["reference".to_string()],
            is_unique: true,
            is_primary: false,
            filter: None,
            index_type: None,
            included_columns: None,
            comment: None,
        };
        let table = doc_table("orders", vec![col("reference", "text")], vec![index]);
        let mut warnings = Vec::new();
        let out = render_table(&table, false, &mut warnings);
        assert!(out.contains("(reference) [name: 'uq_orders_ref', unique]"), "got:\n{out}");
    }

    #[test]
    fn the_primary_key_index_is_skipped_because_columns_already_carry_pk() {
        let index = IndexInfo {
            name: "orders_pkey".to_string(),
            columns: vec!["id".to_string()],
            is_unique: true,
            is_primary: true,
            filter: None,
            index_type: None,
            included_columns: None,
            comment: None,
        };
        let mut id = col("id", "integer");
        id.is_primary_key = true;
        let table = doc_table("orders", vec![id], vec![index]);

        let mut warnings = Vec::new();
        let out = render_table(&table, false, &mut warnings);
        assert!(!out.contains("orders_pkey"), "got:\n{out}");
    }

    #[test]
    fn a_filtered_index_is_omitted_and_warned_about() {
        let index = IndexInfo {
            name: "idx_orders_open".to_string(),
            columns: vec!["status".to_string()],
            is_unique: false,
            is_primary: false,
            filter: Some("status <> 'cancelled'".to_string()),
            index_type: None,
            included_columns: None,
            comment: None,
        };
        let table = doc_table("orders", vec![col("status", "text")], vec![index]);

        let mut warnings = Vec::new();
        let out = render_table(&table, false, &mut warnings);

        assert!(!out.contains("idx_orders_open"), "filtered index must not be emitted, got:\n{out}");
        assert_eq!(warnings.len(), 1);
        match &warnings[0] {
            SnapshotWarning::DbmlOmitted { table, item, reason } => {
                assert_eq!(table, "public.orders");
                assert_eq!(item, "idx_orders_open");
                assert!(reason.contains("filter"), "got {reason}");
            }
            other => panic!("unexpected warning: {other:?}"),
        }
    }
}
