//! Markdown to safe HTML (FR-UI-002, NFR-SEC-011): comrak with GitHub's
//! extensions, then `ammonia`, so nothing a pusher writes can run script on
//! this origin. Raw HTML in the source is dropped by comrak and anything that
//! slips through is removed by ammonia.

use comrak::nodes::NodeValue;
use comrak::{Arena, Options, format_html, parse_document};

/// Where relative links in a document point. A README at `docs/README.md` on
/// `main` links `guide.md` to `{blob}/docs/guide.md` and shows `logo.png`
/// from `{raw}/docs/logo.png`.
#[derive(Debug, Clone)]
pub struct LinkBase {
    /// The page that shows a file, without a trailing slash, e.g. `/alice/demo/blob/main`.
    pub blob: String,
    /// The download of a file, without a trailing slash, e.g. `/alice/demo/raw/main`.
    pub raw: String,
    /// The directory the document is in, `""` for the root.
    pub dir: String,
}

/// Renders `markdown` to sanitised HTML.
pub fn render(markdown: &str, base: Option<&LinkBase>) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;

    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &options);
    if let Some(base) = base {
        for node in root.descendants() {
            let mut data = node.data_mut();
            match &mut data.value {
                NodeValue::Link(link) => link.url = rebase(&link.url, &base.blob, &base.dir),
                NodeValue::Image(image) => image.url = rebase(&image.url, &base.raw, &base.dir),
                _ => {}
            }
        }
    }
    let mut html = String::new();
    if format_html(root, &options, &mut html).is_err() {
        return String::new();
    }
    ammonia::Builder::default()
        .link_rel(Some("noopener noreferrer nofollow"))
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tags(["input"])
        // Footnote anchors and column alignment, as comrak writes them.
        .add_tag_attributes("a", ["id"])
        .add_tag_attributes("li", ["id"])
        .add_tag_attributes("th", ["align"])
        .add_tag_attributes("td", ["align"])
        .attribute_filter(|_, attribute, value| match attribute {
            // Only footnote ids, so a document can't take over the page's own ids.
            "id" => ["fn-", "fnref-"].iter().any(|p| value.starts_with(p)).then_some(value.into()),
            "align" => matches!(value, "left" | "center" | "right").then_some(value.into()),
            _ => Some(value.into()),
        })
        .clean(&html)
        .to_string()
}

/// A link relative to the document becomes a path under `prefix`. Absolute
/// URLs, site paths (`/…`), fragments (`#…`) and queries are left alone.
fn rebase(url: &str, prefix: &str, dir: &str) -> String {
    let is_relative = !url.is_empty()
        && !url.starts_with(['/', '#', '?'])
        && !url.split_once(':').is_some_and(|(scheme, _)| {
            scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
    if !is_relative {
        return url.to_owned();
    }
    let (path, suffix) = match url.find(['#', '?']) {
        Some(at) => url.split_at(at),
        None => (url, ""),
    };
    let mut segments: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    format!("{prefix}/{}{suffix}", segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> LinkBase {
        LinkBase { blob: "/a/r/blob/main".into(), raw: "/a/r/raw/main".into(), dir: "docs".into() }
    }

    #[test]
    fn script_and_raw_html_never_survive() {
        let html = render(
            "<script>alert(1)</script>\n\n[x](javascript:alert(1)) <img src=x onerror=alert(1)>",
            None,
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(!html.contains("onerror"), "{html}");
    }

    #[test]
    fn github_extensions_render() {
        let html = render("| a |\n|---|\n| b |\n\n- [x] done\n\n~~old~~ https://example.com", None);
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("type=\"checkbox\""), "{html}");
        assert!(html.contains("<del>old</del>"), "{html}");
        assert!(html.contains("href=\"https://example.com\""), "{html}");
        assert!(html.contains("rel=\"noopener noreferrer nofollow\""), "{html}");
    }

    #[test]
    fn footnotes_and_column_alignment_survive() {
        let html = render("x[^1]\n\n| a | b |\n|:-:|--:|\n| c | d |\n\n[^1]: note\n", None);
        assert!(html.contains("href=\"#fn-1\""), "{html}");
        assert!(html.contains("id=\"fn-1\""), "{html}");
        assert!(html.contains("href=\"#fnref-1\""), "{html}");
        assert!(html.contains("id=\"fnref-1\""), "{html}");
        assert!(html.contains("<th align=\"center\">"), "{html}");
        assert!(html.contains("<td align=\"right\">"), "{html}");
    }

    #[test]
    fn relative_links_point_into_the_repository() {
        let html = render(
            "[guide](guide.md#setup) [up](../LICENSE) ![logo](./img/logo.png) [abs](/x) [s](#top)",
            Some(&base()),
        );
        assert!(html.contains("href=\"/a/r/blob/main/docs/guide.md#setup\""), "{html}");
        assert!(html.contains("href=\"/a/r/blob/main/LICENSE\""), "{html}");
        assert!(html.contains("src=\"/a/r/raw/main/docs/img/logo.png\""), "{html}");
        assert!(html.contains("href=\"/x\""), "{html}");
        assert!(html.contains("href=\"#top\""), "{html}");
    }
}
