//! Style assignment dialog: pick a `.qml` file, preview its renderer, apply.
//!
//! Bead qgis-yjr (E14.4). The dialog is a *file picker* for `.qml` styles: it
//! lists the `.qml` files in a browsable directory, previews the renderer the
//! file declares (single symbol, categorized, or graduated), and applies the
//! chosen style to the selected layer.
//!
//! The pure helpers ([`list_qml_files`], [`validate_style_path`],
//! [`parse_style`], [`describe_renderer`]) are deliberately free of `iced`
//! so they can be unit-tested without a window.

use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, row, scrollable, text, text_input, Column};
use iced::{alignment, Color, Element, Length};

use qgis_style::{parse_qml_file, Renderer, StyleModel};

use crate::app::Message;

/// A `.qml` style file discovered by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleCandidate {
    /// Path to the `.qml` file.
    pub path: PathBuf,
    /// Display label (the file name).
    pub label: String,
}

/// Lists the `.qml` files in `dir`, sorted by label.
///
/// A missing or unreadable directory yields an empty list rather than an
/// error: the picker simply shows nothing to choose from.
pub fn list_qml_files(dir: impl AsRef<Path>) -> Vec<StyleCandidate> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut candidates: Vec<StyleCandidate> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("qml")))
        .filter_map(|path| {
            let label = path.file_name()?.to_str()?.to_string();
            Some(StyleCandidate { path, label })
        })
        .collect();
    candidates.sort_by(|a, b| a.label.cmp(&b.label));
    candidates
}

/// The default directory to browse for styles.
///
/// Prefers `<cwd>/styles` (the repository layout used by the acceptance run),
/// falling back to the current directory when it is absent.
pub fn default_styles_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let styles = cwd.join("styles");
    if styles.is_dir() {
        styles
    } else {
        cwd
    }
}

/// The directory the picker should browse for a given path input.
///
/// Uses the input's parent when it is an existing directory, otherwise
/// [`default_styles_dir`].
pub fn browse_dir_for(input: &str) -> PathBuf {
    if !input.trim().is_empty() {
        if let Some(parent) = Path::new(input).parent() {
            if parent.is_dir() {
                return parent.to_path_buf();
            }
        }
    }
    default_styles_dir()
}

/// Short renderer kind: `"single"`, `"categorized"`, or `"graduated"`.
pub fn renderer_kind(renderer: &Renderer) -> &'static str {
    match renderer {
        Renderer::Single(_) => "single",
        Renderer::Categorized { .. } => "categorized",
        Renderer::Graduated { .. } => "graduated",
    }
}

/// Human-readable one-line summary of a renderer, naming its attribute and
/// class/range count for the classifying renderers.
pub fn describe_renderer(renderer: &Renderer) -> String {
    match renderer {
        Renderer::Single(_) => "single symbol".to_string(),
        Renderer::Categorized { attr, classes } => {
            format!("categorized on \"{}\" ({} classes)", attr, classes.len())
        }
        Renderer::Graduated { attr, ranges } => {
            format!("graduated on \"{}\" ({} ranges)", attr, ranges.len())
        }
    }
}

/// Validates a user-supplied style path *by name* before it is parsed.
///
/// Rejects an empty path, a path whose extension is not `qml`, and a path that
/// does not exist. A rejected path is never silently ignored.
pub fn validate_style_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Enter or pick a .qml style file".to_string());
    }
    let has_qml_ext = Path::new(trimmed)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("qml"));
    if !has_qml_ext {
        return Err(format!("Not a .qml style file: \"{}\" (expected a .qml extension)", trimmed));
    }
    if !Path::new(trimmed).is_file() {
        return Err(format!("Style file not found: \"{}\"", trimmed));
    }
    Ok(())
}

/// Validates and parses a `.qml` style file.
pub fn parse_style(path: &str) -> Result<StyleModel, String> {
    validate_style_path(path)?;
    parse_qml_file(path)
}

/// A one-line renderer preview for the dialog: the renderer summary on
/// success, or the validation/parse error to show inline.
pub fn preview(path: &str) -> Result<String, String> {
    parse_style(path).map(|model| describe_renderer(&model.renderer))
}

