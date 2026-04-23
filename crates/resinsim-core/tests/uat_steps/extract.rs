//! Extract Gherkin from ```gherkin fenced code blocks in markdown source.
//!
//! `spec/uat/*.md` is the single source of truth for UAT scenarios. Each
//! file mixes frontmatter + rationale prose + one or more ```gherkin fenced
//! code blocks, each placed under a `## Scenario: <title>` heading. This
//! module walks the pulldown-cmark event stream and returns every fence's
//! body paired with the closest preceding heading.
//!
//! The function is **total**: any byte sequence (arbitrary UTF-8, CRLF,
//! BOM, nested fence attempts, malformed YAML frontmatter) yields a `Vec`
//! — possibly empty — and never panics. See `extract_tests.rs` for the
//! property-based coverage that pins this invariant.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// One scenario block extracted from a markdown source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedScenario {
    /// Title derived from the most recent markdown heading before the fence.
    /// Leading `Scenario:` / `Scenario Outline:` keywords are stripped so
    /// the stored title matches the Gherkin scenario name one-for-one.
    /// Empty when no heading precedes the fence.
    pub title: String,
    /// Verbatim fence body (no trimming). Callers feed this to
    /// cucumber-rs after wrapping with a synthesized `Feature:` header.
    pub gherkin: String,
}

/// Extract all ```gherkin fenced code blocks from `source`.
///
/// Total function — never panics on arbitrary input. Returns an empty
/// `Vec` when `source` contains no `gherkin`-tagged fences.
pub fn extract(source: &str) -> Vec<ExtractedScenario> {
    let source = strip_bom(source);
    let body = strip_frontmatter(source);

    let parser = Parser::new_ext(body, Options::empty());

    let mut out = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut heading_buffer = String::new();
    let mut in_heading = false;
    let mut fence_buffer = String::new();
    let mut in_gherkin_fence = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                heading_buffer.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                current_heading = Some(normalize_heading(&heading_buffer));
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang)))
                if lang.as_ref() == "gherkin" =>
            {
                in_gherkin_fence = true;
                fence_buffer.clear();
            }
            Event::End(TagEnd::CodeBlock) if in_gherkin_fence => {
                in_gherkin_fence = false;
                out.push(ExtractedScenario {
                    title: current_heading.clone().unwrap_or_default(),
                    gherkin: std::mem::take(&mut fence_buffer),
                });
            }
            Event::Text(ref text) => {
                if in_heading {
                    heading_buffer.push_str(text);
                } else if in_gherkin_fence {
                    fence_buffer.push_str(text);
                }
            }
            Event::Code(ref code) if in_heading => {
                heading_buffer.push_str(code);
            }
            _ => {}
        }
    }

    out
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Strip a YAML frontmatter block delimited by `---` lines. No YAML
/// parsing: the extractor only needs the prose/headings after the block.
/// Returns the original slice when no closing delimiter is found, so a
/// malformed frontmatter can't silently swallow the whole file.
fn strip_frontmatter(s: &str) -> &str {
    let Some(rest) = s.strip_prefix("---") else {
        return s;
    };
    // Skip the rest of the opener line (including any trailing CR).
    let Some(nl_idx) = rest.find('\n') else {
        return s;
    };
    let after_opener = &rest[nl_idx + 1..];

    // Scan line-by-line for a bare `---` closer.
    let mut offset = 0usize;
    for line in after_opener.split('\n') {
        if line.trim_end_matches('\r') == "---" {
            let closer_end = offset + line.len() + 1;
            return after_opener.get(closer_end..).unwrap_or("");
        }
        offset += line.len() + 1;
    }
    s
}

fn normalize_heading(raw: &str) -> String {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("Scenario Outline:")
        .or_else(|| trimmed.strip_prefix("Scenario:"))
        .unwrap_or(trimmed);
    stripped.trim().to_string()
}
