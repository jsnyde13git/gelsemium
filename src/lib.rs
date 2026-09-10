pub mod player;
pub mod playlist_parser;
pub mod library;

#[allow(clippy::all)]
pub mod ui {
    slint::include_modules!();
}