/// The dialog view: path input, `.qml` file list, renderer preview, actions.
pub fn view<'a>(layer_name: Option<&'a str>, path: &'a str) -> Element<'a, Message> {
    let target = match layer_name {
        Some(name) => format!("Assign a .qml style to layer \"{name}\""),
        None => "Assign a .qml style".to_string(),
    };

    let mut content = column![
        text(target),
        text("Choose a .qml style file:"),
        text_input("path/to/style.qml", path)
            .on_input(Message::StyleDialogInputChanged)
            .on_submit(Message::StyleDialogConfirm),
    ]
    .spacing(12);

    // Renderer preview: name the renderer type, or explain the problem.
    match preview(path) {
        Ok(summary) => {
            content = content.push(text(format!("Renderer: {summary}")).size(13));
        }
        Err(err) => {
            content = content.push(text(err).size(13).color(Color::from_rgb(0.7, 0.1, 0.1)));
        }
    }

    // `.qml` file picker: list the candidates in the browsed directory.
    let dir = browse_dir_for(path);
    let candidates = list_qml_files(&dir);
    if !candidates.is_empty() {
        let mut list =
            column![text(format!("Available in {}:", dir.display())).size(12)].spacing(2);
        for candidate in candidates {
            list = list.push(
                button(text(candidate.label).width(Length::Fill))
                    .style(button::text)
                    .on_press(Message::StyleFilePicked(candidate.path.display().to_string())),
            );
        }
        content = content.push(scrollable(list).height(Length::Fixed(160.0)));
    }

    content = content.push(
        row![
            button("Apply").on_press(Message::StyleDialogConfirm),
            button("Cancel").on_press(Message::Cancel),
        ]
        .spacing(8),
    );

    dialog_frame("Assign Style", content)
}

/// Shared dialog frame: centered container with consistent sizing.
fn dialog_frame<'a>(title: &'a str, content: Column<'a, Message>) -> Element<'a, Message> {
    container(column![text(title).size(18), content].spacing(16).padding(24))
        .style(|_theme| container::Style {
            background: Some(Color::from_rgb(0.98, 0.98, 0.98).into()),
            ..Default::default()
        })
        .width(Length::Fixed(500.0))
        .height(Length::Shrink)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styles_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../styles")
    }

    #[test]
    fn lists_only_qml_files_sorted() {
        let candidates = list_qml_files(styles_dir());
        assert_eq!(candidates.len(), 11, "styles/ has 11 .qml fixtures");
        assert!(
            candidates.iter().all(|c| c.path.extension().is_some_and(|e| e == "qml")),
            "only .qml files should be listed"
        );
        let labels: Vec<&str> = candidates.iter().map(|c| c.label.as_str()).collect();
        let mut sorted = labels.clone();
        sorted.sort();
        assert_eq!(labels, sorted, "candidates should be sorted by label");
        assert!(labels.contains(&"polys.qml"));
    }

    #[test]
    fn missing_directory_yields_empty_list() {
        let candidates = list_qml_files(styles_dir().join("does-not-exist"));
        assert!(candidates.is_empty());
    }

    #[test]
    fn describe_renderer_covers_all_three_types() {
        let single =
            qgis_style::parse_qml_file(styles_dir().join("inverted_polys_single.qml")).unwrap();
        assert_eq!(renderer_kind(&single.renderer), "single");
        assert_eq!(describe_renderer(&single.renderer), "single symbol");

        let categorized = qgis_style::parse_qml_file(styles_dir().join("polys.qml")).unwrap();
        assert_eq!(renderer_kind(&categorized.renderer), "categorized");
        let desc = describe_renderer(&categorized.renderer);
        assert!(desc.contains("categorized"), "{desc}");
        assert!(desc.contains("Name"), "{desc}");
        assert!(desc.contains('2'), "{desc}");

        let graduated =
            qgis_style::parse_qml_file(styles_dir().join("points_graduated_symbol.qml")).unwrap();
        assert_eq!(renderer_kind(&graduated.renderer), "graduated");
        let desc = describe_renderer(&graduated.renderer);
        assert!(desc.contains("graduated"), "{desc}");
        assert!(desc.contains("Importance"), "{desc}");
        assert!(desc.contains('3'), "{desc}");
    }

    #[test]
    fn validate_rejects_non_qml_by_name() {
        let err = validate_style_path("/tmp/not-a-style.txt").unwrap_err();
        assert!(err.contains("qml"), "error should name the .qml requirement: {err}");
    }

    #[test]
    fn validate_rejects_empty_path() {
        assert!(validate_style_path("").is_err());
    }

    #[test]
    fn validate_rejects_missing_file() {
        let err = validate_style_path("/tmp/qgis-yjr-missing.qml").unwrap_err();
        assert!(err.contains("not found") || err.contains("exist"), "{err}");
    }

    #[test]
    fn parse_style_reports_the_renderer() {
        let model = parse_style(styles_dir().join("polys.qml").to_str().unwrap()).unwrap();
        assert!(matches!(model.renderer, Renderer::Categorized { .. }));
    }

    #[test]
    fn browse_dir_uses_parent_of_input() {
        let input = styles_dir().join("polys.qml");
        assert_eq!(browse_dir_for(input.to_str().unwrap()), styles_dir());
    }

    #[test]
    fn preview_names_the_renderer_or_the_problem() {
        let ok = preview(styles_dir().join("polys.qml").to_str().unwrap()).unwrap();
        assert!(ok.contains("categorized on \"Name\""), "{ok}");

        let err = preview("/tmp/not-a-style.txt").unwrap_err();
        assert!(err.contains("qml"), "{err}");
    }
}
