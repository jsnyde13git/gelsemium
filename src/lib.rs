pub mod library;
pub mod player;
pub mod playlist_parser;
pub mod playlist_data;

#[allow(clippy::all)]
#[allow(clippy::pedantic)]
#[allow(clippy::restriction)]
pub mod ui {
    slint::include_modules!();
}
