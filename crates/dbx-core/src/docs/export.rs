use base64::Engine as _;

use crate::docs::annotations::AnnotationFile;
use crate::docs::snapshot::SchemaSnapshot;

/// The viewer bundle, built by `pnpm build:docs-export` and committed.
///
/// Committed rather than built by cargo because Rust cannot run Vite and
/// `dbx docs` must work from a plain `cargo install` on a machine with no
/// Node. `docs_export_bundle_is_current` is what keeps it honest.
const EXPORT_JS: &str = include_str!("../../assets/docs-export.js");
const EXPORT_CSS: &str = include_str!("../../assets/docs-export.css");

pub const EXPORT_LANGUAGES: [&str; 8] = ["en", "es", "it", "ja", "ko", "pt-BR", "zh-CN", "zh-TW"];

/// Render a snapshot as one self-contained HTML file.
///
/// `snapshot` must already have annotations applied — `apply_annotations` is
/// Rust and the export has no Rust at runtime. `annotations` travels too,
/// because the merge erases what the viewer needs to colour groups:
/// `snapshot.groups` holds resolved `TableGroup`s, `annotations.groups` holds
/// the hue.
pub fn to_standalone_html(
    snapshot: &SchemaSnapshot,
    annotations: &AnnotationFile,
    lang: &str,
) -> Result<String, String> {
    if !EXPORT_LANGUAGES.contains(&lang) {
        return Err(format!("Unknown language \"{lang}\". Valid values: {}.", EXPORT_LANGUAGES.join(", ")));
    }

    let payload = serde_json::json!({ "snapshot": snapshot, "annotations": annotations, "lang": lang });
    let json = serde_json::to_vec(&payload)
        .map_err(|error| format!("Failed to serialise the documentation payload: {error}"))?;
    // base64 rather than escaped JSON: the alphabet cannot contain `<`, so no
    // escaping rule exists to forget. The alternative's correctness depends
    // on every serialisation path applying the escape.
    let encoded = base64::engine::general_purpose::STANDARD.encode(&json);

    let title = html_escape(&snapshot.project.name);
    Ok(format!(
        "<!doctype html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{title}</title>\n<style>{EXPORT_CSS}</style>\n</head>\n<body>\n<div id=\"app\"></div>\n<script type=\"application/dbx-snapshot\">{encoded}</script>\n<script>{EXPORT_JS}</script>\n</body>\n</html>\n"
    ))
}

fn html_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::annotations::AnnotationFile;
    use crate::docs::snapshot::SchemaSnapshot;

    /// `SchemaSnapshot` is `#[serde(rename_all = "camelCase")]` with
    /// `format_version` at the TOP level — not inside `project` — and
    /// `ProjectMeta` requires `name`, `databaseType`, `schemas` and
    /// `generatedAt`. `AnnotationFile` does NOT derive `Default`, and its
    /// `format_version` must be 1, so it is built explicitly.
    fn fixture() -> (SchemaSnapshot, AnnotationFile) {
        let snapshot: SchemaSnapshot = serde_json::from_str(
            r#"{"formatVersion":1,"project":{"name":"shop","databaseType":"postgres","database":"shop","schemas":["public"],"generatedAt":"2026-08-06T00:00:00Z","note":null},"tables":[],"enums":[],"relationships":[],"groups":[],"warnings":[]}"#,
        )
        .expect("fixture snapshot");
        let annotations = AnnotationFile {
            format_version: 1,
            project: None,
            groups: Vec::new(),
            tables: std::collections::BTreeMap::new(),
        };
        (snapshot, annotations)
    }

    #[test]
    fn a_note_containing_a_closing_script_tag_survives() {
        // THE reason the payload is base64. A note discussing HTML is
        // entirely plausible in a schema document, and inlined as text it
        // would terminate the script element early and inject the rest of
        // the payload as markup.
        let (snapshot, mut annotations) = fixture();
        annotations.project = Some(crate::docs::annotations::ProjectAnnotation {
            name: None,
            note: Some("</script><img src=x onerror=alert(1)>".into()),
        });
        let html = to_standalone_html(&snapshot, &annotations, "en").expect("export");

        assert!(!html.contains("<img src=x"), "the payload leaked into markup");
        assert_eq!(html.matches("</script>").count(), 2, "exactly the two real script elements");
    }

    #[test]
    fn the_payload_round_trips() {
        let (snapshot, annotations) = fixture();
        let html = to_standalone_html(&snapshot, &annotations, "en").expect("export");
        let start = html.find("application/dbx-snapshot").expect("payload element");
        let body = &html[start..];
        let encoded = body[body.find('>').unwrap() + 1..body.find("</script>").unwrap()].trim();
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).expect("valid base64");
        let value: serde_json::Value = serde_json::from_slice(&decoded).expect("valid json");
        assert_eq!(value["snapshot"]["project"]["name"], "shop");
        assert_eq!(value["lang"], "en");
    }

    #[test]
    fn the_shell_and_stylesheet_reference_no_external_resources() {
        // This test's reach stops at the hand-authored shell and the
        // bundled CSS's `url(...)` references — it does not scan EXPORT_JS
        // for network calls. A substring scan of the bundle can't prove
        // that: it legitimately contains the literal `http://` four times
        // (three SVG/MathML/xlink XML namespace URIs passed to
        // `createElementNS`, one inside the markdown autolinker building an
        // href for `www.`-prefixed text), none of which are fetched. The
        // bundle's purity is covered on the viewer side instead:
        // `componentContract.spec.ts` forbids `fetch(`, `axios` and
        // `invoke(` in every viewer source, and the manifest guard ties the
        // committed bundle to those sources.
        let (snapshot, annotations) = fixture();
        let html = to_standalone_html(&snapshot, &annotations, "en").expect("export");

        // 1. Every `url(...)` in the emitted stylesheet must be a `data:`
        //    URI — an allowlist of the one legitimate scheme, not a
        //    blocklist of bad ones, so a new absolute font or image url()
        //    fails without anyone having to extend a list.
        let style_start = html.find("<style>").expect("style element") + "<style>".len();
        let style_end = html.find("</style>").expect("style element closes");
        let mut rest = &html[style_start..style_end];
        while let Some(pos) = rest.find("url(") {
            rest = &rest[pos + "url(".len()..];
            let end = rest.find(')').expect("unterminated url(");
            let value = rest[..end].trim().trim_matches('\'').trim_matches('"');
            assert!(value.starts_with("data:"), "stylesheet references a non-data url(): {value}");
            rest = &rest[end + 1..];
        }

        // 2. The hand-authored shell — the document with the bundle's own
        //    CSS, JS, and the base64 payload removed — must contain no
        //    `src=` or `href=` at all. Checked against the shell alone so
        //    neither the bundle's contents nor the payload can influence
        //    the result.
        let payload_start = html.find("application/dbx-snapshot").expect("payload element");
        let payload_body = &html[payload_start..];
        let encoded = payload_body[payload_body.find('>').unwrap() + 1..payload_body.find("</script>").unwrap()].trim();
        let shell = html.replace(EXPORT_CSS, "").replace(EXPORT_JS, "").replace(encoded, "");
        assert!(!shell.contains("src="), "the shell references an external src");
        assert!(!shell.contains("href="), "the shell references an external href");
    }

    #[test]
    fn an_unknown_language_is_rejected() {
        let (snapshot, annotations) = fixture();
        let error = to_standalone_html(&snapshot, &annotations, "kl").expect_err("should reject");
        assert!(error.contains("kl"), "got: {error}");
        assert!(error.contains("en"), "the error must list the valid locales, got: {error}");
    }
}
