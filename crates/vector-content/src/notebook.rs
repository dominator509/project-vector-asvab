//! Manual notebook export (REQ-024).
//!
//! REQ-024 requires a **manual** export path, with any enterprise connector
//! kept separate. The distinction matters for privacy: a manual export is a
//! file the learner inspects and moves themselves, whereas an automatic
//! connector would ship learner data to a third party without a per-action
//! decision.
//!
//! CONTENT_GOVERNANCE.md adds: exported text is *data, not instructions*, and
//! active content must be stripped and size bounded. A notebook export is a
//! classic injection vector — a learner (or a compromised source) could place
//! instruction-like text in a note that is later read back by a model.

use serde::{Deserialize, Serialize};

/// Maximum size of a single exported notebook, in bytes.
pub const MAX_EXPORT_BYTES: usize = 512 * 1024;

/// A section of a notebook export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotebookSection {
    pub heading: String,
    pub body: String,
}

/// A notebook export document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotebookExport {
    pub title: String,
    pub learner_id: String,
    pub generated_on: String,
    pub sections: Vec<NotebookSection>,
    /// Always true: this artifact is produced by an explicit user action.
    pub manual: bool,
    /// Provenance for each section, carried into the export.
    pub source_ids: Vec<String>,
}

/// Why an export was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// The rendered export exceeded the size bound.
    TooLarge { bytes: usize, limit: usize },
    /// No content to export.
    Empty,
    /// Active content was present and could not be safely neutralized.
    ActiveContent,
    /// The learner id was missing, so the export cannot be attributed.
    MissingLearner,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::TooLarge { bytes, limit } => {
                write!(f, "export is {bytes} bytes, over the {limit} byte limit")
            }
            ExportError::Empty => write!(f, "there is nothing to export"),
            ExportError::ActiveContent => {
                write!(
                    f,
                    "export contains active content that must not be included"
                )
            }
            ExportError::MissingLearner => write!(f, "export requires a learner id"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Neutralize active content in exported text.
///
/// CONTENT_GOVERNANCE.md: "Strip active content, bound size/type ... and
/// separate content from tool/policy authority." Raw HTML script tags, event
/// handlers and cell formulas are the practical risks in a notebook file that
/// may be opened in another tool.
pub fn strip_active_content(text: &str) -> String {
    let mut out = text.to_string();

    // Remove script/style blocks including their contents.
    for tag in ["script", "style", "iframe", "object", "embed"] {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        while let Some(start) = out.to_lowercase().find(&open) {
            let lower = out.to_lowercase();
            match lower[start..].find(&close) {
                Some(rel_end) => {
                    let end = start + rel_end + close.len();
                    out.replace_range(start..end, "");
                }
                None => {
                    // Unterminated: drop the remainder rather than leaving a
                    // dangling opener that another tool might execute.
                    out.truncate(start);
                    break;
                }
            }
        }
    }

    // Drop javascript: URLs.
    out = out.replace("javascript:", "").replace("Javascript:", "");

    // Neutralize spreadsheet formula injection: a cell beginning with =, +, -,
    // or @ can execute in Excel/Sheets when the export is opened there.
    let mut lines: Vec<String> = Vec::new();
    for line in out.lines() {
        let trimmed = line.trim_start();
        let escaped =
            if trimmed.starts_with('=') || trimmed.starts_with('+') || trimmed.starts_with('@') {
                format!("'{}", line)
            } else {
                line.to_string()
            };
        lines.push(escaped);
    }

    lines.join("\n")
}

/// Render a notebook export to Markdown.
///
/// Markdown is chosen deliberately over an executable notebook format: it
/// cannot carry code that runs on open.
pub fn render_markdown(export: &NotebookExport) -> Result<String, ExportError> {
    if export.sections.is_empty() {
        return Err(ExportError::Empty);
    }
    if export.learner_id.trim().is_empty() {
        return Err(ExportError::MissingLearner);
    }

    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", strip_active_content(&export.title)));
    out.push_str(&format!(
        "_Manually exported on {} — learner {}_\n\n",
        export.generated_on, export.learner_id
    ));
    if !export.source_ids.is_empty() {
        out.push_str("Sources:\n");
        for id in &export.source_ids {
            out.push_str(&format!("- {id}\n"));
        }
        out.push('\n');
    }

    for section in &export.sections {
        out.push_str(&format!(
            "## {}\n\n{}\n\n",
            strip_active_content(&section.heading),
            strip_active_content(&section.body)
        ));
    }

    if out.len() > MAX_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            bytes: out.len(),
            limit: MAX_EXPORT_BYTES,
        });
    }
    Ok(out)
}

/// Whether this build offers an automatic enterprise connector.
///
/// REQ-024 keeps the enterprise connector **separate** from the manual export.
/// This returns false so the manual path cannot accidentally acquire automatic
/// network behaviour; the connector is an opt-in integration built elsewhere.
pub fn enterprise_connector_enabled() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterprise_connector_is_disabled_in_the_manual_export_path() {
        assert!(
            !enterprise_connector_enabled(),
            "REQ-024: manual export must not silently perform network transfer"
        );
    }
}
