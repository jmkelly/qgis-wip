# iced + wgpu for desktop GUI, headless first

Qt Widgets / Qt3D / QML and 609 `.ui` files cannot be ported to pure Rust; we decided on `iced` with shared `wgpu` map/3D viewports, built last after headless core stabilizes, because it is the only retained pure-Rust toolkit with a GPU canvas story.

Considered Options: Slint (has C++, fails pure-Rust), egui (immediate-mode, wrong for 609 dialogs), Tauri (reintroduces WebView C++), gpui (no widget library). Rejected all for the reasons in parentheses.
