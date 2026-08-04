//! Authoring-time guard: every backtick-quoted repo path and every
//! file-attributed code symbol in `agent-constraints/*.md` must still
//! resolve against the real tree, the See-also graph among those four
//! files must stay symmetric and orphan-free, and none of the four may
//! grow a `file.rs:NNN` / "line NN" anchor.
//!
//! # Why this is a separate test target
//!
//! Same reasoning as `spec_gherkin_wellformed.rs`: this target's name does
//! NOT match `^uat_`, so `.config/nextest.toml`'s `default-filter = "not
//! binary(/^uat_/)"` does not exclude it, and it runs in the mandated
//! ADR-0017 four-config matrix like any other test.
//! `nextest_filter_sanity.rs` pins that exclusion pattern, so the naming
//! constraint is enforced rather than merely remembered.
//!
//! # Baseline this guard is expected to reproduce (measured 2026-08-04,
//! re-derived against the live tree at `main = 0ea33d26`, per plan step 1)
//!
//! `pulldown_cmark::Parser::into_offset_iter()` filtered to `Event::Code`
//! over the four files yields **424 inline code spans (282 distinct)**.
//! Classifying every span against the rules below (R1-R4, R6) and
//! re-running against the real tree produces, with **zero failures**:
//!
//! - **91 path checks** (88 bare repo-path spans + 3 more from the file
//!   part of the three `file.rs::Symbol` composite spans)
//! - **28 attributed-symbol checks** (25 plain attributed spans + 3 more
//!   from the symbol part of the same three composite spans)
//! - **4 negative assertions** (`spikes/`, `knowledge-base/`, `decisions/`
//!   in `knowledge-base.md`; `tests/uat/` in `uat-conventions.md`) — all
//!   four correctly stay unresolved
//! - **304 ignored spans**, all under one of the six documented reasons
//!   (a closed `IgnoreReason` enum — see below): 103 commands/flags/prose
//!   (contain whitespace), 111 shapes that match neither the path nor the
//!   identifier grammar, 49 English words/acronyms, 29 code-shaped but
//!   unattributed symbols, 4 lowerCamelCase lifecycle-model fields, 8
//!   external-authority tokens (`references/…`, `feedback_*.md`,
//!   `autonomous-loop.md`)
//!
//! 91 + 28 + 4 + 304 = 427 checks over 424 spans; the difference is
//! exactly the three composite spans, each contributing one path check
//! and one symbol check. These counts differ by a few from the planning
//! prototype's illustrative 88/27/305 (which likely counted the three
//! composite spans differently); re-deriving against the live tree per
//! plan step 1 reproduced the load-bearing figures exactly — **4 negative
//! assertions and 0 failures** — so this is not a material difference.
//!
//! # The ten tests
//!
//! Five RED-first unit tests (step 3), each proving one failure direction
//! fires against a hand-written fixture + [`FakeTree`], so nothing here
//! can be made green by accident:
//! - [`the_guard_flags_a_dangling_repo_path`]
//! - [`the_guard_flags_a_symbol_missing_from_its_attributed_file`]
//! - [`the_guard_flags_a_one_way_see_also_link`]
//! - [`the_guard_flags_an_external_authority_that_became_resolvable`]
//! - [`the_guard_ignores_commands_flags_and_skill_references`] (positive
//!   control: proves the guard cannot be made green by over-ignoring)
//!
//! Five GREEN integration tests (step 4), each calling the same [`audit`]
//! with [`RealTree`] over the `read_dir`-discovered doc set:
//! - [`every_backticked_repo_path_resolves`]
//! - [`every_attributed_symbol_resolves`]
//! - [`paths_the_docs_declare_absent_are_absent`]
//! - [`the_see_also_graph_is_symmetric_and_orphan_free`]
//! - [`the_docs_carry_no_line_number_anchors`]
//!
//! # Verified fault-injection matrix (step 5, adversarial self-check)
//!
//! Run once, in-memory, against STRING COPIES of the real doc text plus a
//! [`FakeTree`] where a fault needed a mutated file body (never against
//! the live tree, never committed — no file under `agent-constraints/`
//! was written to):
//!
//! | Injected defect | Where | Detected by |
//! |---|---|---|
//! | Renamed a const the doc still cites | `implementation-conventions.md`'s in-memory copy audited against a `FakeTree` whose `uat_gherkin.rs` entry is the real file with every `SPECS_WITHOUT_STEP_DEFS` replaced by `SPECS_WITHOUT_STEP_DEFX` | symbol-missing failure, same shape as [`the_guard_flags_a_symbol_missing_from_its_attributed_file`] |
//! | Deleted a See-also entry | `uat-conventions.md`'s in-memory copy with the `agent-constraints/knowledge-base.md` line stripped out, checked alongside the three untouched real docs | asymmetric-link violation, same shape as [`the_guard_flags_a_one_way_see_also_link`] |
//! | Typo'd a `docs/patterns/…` path | `knowledge-base.md`'s in-memory copy with `docs/patterns/anti/adr-pattern-doc-drift-from-iterated-values.md` mangled to end `-valuesx.md`, audited against the real `RealTree` | dangling-path failure, same shape as [`the_guard_flags_a_dangling_repo_path`] |
//! | Added a `file.rs:42` anchor | appended `"See world.rs:42 for details."` to `knowledge-base.md`'s in-memory copy | [`line_anchor_violations`] returns a `:42` hit |
//! | Made a negated path exist | `knowledge-base.md`'s in-memory copy audited against a `FakeTree` where `spikes/` (one of R4's four negated paths) resolves | negated-path-now-resolves failure, same polarity-flip shape as [`the_guard_flags_an_external_authority_that_became_resolvable`] |
//!
//! Each of the five was reported by the guard; re-running `audit_all`
//! against the four PRISTINE, unmutated docs immediately afterward
//! reported zero failures, confirming the guard is not simply failing
//! everything. `git status` stayed clean of `agent-constraints/` changes
//! throughout — every mutation lived in a local `String`, never written
//! to disk.
//!
//! # What this guard is blind to (R7 anti-blindness floors)
//!
//! A classifier bug that routed every span to `Ignored` would leave this
//! guard green and useless — see
//! `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`.
//! So the integration tests also assert floors well below today's
//! measured headroom (91 paths, 28 symbols, 4 docs): **>= 60 path
//! checks, >= 15 symbol checks, >= 4 discovered docs**. `IgnoreReason` is
//! a closed Rust enum, so a span the classifier cannot place is a compile
//! error in this file, not a silent pass.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, Options, Parser};

