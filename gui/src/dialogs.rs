//! Modal dialogs: file open, style assign, project save/open, error.
//!
//! Each dialog replaces the main view (simple modal pattern). The dialog
//! frame provides consistent sizing and centering.

use iced::widget::{button, column, container, row, text, text_input, Column};
use iced::{alignment, Color, Element, Length};

use crate::app::{Dialog, Message};

/// Creates the dialog view, replacing the main content.
pub fn view(dialog: &Dialog) -> Element<'_, Message> {
    match dialog {
        Dialog::FileOpen { path } => file_open_dialog(path),
        Dialog::StyleAssign { path, .. } => style_assign_dialog(path),
        Dialog::ProjectSave { path } => project_save_dialog(path),
        Dialog::ProjectOpen { path } => project_open_dialog(path),
        Dialog::Error { message } => error_dialog(message),
    }
}

/// Shared dialog frame: centered container with consistent sizing.
fn dialog_frame<'a>(title: &'a str, content: Column<'a, Message>) -> Element<'a, Message> {
    container(
        column![
            text(title).size(18),
            content,
        ]
        .spacing(16)
        .padding(24),
    )
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

/// File open dialog: enter a path to a spatial file or a `.qgs` project.
///
/// The accepted extensions are the dialog's file filters
/// ([`crate::app::OPEN_FILE_FILTERS`]): `.shp`, `.geojson`, `.json`, `.gpkg`
/// and `.qgs`.
fn file_open_dialog(path: &str) -> Element<'_, Message> {
    let hint = format!(
        "Enter the path to a spatial file or QGIS project ({}):",
        crate::app::open_file_filter()
    );
    dialog_frame(
        "Open Spatial File",
        column![
            text(hint),
            text_input("path/to/file.shp", path)
                .on_input(Message::FileDialogInputChanged)
                .on_submit(Message::FileDialogConfirm),
            row![
                button("Open").on_press(Message::FileDialogConfirm),
                button("Cancel").on_press(Message::Cancel),
            ]
            .spacing(8),
        ]
        .spacing(12),
    )
}

/// Style assignment dialog: enter a path to a .qml style file.
fn style_assign_dialog(path: &str) -> Element<'_, Message> {
    dialog_frame(
        "Assign Style",
        column![
            text("Enter the path to a QML style file (.qml):"),
            text_input("path/to/style.qml", path)
                .on_input(Message::StyleDialogInputChanged)
                .on_submit(Message::StyleDialogConfirm),
            row![
                button("Apply").on_press(Message::StyleDialogConfirm),
                button("Cancel").on_press(Message::Cancel),
            ]
            .spacing(8),
        ]
        .spacing(12),
    )
}

/// Project save dialog: enter a path for the .qgs project file.
fn project_save_dialog(path: &str) -> Element<'_, Message> {
    dialog_frame(
        "Save Project",
        column![
            text("Enter the path for the project file (.qgs):"),
            text_input("project.qgs", path)
                .on_input(Message::ProjectPathChanged)
                .on_submit(Message::ProjectSaveConfirm),
            row![
                button("Save").on_press(Message::ProjectSaveConfirm),
                button("Cancel").on_press(Message::Cancel),
            ]
            .spacing(8),
        ]
        .spacing(12),
    )
}

/// Project open dialog: enter a path to an existing .qgs project file.
fn project_open_dialog(path: &str) -> Element<'_, Message> {
    dialog_frame(
        "Open Project",
        column![
            text("Enter the path to an existing project file (.qgs):"),
            text_input("project.qgs", path)
                .on_input(Message::ProjectPathChanged)
                .on_submit(Message::ProjectOpenConfirm),
            row![
                button("Open").on_press(Message::ProjectOpenConfirm),
                button("Cancel").on_press(Message::Cancel),
            ]
            .spacing(8),
        ]
        .spacing(12),
    )
}

/// Error dialog: shows an error message with a dismiss button.
fn error_dialog(message: &str) -> Element<'_, Message> {
    dialog_frame(
        "Error",
        column![
            text(message),
            button("OK").on_press(Message::DismissError),
        ]
        .spacing(12),
    )
}
