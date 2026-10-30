//! Exact normalized source text; display caps belong to individual views.
pub(crate) fn text(source: &str, span: proc_macro2::Span) -> String {
    source
        .get(span.byte_range())
        .unwrap_or("<source unavailable>")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
