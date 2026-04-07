use ratatui::text::Text;

/// Renders markdown into styled `ratatui::Text`.
pub(crate) fn render_markdown(input: &str) -> Text<'static> {
    crate::markdown_render::render_markdown_text(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_lists_are_rendered() {
        let text = render_markdown("# Header\n- item");
        let rendered = text
            .lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("# Header"));
        assert!(rendered.contains("- item"));
    }
}