// ---------------------------------------------------------------------
// R1 — multi-root path resolution
// ---------------------------------------------------------------------

/// Ordered root list. Resolve a class-1 token against each in turn and
/// stop at the first hit. Order mirrors the plan's "Choosing the tree"
/// shorthand table exactly.
const ROOTS: &[&str] = &[
    "",
    "crates/resinsim-core/",
    "crates/resinsim-core/tests/",
    "crates/resinsim-core/tests/uat_steps/",
    "agent-constraints/",
    "docs/",
];

/// Port through which `audit` touches the outside world. The only seam —
/// `RealTree` walks `std::fs`, `FakeTree` is a `BTreeMap`, and every
/// failure direction below is reachable from the latter without a
/// filesystem.
trait TreeResolver {
    /// Try `rel` against each of the 6 [`ROOTS`] in order; return the
    /// first root-joined path that exists (as a file OR a directory).
    fn resolve(&self, rel: &str) -> Option<String>;
    /// Contents of an already-resolved path. If `resolved` names a
    /// directory, this is the concatenation of every file under it
    /// (recursively) — R3's "or, if a directory, anywhere under it".
    fn contents(&self, resolved: &str) -> String;
}

struct RealTree {
    repo_root: PathBuf,
}

impl RealTree {
    fn discover() -> Self {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("CARGO_MANIFEST_DIR has crate + workspace + repo ancestors")
            .to_path_buf();
        RealTree { repo_root }
    }

    /// `read_dir` on `agent-constraints/`, sorted, never a hardcoded list
    /// of four — a fifth constraint doc must be picked up automatically.
    fn discover_agent_constraints_docs(&self) -> Vec<(String, String)> {
        let dir = self.repo_root.join("agent-constraints");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
            .flatten()
            .map(|entry| entry.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .collect();
        names.sort();
        assert!(
            !names.is_empty(),
            "no .md files under {} — the resolver is pointing at the wrong directory",
            dir.display()
        );
        names
            .into_iter()
            .map(|name| {
                let path = dir.join(&name);
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                (name, text)
            })
            .collect()
    }
}

fn collect_dir_text(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_dir_text(&path, out);
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            out.push_str(&content);
            out.push('\n');
        }
    }
}

impl TreeResolver for RealTree {
    fn resolve(&self, rel: &str) -> Option<String> {
        for root in ROOTS {
            let candidate = format!("{root}{rel}");
            if self.repo_root.join(&candidate).exists() {
                return Some(candidate);
            }
        }
        None
    }

    fn contents(&self, resolved: &str) -> String {
        let full = self.repo_root.join(resolved);
        if full.is_dir() {
            let mut out = String::new();
            collect_dir_text(&full, &mut out);
            out
        } else {
            std::fs::read_to_string(&full).unwrap_or_default()
        }
    }
}

/// Test-only tree. Keys are already-resolved relative paths (as if
/// `resolve` had joined a root); `resolve` still walks [`ROOTS`] against
/// the map so fixtures exercise the exact same multi-root logic as
/// [`RealTree`].
struct FakeTree(BTreeMap<String, String>);

