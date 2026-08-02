use crate::types::ColumnInfo;

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

/// Notes always use triple quotes so an apostrophe in prose can never break
/// the file. Only a literal `'''` needs escaping.
pub(crate) fn render_note(value: &str) -> String {
    format!("'''{}'''", value.replace("'''", "\\'''"))
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
}