impl TreeResolver for FakeTree {
    fn resolve(&self, rel: &str) -> Option<String> {
        for root in ROOTS {
            let candidate = format!("{root}{rel}");
            if self.0.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn contents(&self, resolved: &str) -> String {
        self.0.get(resolved).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------
// Code span extraction
// ---------------------------------------------------------------------

/// One inline code span: byte range of the WHOLE span (including
/// backticks) in the source, plus the normalised inner text
/// pulldown-cmark hands back via `Event::Code` (CommonMark's
/// line-ending-to-space normalisation already applied).
struct CodeSpan {
    start: usize,
    end: usize,
    text: String,
}

/// Extract every `Event::Code` span from `text`. Fenced code blocks
/// arrive as `Event::Text` inside `Tag::CodeBlock` and are therefore
/// excluded BY CONSTRUCTION — no hand-written exemption needed to keep
/// `jj rebase -s <other-head>` or a `cargo build --features …` recipe
/// line out of the token stream.
fn extract_code_spans(text: &str) -> Vec<CodeSpan> {
    Parser::new_ext(text, Options::empty())
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Code(code) => Some(CodeSpan {
                start: range.start,
                end: range.end,
                text: code.into_string(),
            }),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------
// Pure shape predicates (R1, R2's identifier law, R4's negation cues, R6)
// ---------------------------------------------------------------------

/// `floor_char_boundary`-equivalent: not yet stable, so hand-rolled.
fn floor_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Class-1 grammar: `^\.?[A-Za-z0-9_][A-Za-z0-9_./-]*$`, plus the "NOT a
/// bare `.ext`" and "ends `/` or `.rs`/`.md`/`.toml`" refinements from the
/// plan's table. A bare filename with no `/` (e.g. `rustfmt.toml`) is
/// still allowed as long as it isn't a naked extension like `.md`.
fn is_repo_path_shaped(tok: &str) -> bool {
    if tok.is_empty() {
        return false;
    }
    let rest = tok.strip_prefix('.').unwrap_or(tok);
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() || c == '_' => {}
        _ => return false,
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-')) {
        return false;
    }

    if !tok.contains('/') {
        if matches!(tok, ".md" | ".rs" | ".toml" | ".feature") {
            return false;
        }
        return tok.ends_with(".rs") || tok.ends_with(".md") || tok.ends_with(".toml");
    }
    tok.ends_with('/') || tok.ends_with(".rs") || tok.ends_with(".md") || tok.ends_with(".toml")
}

/// Class-10, the two-authorities rule made executable: `references/`
/// (issue-lifecycle skill's reference tree), `feedback_*` (ora-root
/// project memory), or the single basename `autonomous-loop.md`.
fn is_external_authority(tok: &str) -> bool {
    tok.starts_with("references/") || tok.starts_with("feedback_") || tok == "autonomous-loop.md"
}

/// One `::`-or-plain identifier segment: Rust identifier grammar,
/// `^[A-Za-z_][A-Za-z0-9_]*$`.
fn is_ident_segment(seg: &str) -> bool {
    let mut chars = seg.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Gate before class 3/7/8/9: a token built from anything other than
/// Rust identifier characters (plus `::` separators and an optional
/// trailing `()` call) matches neither the path nor the symbol grammar,
/// so it is class 6 — "matches neither grammar" — not an English word
/// and not a candidate for attribution. Excludes things like `KB-R###`,
/// `<spec-stem>.feature`, `hydrate.signature`, `$CARGO_TARGET_TMPDIR/…`,
/// `@magistr/issue-lifecycle`, `feat/12`, `spec/uat/*.md`.
fn is_identifier_like(tok: &str) -> bool {
    let t = tok.strip_suffix("()").unwrap_or(tok);
    if t.is_empty() {
        return false;
    }
    t.split("::").all(is_ident_segment)
}

/// A lowercase-to-uppercase transition strictly after the first
/// character — a true CamelCase hump, not merely "contains an uppercase
/// letter" (which would wrongly flag ALL-CAPS acronyms like `PATH` or
/// `LGTM`).
fn has_camel_hump(tok: &str) -> bool {
    let chars: Vec<char> = tok.chars().collect();
    (1..chars.len()).any(|i| chars[i].is_uppercase() && chars[i - 1].is_lowercase())
}

/// Class 9: lowerCamelCase lifecycle/model field names — starts
/// lowercase, no `_`, has an interior hump. Checked BEFORE the
/// code-shaped/attribution branch so a field like `affectedAreas` is
/// ignored even when it sits next to a resolvable path (the class-9
/// motivating false-failure from the plan's evidence base).
fn is_lifecycle_field(tok: &str) -> bool {
    let mut chars = tok.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    if !chars.as_str().chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    has_camel_hump(tok)
}

/// Class 7's law, inverted: code-shaped means Rust's own naming
/// convention shows through — a `_` (snake_case / SCREAMING_SNAKE), a
/// `::` (qualified path), or an interior CamelCase hump (UpperCamel
/// types). Anything identifier-shaped that has none of these three is
/// English prose in backticks.
fn is_code_shaped(tok: &str) -> bool {
    tok.contains('_') || tok.contains("::") || has_camel_hump(tok)
}

/// Class-2 composite grammar: `^(<path>\.rs)::([A-Za-z_]\w*)$` — exactly
/// one `::`, left side is a `.rs` path, right side is a bare identifier.
fn composite_match(tok: &str) -> Option<(&str, &str)> {
    if tok.matches("::").count() != 1 {
        return None;
    }
    let mut parts = tok.split("::");
    let file_part = parts.next()?;
    let sym_part = parts.next()?;
    if !file_part.ends_with(".rs") || !is_repo_path_shaped(file_part) {
        return None;
    }
    if !is_ident_segment(sym_part) {
        return None;
    }
    Some((file_part, sym_part))
}

/// R3: normalise `A::B::c()` to `c` — strip a trailing `()`, take the
/// segment after the last `::`. Used only for the substring lookup, not
/// for classification.
fn lookup_needle(tok: &str) -> &str {
    let t = tok.strip_suffix("()").unwrap_or(tok);
    t.rsplit("::").next().unwrap_or(t)
}

/// Block boundary set from R2/R4: `. ` / `.\n` / em-dash / `;` / a blank
/// line / a line starting `#` / a table-row `|` / a list bullet.
fn crosses_block_boundary(gap: &str) -> bool {
    if gap.contains(". ") || gap.contains(".\n") || gap.contains('—') || gap.contains(';') {
        return true;
    }
    if gap.contains("\n\n") {
        return true;
    }
    gap.split('\n').skip(1).any(|line| {
        let t = line.trim_start();
        t.starts_with('#') || t.starts_with('|') || t.starts_with("- ") || t.starts_with("* ")
    })
}

/// Collapse whitespace (including embedded newlines) to single spaces and
/// trim, so a soft-wrapped gap like `\n  in the\n  ` normalises to the
/// same `"in the"` a same-line gap would.
fn normalize_gap(gap: &str) -> String {
    gap.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// R2 forward connective whitelist — literal, not a fuzzy character-gap
/// heuristic (an earlier "gap <= 4 chars" draft mis-attributed
/// `RecipeBuilder` to `fixtures.rs` across `") or "`).
const FORWARD_CONNECTIVES: &[&str] = &["", "(", "in", "in the", "'s", ","];

/// R4's tight literal negation-cue set.
const NEGATION_CUES: &[&str] = &[
    "There is no",
    "there are no",
    "does not exist",
    "no longer exist",
    "**not**",
];

/// Find the start of the sentence/paragraph containing byte offset
/// `before`, using the same [`crosses_block_boundary`] delimiter set, so
/// R4's negation cue can be scoped to "within its sentence/paragraph"
/// rather than bleeding into the next sentence's legitimate paths.
fn sentence_start(text: &str, before: usize) -> usize {
    let window_start = floor_char_boundary(text, before.saturating_sub(400));
    let window = &text[window_start..before];
    let mut last = 0usize;
    for pat in [". ", ".\n", ";", "\n\n"] {
        for (i, _) in window.match_indices(pat) {
            last = last.max(i + pat.len());
        }
    }
    for (i, m) in window.match_indices('—') {
        last = last.max(i + m.len());
    }
    window_start + floor_char_boundary(window, last)
}

/// R4: does a negation cue appear earlier in the SAME sentence as the
/// span starting at `span_start`?
fn negation_precedes(text: &str, span_start: usize) -> bool {
    let seg_start = sentence_start(text, span_start);
    let segment = &text[seg_start..span_start];
    NEGATION_CUES.iter().any(|cue| segment.contains(cue))
}

/// R2: attribute a code-shaped span at `spans[idx]` to a resolvable path,
/// forward first, then backward (comma-runs), each direction stopping at
/// a block boundary. Returns the resolved path, or `None` if the symbol
/// is unattributed (class 8).
fn attribute_symbol(
    spans: &[CodeSpan],
    idx: usize,
    text: &str,
    tree: &dyn TreeResolver,
) -> Option<String> {
    // Forward: `SYMBOL` (`file.rs`) / `SYMBOL` in `file.rs` — the literal
    // connective whitelist, never a character-count heuristic.
    if idx + 1 < spans.len() {
        let gap = &text[spans[idx].end..spans[idx + 1].start];
        if !crosses_block_boundary(gap) && FORWARD_CONNECTIVES.contains(&normalize_gap(gap).as_str())
        {
            let candidate = spans[idx + 1].text.as_str();
            if is_repo_path_shaped(candidate)
                && !is_external_authority(candidate)
                && let Some(resolved) = tree.resolve(candidate)
            {
                return Some(resolved);
            }
        }
    }
    // Backward: `file.rs`'s `Symbol`, and comma-runs — walk back over
    // previous spans to the nearest resolvable path, stopping at a block
    // boundary.
    let mut cursor = idx;
    while cursor > 0 {
        let gap = &text[spans[cursor - 1].end..spans[cursor].start];
        if crosses_block_boundary(gap) {
            break;
        }
        let candidate = spans[cursor - 1].text.as_str();
        if is_repo_path_shaped(candidate)
            && !is_external_authority(candidate)
            && let Some(resolved) = tree.resolve(candidate)
        {
            return Some(resolved);
        }
        cursor -= 1;
    }
    None
}

// ---------------------------------------------------------------------
// R6 — line-anchor policy
// ---------------------------------------------------------------------

/// `:NNN` anchors and "line NN" prose — forbidden in all four files, no
/// grandfathering. Scans the RAW text (not just code spans), because a
/// `file.rs:123` reference could appear either inside or outside
/// backticks and both are equally a rotting reference.
fn line_anchor_violations(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (i, c) in text.char_indices() {
        if c != ':' {
            continue;
        }
        let digits: String = text[i + 1..].chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            hits.push(format!("`:{digits}` anchor at byte offset {i}"));
        }
    }
    for (i, _) in text.match_indices("line ") {
        let after = &text[i + "line ".len()..];
        if after.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            hits.push(format!("\"line {digits}\" prose at byte offset {i}"));
        }
    }
    hits
}

// ---------------------------------------------------------------------
// R5 — See-also graph
// ---------------------------------------------------------------------

fn basename(tok: &str) -> String {
    tok.rsplit('/').next().unwrap_or(tok).to_string()
}

/// Basenames of every code span appearing at or after the `## See also`
/// heading. Scoped by BYTE OFFSET, not by re-parsing a substring, so a
/// span that happens to look like a heading inside the See-also list body
/// can't confuse a second markdown parse.
fn see_also_basenames(text: &str) -> Vec<String> {
    let Some(heading_pos) = text.find("## See also") else {
        return Vec::new();
    };
    extract_code_spans(text)
        .into_iter()
        .filter(|s| s.start >= heading_pos)
        .map(|s| basename(&s.text))
        .collect()
}

/// R5, scoped to the discovered doc set only (never tree-wide — see the
/// module doc comment and
/// `docs/patterns/anti/doc-audit-scoped-to-one-authority.md`, which cites
/// `iteration-limits.md` one-way by design and would fail immediately if
/// this were widened). Returns a human-readable violation per structural
/// / symmetry / orphan problem found; empty means clean.
fn see_also_symmetry_violations(docs: &[(String, String)]) -> Vec<String> {
    let names: BTreeSet<&str> = docs.iter().map(|(n, _)| n.as_str()).collect();
    let mut edges: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut violations = Vec::new();

    for (name, text) in docs {
        if !text.contains("## See also") {
            violations.push(format!("{name} has no `## See also` section"));
        }
        let targets: BTreeSet<String> = see_also_basenames(text)
            .into_iter()
            .filter(|b| names.contains(b.as_str()))
            .collect();
        edges.insert(name.as_str(), targets);
    }

    for &a in &names {
        for &b in &names {
            if a == b {
                continue;
            }
            let a_to_b = edges.get(a).is_some_and(|s| s.contains(b));
            let b_to_a = edges.get(b).is_some_and(|s| s.contains(a));
            if a_to_b != b_to_a {
                violations.push(format!(
                    "asymmetric See-also link: {a} -> {b} = {a_to_b}, but {b} -> {a} = {b_to_a}"
                ));
            }
        }
    }

    for &name in &names {
        let cited = edges.values().any(|s| s.contains(name));
        if !cited {
            violations.push(format!(
                "{name} is never cited by a sibling's `## See also` section (orphan)"
            ));
        }
    }

    violations
}

// ---------------------------------------------------------------------
// Audit: the domain service tying it together
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum IgnoreReason {
    /// Class 5: span contains whitespace (commands, flags, prose,
    /// headings, shapes like `&[(&str, usize)]`).
    CommandFlagOrProse,
    /// Class 6: matches neither the path grammar nor the identifier
    /// grammar (`KB-NNN-<slug>.md`, `hydrate.signature`, `feat/12`, …).
    NonPathNonSymbolShape,
    /// Class 7: identifier-shaped but not code-shaped — no `_`, no `::`,
    /// no interior CamelCase hump (`main`, `Version`, `LGTM`, …).
    EnglishWordOrAcronym,
    /// Class 8: code-shaped but no resolvable path attaches to it in the
    /// same block (`approve_plan`, `MAX_PLAN_ITERATIONS`, …).
    UnattributedSymbol,
    /// Class 9: lowerCamelCase lifecycle/model field name
    /// (`affectedAreas`, `testReviewIteration`, …).
    LifecycleField,
    /// Class 10: the two-authorities rule — `references/…`,
    /// `feedback_*.md`, or `autonomous-loop.md`.
    ExternalAuthority,
}

impl IgnoreReason {
    fn label(self) -> &'static str {
        match self {
            IgnoreReason::CommandFlagOrProse => "command/flag/prose (contains whitespace)",
            IgnoreReason::NonPathNonSymbolShape => "matches neither the path nor symbol grammar",
            IgnoreReason::EnglishWordOrAcronym => "English word or acronym, not code-shaped",
            IgnoreReason::UnattributedSymbol => "code-shaped but unattributed",
            IgnoreReason::LifecycleField => "lowerCamelCase lifecycle/model field",
            IgnoreReason::ExternalAuthority => "external authority (two-authorities rule)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckedKind {
    Path,
    Symbol,
    NegatedPath,
}

#[derive(Debug, Clone)]
struct Checked {
    doc: String,
    token: String,
    kind: CheckedKind,
}

impl Checked {
    /// Names the file and the token — house voice, per
    /// `agent-constraints/implementation-conventions.md`'s failure-message
    /// convention.
    fn describe(&self) -> String {
        format!("{}: `{}` ({:?})", self.doc, self.token, self.kind)
    }
}

#[derive(Debug, Clone)]
struct Finding {
    doc: String,
    token: String,
    message: String,
}

impl Finding {
    /// Names the file, the token, and what was expected — the house
    /// voice `spec_gherkin_wellformed.rs` also uses for its offender
    /// lines.
    fn describe(&self) -> String {
        format!("{}: `{}` — {}", self.doc, self.token, self.message)
    }
}

#[derive(Debug, Clone)]
struct Ignored {
    doc: String,
    token: String,
    reason: IgnoreReason,
}

#[derive(Debug, Default)]
struct Audit {
    failures: Vec<Finding>,
    checked: Vec<Checked>,
    ignored: Vec<Ignored>,
}

impl Audit {
    fn path_checks(&self) -> usize {
        self.checked
            .iter()
            .filter(|c| c.kind == CheckedKind::Path)
            .count()
    }

    fn symbol_checks(&self) -> usize {
        self.checked
            .iter()
            .filter(|c| c.kind == CheckedKind::Symbol)
            .count()
    }

    fn negated_checks(&self) -> usize {
        self.checked
            .iter()
            .filter(|c| c.kind == CheckedKind::NegatedPath)
            .count()
    }

    fn merge(&mut self, other: Audit) {
        self.failures.extend(other.failures);
        self.checked.extend(other.checked);
        self.ignored.extend(other.ignored);
    }

    /// R7: the full ignore ledger, grouped by reason, for failure
    /// messages — "print the full ignore ledger grouped by reason in
    /// every failure message" so a regression that quietly routes real
    /// checks into IGNORED is visible at the point of failure, not just
    /// via the floor assertions.
    fn ignore_ledger_report(&self) -> String {
        let mut by_reason: BTreeMap<IgnoreReason, Vec<&Ignored>> = BTreeMap::new();
        for entry in &self.ignored {
            by_reason.entry(entry.reason).or_default().push(entry);
        }
        let mut out = String::new();
        for (reason, entries) in &by_reason {
            out.push_str(&format!("  {} ({}): ", reason.label(), entries.len()));
            let sample: Vec<String> = entries
                .iter()
                .take(5)
                .map(|e| format!("{}:`{}`", e.doc, e.token))
                .collect();
            out.push_str(&sample.join(", "));
            if entries.len() > 5 {
                out.push_str(&format!(", … +{} more", entries.len() - 5));
            }
            out.push('\n');
        }
        out
    }
}

/// The entry point. A pure domain service: everything it touches outside
/// `doc_name`/`text` goes through `tree`, so every failure direction is
/// reachable from a fixture string plus a [`FakeTree`] with no
/// filesystem involved.
fn audit(doc_name: &str, text: &str, tree: &dyn TreeResolver) -> Audit {
    let spans = extract_code_spans(text);
    let mut result = Audit::default();

    for (idx, span) in spans.iter().enumerate() {
        let tok = span.text.as_str();

        // Class 5: contains whitespace.
        if tok.chars().any(char::is_whitespace) {
            result.ignored.push(Ignored {
                doc: doc_name.to_string(),
                token: tok.to_string(),
                reason: IgnoreReason::CommandFlagOrProse,
            });
            continue;
        }

        // Class 2: `file.rs::Symbol` composite — two checks from one span.
        if let Some((file_part, sym_part)) = composite_match(tok) {
            result.checked.push(Checked {
                doc: doc_name.to_string(),
                token: file_part.to_string(),
                kind: CheckedKind::Path,
            });
            result.checked.push(Checked {
                doc: doc_name.to_string(),
                token: tok.to_string(),
                kind: CheckedKind::Symbol,
            });
            match tree.resolve(file_part) {
                None => result.failures.push(Finding {
                    doc: doc_name.to_string(),
                    token: tok.to_string(),
                    message: format!(
                        "composite path `{file_part}` does not resolve under any of the 6 roots"
                    ),
                }),
                Some(resolved) => {
                    if !tree.contents(&resolved).contains(sym_part) {
                        result.failures.push(Finding {
                            doc: doc_name.to_string(),
                            token: tok.to_string(),
                            message: format!(
                                "symbol `{sym_part}` not found in resolved `{resolved}`"
                            ),
                        });
                    }
                }
            }
            continue;
        }

        // Class 10: external authority — bidirectional; must NOT resolve.
        if is_external_authority(tok) {
            result.ignored.push(Ignored {
                doc: doc_name.to_string(),
                token: tok.to_string(),
                reason: IgnoreReason::ExternalAuthority,
            });
            if let Some(resolved) = tree.resolve(tok) {
                result.failures.push(Finding {
                    doc: doc_name.to_string(),
                    token: tok.to_string(),
                    message: format!(
                        "external authority now resolves in-repo at `{resolved}` — reclassify, \
                         per the exemption-list-that-only-grows trap"
                    ),
                });
            }
            continue;
        }

        // Class 1 / 4: path-shaped, split on whether a negation cue
        // precedes it in the same sentence.
        if is_repo_path_shaped(tok) {
            if negation_precedes(text, span.start) {
                result.checked.push(Checked {
                    doc: doc_name.to_string(),
                    token: tok.to_string(),
                    kind: CheckedKind::NegatedPath,
                });
                if let Some(resolved) = tree.resolve(tok) {
                    result.failures.push(Finding {
                        doc: doc_name.to_string(),
                        token: tok.to_string(),
                        message: format!(
                            "negated path now resolves at `{resolved}` — the doc's absence \
                             claim is stale"
                        ),
                    });
                }
            } else {
                result.checked.push(Checked {
                    doc: doc_name.to_string(),
                    token: tok.to_string(),
                    kind: CheckedKind::Path,
                });
                if tree.resolve(tok).is_none() {
                    result.failures.push(Finding {
                        doc: doc_name.to_string(),
                        token: tok.to_string(),
                        message: format!(
                            "dangling path: `{tok}` does not resolve under any of the 6 roots"
                        ),
                    });
                }
            }
            continue;
        }

        // Gate: not path-shaped and not identifier-shaped -> class 6.
        if !is_identifier_like(tok) {
            result.ignored.push(Ignored {
                doc: doc_name.to_string(),
                token: tok.to_string(),
                reason: IgnoreReason::NonPathNonSymbolShape,
            });
            continue;
        }

        // Class 9: lowerCamelCase lifecycle field — checked BEFORE
        // attribution so it is ignored even beside a resolvable path.
        if is_lifecycle_field(tok) {
            result.ignored.push(Ignored {
                doc: doc_name.to_string(),
                token: tok.to_string(),
                reason: IgnoreReason::LifecycleField,
            });
            continue;
        }

        // Class 3 / 8: code-shaped -> attempt attribution.
        if is_code_shaped(tok) {
            match attribute_symbol(&spans, idx, text, tree) {
                Some(attributed) => {
                    result.checked.push(Checked {
                        doc: doc_name.to_string(),
                        token: tok.to_string(),
                        kind: CheckedKind::Symbol,
                    });
                    let needle = lookup_needle(tok);
                    if !tree.contents(&attributed).contains(needle) {
                        result.failures.push(Finding {
                            doc: doc_name.to_string(),
                            token: tok.to_string(),
                            message: format!(
                                "symbol `{needle}` not found in attributed `{attributed}`"
                            ),
                        });
                    }
                }
                None => {
                    result.ignored.push(Ignored {
                        doc: doc_name.to_string(),
                        token: tok.to_string(),
                        reason: IgnoreReason::UnattributedSymbol,
                    });
                }
            }
            continue;
        }

        // Class 7: identifier-shaped, not code-shaped -> English prose.
        result.ignored.push(Ignored {
            doc: doc_name.to_string(),
            token: tok.to_string(),
            reason: IgnoreReason::EnglishWordOrAcronym,
        });
    }

    result
}

fn audit_all(docs: &[(String, String)], tree: &dyn TreeResolver) -> Audit {
    let mut combined = Audit::default();
    for (name, text) in docs {
        combined.merge(audit(name, text, tree));
    }
    combined
}

// =======================================================================
// RED FIRST — five fault-injection unit tests over FakeTree fixtures.
// The four real docs are accurate today, so a test written against them
// is green on arrival and proves nothing; these fixtures are the honest
// red, per
// `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`.
// =======================================================================

#[test]
fn the_guard_flags_a_dangling_repo_path() {
    let fixture = "See `docs/patterns/gone.md` for details.\n\n## See also\n";
    let tree = FakeTree(BTreeMap::new());
    let result = audit("fixture.md", fixture, &tree);

    assert!(
        result
            .failures
            .iter()
            .any(|f| f.token == "docs/patterns/gone.md" && f.message.contains("dangling path")),
        "expected a dangling-path failure for `docs/patterns/gone.md`, got: {:?}",
        result.failures.iter().map(Finding::describe).collect::<Vec<_>>()
    );
}

#[test]
fn the_guard_flags_a_symbol_missing_from_its_attributed_file() {
    let fixture = "See `RENAMED_CONST` (`world.rs`) for the builder.\n\n## See also\n";
    let mut map = BTreeMap::new();
    // world.rs resolves under the 4th root; content deliberately lacks
    // the symbol the doc claims it defines.
    map.insert(
        "crates/resinsim-core/tests/uat_steps/world.rs".to_string(),
        "pub struct ResinBuilder;\n".to_string(),
    );
    let tree = FakeTree(map);
    let result = audit("fixture.md", fixture, &tree);

    assert!(
        result.failures.iter().any(|f| {
            f.token == "RENAMED_CONST"
                && f.message.contains("not found")
                && f.message.contains("world.rs")
        }),
        "expected a symbol-missing failure for `RENAMED_CONST` attributed to world.rs, got: {:?}",
        result.failures.iter().map(Finding::describe).collect::<Vec<_>>()
    );
}

#[test]
fn the_guard_flags_a_one_way_see_also_link() {
    let doc_a = (
        "doc-a.md".to_string(),
        "# Doc A\n\n## See also\n\n- `doc-b.md` — the sibling\n".to_string(),
    );
    let doc_b = (
        "doc-b.md".to_string(),
        "# Doc B\n\n## See also\n\n- nothing here cites doc-a\n".to_string(),
    );
    let violations = see_also_symmetry_violations(&[doc_a, doc_b]);

    assert!(
        violations
            .iter()
            .any(|v| v.contains("doc-a.md") && v.contains("doc-b.md") && v.contains("asymmetric")),
        "expected an asymmetric-link violation between doc-a.md and doc-b.md, got: {violations:#?}"
    );
}

#[test]
fn the_guard_flags_an_external_authority_that_became_resolvable() {
    let fixture =
        "Per the skill's `references/autonomous-loop.md`.\n\n## See also\n";
    let mut map = BTreeMap::new();
    // The external authority has, hypothetically, started resolving
    // in-repo — this must fail, not silently keep exempting it (the
    // "exemption list that only grows" trap).
    map.insert(
        "references/autonomous-loop.md".to_string(),
        "surprise, this now exists in-repo".to_string(),
    );
    let tree = FakeTree(map);
    let result = audit("fixture.md", fixture, &tree);

    assert!(
        result.failures.iter().any(|f| {
            f.token == "references/autonomous-loop.md" && f.message.contains("now resolves in-repo")
        }),
        "expected an external-authority-now-resolvable failure, got: {:?}",
        result.failures.iter().map(Finding::describe).collect::<Vec<_>>()
    );
}

#[test]
fn the_guard_ignores_commands_flags_and_skill_references() {
    // Positive control: every one of these is a REAL shape from the
    // plan's evidence base that must NOT be flagged. Without this test
    // the guard could be made green by over-ignoring — the guard's own
    // characteristic failure mode.
    let fixture = "\
Run `cargo uat-field-sim` with `-Aunused_imports` set. The hydrate \
counter is `hydrate.signature`; approval needs `tests_approved`. Grep \
`spec/uat/*.md` by `affectedAreas`. See the skill's `references/` tree \
and project memory `feedback_no_ora_commits.md`, plus the \
`@magistr/issue-lifecycle` model. Files are named `<spec-stem>.feature`. \
This sentence just says `main`.\n\n## See also\n";
    let tree = FakeTree(BTreeMap::new());
    let result = audit("fixture.md", fixture, &tree);

    assert!(
        result.failures.is_empty(),
        "positive control must yield ZERO failures, got: {:?}\nignore ledger:\n{}",
        result.failures.iter().map(Finding::describe).collect::<Vec<_>>(),
        result.ignore_ledger_report()
    );
    assert!(
        result.checked.is_empty(),
        "none of these tokens should be CHECKED (they should all be ignored under a \
         documented reason) — got: {:?}",
        result.checked.iter().map(Checked::describe).collect::<Vec<_>>()
    );

    // Assert specific reasons, not merely the absence of failures — an
    // over-ignoring regression must not slip past a weaker assertion.
    let reason_of = |tok: &str| -> Option<IgnoreReason> {
        result
            .ignored
            .iter()
            .find(|i| i.token == tok)
            .map(|i| i.reason)
    };
    assert_eq!(
        reason_of("hydrate.signature"),
        Some(IgnoreReason::NonPathNonSymbolShape)
    );
    assert_eq!(
        reason_of("tests_approved"),
        Some(IgnoreReason::UnattributedSymbol)
    );
    assert_eq!(
        reason_of("affectedAreas"),
        Some(IgnoreReason::LifecycleField)
    );
    assert_eq!(
        reason_of("references/"),
        Some(IgnoreReason::ExternalAuthority)
    );
    assert_eq!(
        reason_of("feedback_no_ora_commits.md"),
        Some(IgnoreReason::ExternalAuthority)
    );
    assert_eq!(
        reason_of("@magistr/issue-lifecycle"),
        Some(IgnoreReason::NonPathNonSymbolShape)
    );
    assert_eq!(
        reason_of("<spec-stem>.feature"),
        Some(IgnoreReason::NonPathNonSymbolShape)
    );
    assert_eq!(reason_of("main"), Some(IgnoreReason::EnglishWordOrAcronym));
}

// =======================================================================
// GREEN SECOND — five integration tests over the real files, via the
// SAME audit() / RealTree code path the unit tests exercised (no second
// implementation to disagree with itself).
// =======================================================================

#[test]
fn every_backticked_repo_path_resolves() {
    let tree = RealTree::discover();
    let docs = tree.discover_agent_constraints_docs();
    assert!(
        docs.len() >= 4,
        "expected at least the 4 known agent-constraints docs, found {}",
        docs.len()
    );

    let result = audit_all(&docs, &tree);
    let path_failures: Vec<&Finding> = result
        .failures
        .iter()
        .filter(|f| !f.message.contains("symbol"))
        .collect();

    assert!(
        path_failures.is_empty(),
        "{} dangling/negated-path failure(s):\n{}\n\nignore ledger:\n{}",
        path_failures.len(),
        path_failures
            .iter()
            .map(|f| f.describe())
            .collect::<Vec<_>>()
            .join("\n"),
        result.ignore_ledger_report()
    );

    // R7 anti-blindness floor. Today's measured headroom is 91.
    assert!(
        result.path_checks() >= 60,
        "path-check floor breached: only {} path checks ran (floor 60) — a classifier \
         regression may be routing real paths into IGNORED.\n\nignore ledger:\n{}",
        result.path_checks(),
        result.ignore_ledger_report()
    );
}

#[test]
fn every_attributed_symbol_resolves() {
    let tree = RealTree::discover();
    let docs = tree.discover_agent_constraints_docs();
    let result = audit_all(&docs, &tree);

    let symbol_failures: Vec<&Finding> = result
        .failures
        .iter()
        .filter(|f| f.message.contains("symbol"))
        .collect();

    assert!(
        symbol_failures.is_empty(),
        "{} symbol-resolution failure(s):\n{}\n\nignore ledger:\n{}",
        symbol_failures.len(),
        symbol_failures
            .iter()
            .map(|f| f.describe())
            .collect::<Vec<_>>()
            .join("\n"),
        result.ignore_ledger_report()
    );

    // R7 anti-blindness floor. Today's measured headroom is 28.
    assert!(
        result.symbol_checks() >= 15,
        "symbol-check floor breached: only {} symbol checks ran (floor 15) — a classifier \
         regression may be routing real attributed symbols into IGNORED.\n\nignore ledger:\n{}",
        result.symbol_checks(),
        result.ignore_ledger_report()
    );
}

#[test]
fn paths_the_docs_declare_absent_are_absent() {
    let tree = RealTree::discover();
    let docs = tree.discover_agent_constraints_docs();
    let result = audit_all(&docs, &tree);

    let negation_failures: Vec<&Finding> = result
        .failures
        .iter()
        .filter(|f| f.message.contains("negated path"))
        .collect();
    assert!(
        negation_failures.is_empty(),
        "{} negated path(s) now resolve (the doc's absence claim is stale):\n{}",
        negation_failures.len(),
        negation_failures
            .iter()
            .map(|f| f.describe())
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Measured today: 4 (`spikes/`, `knowledge-base/`, `decisions/` in
    // knowledge-base.md; `tests/uat/` in uat-conventions.md).
    assert!(
        result.negated_checks() >= 4,
        "expected at least the 4 known negated-path assertions, found {}",
        result.negated_checks()
    );
}

#[test]
fn the_see_also_graph_is_symmetric_and_orphan_free() {
    let tree = RealTree::discover();
    let docs = tree.discover_agent_constraints_docs();

    let violations = see_also_symmetry_violations(&docs);
    assert!(
        violations.is_empty(),
        "See-also graph among {} agent-constraints/*.md files is not symmetric/orphan-free:\n{}",
        docs.len(),
        violations.join("\n")
    );
}

#[test]
fn the_docs_carry_no_line_number_anchors() {
    let tree = RealTree::discover();
    let docs = tree.discover_agent_constraints_docs();

    let mut all_hits = Vec::new();
    for (name, text) in &docs {
        for hit in line_anchor_violations(text) {
            all_hits.push(format!("{name}: {hit}"));
        }
    }

    assert!(
        all_hits.is_empty(),
        "{} line-number anchor(s) found — the docs deliberately avoid \
         `file.rs:NNN` / \"line NN\" references because they rot the fastest; \
         name the file/symbol instead:\n{}",
        all_hits.len(),
        all_hits.join("\n")
    );
}
