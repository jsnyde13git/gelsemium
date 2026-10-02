use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::io;
use std::iter::{Iterator, Peekable};
use std::path::{Path, PathBuf};
use crate::playlist_data::{Playlist, PlaylistCollection, PlaylistData};

pub fn parse_playlists(
    playlist_str: &str,
) -> Result<PlaylistCollection, PlaylistParseError> {
    let mut playlist_str_iter = playlist_str
        .split_inclusive('\n')
        .enumerate()
        .flat_map(|(row, str)| {
            str.chars()
                .enumerate()
                .map(move |(col, char)| (row, col, char))
        })
        .peekable();
    let mut playlists = PlaylistCollection::new();
    while playlist_str_iter.peek().is_some_and(|(_, _, c)| *c == '[') {
        let playlist_maybe = parse_playlist(&mut playlist_str_iter);
        if let Err(err) = playlist_maybe {
            return Err(err);
        }
        let playlist = playlist_maybe.unwrap();
        playlists.push(playlist);
    }

    return Ok(playlists);
}

fn parse_playlist(
    playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>,
) -> Result<Playlist, PlaylistParseError> {
    // Parse the title.
    let title_result = parse_playlist_title(playlist_iter);
    if let Err(err) = title_result {
        return Err(err);
    }
    let title = title_result.unwrap();

    // Consume tokens until a newline is encountered.
    // Anything on the line after the playlist name is simply ignored.
    while playlist_iter.peek().is_some_and(|(_, _, c)| *c != '\n') {
        println!("{:?}", playlist_iter.next());
    }

    if playlist_iter.peek().is_none() {
        return Ok(Playlist::new(Vec::new(), title));
    }

    // Parse the songs in the list.
    let songs = parse_playlist_songs(playlist_iter);
    // if let Err(err) = songs_result {
    //     return Err(err);
    // }
    // let songs = songs_result.unwrap();

    Ok(Playlist::new(songs, title))
}

fn parse_playlist_title(
    playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>,
) -> Result<String, PlaylistParseError> {
    // Consume the initial [.
    #[allow(clippy::expect_used)]
    let (start_line, start_col, _) = playlist_iter.next().expect("Implementation error in parse_playlist_title: Expected a character, but none was found. Please report to the devs");

    // Consume non-] characters until we reach EOF or one is found.
    let mut playlist_title = String::new();
    while playlist_iter
        .peek()
        .is_some_and(|(_, _, c)| *c != ']' && *c != '\n')
    {
        // Advances iterator
        playlist_title.push(playlist_iter.next().unwrap().2);
    }

    // Assert that there is a ] at the end, and if not, return error
    if !playlist_iter.peek().is_some_and(|(_, _, c)| *c == ']') {
        return Err(PlaylistParseError::UnclosedBracket {
            line: start_line,
            col: start_col,
        });
    }

    // Consume the closing ] and return the title
    playlist_iter.next();
    Ok(playlist_title)
}

fn parse_playlist_songs(
    playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>,
) -> Vec<PlaylistData> {
    // Parse playlist songs until a [ is encountered at the start of a line,
    // or we reach the end of the file.
    let mut songs = Vec::new();
    while playlist_iter.peek().is_some_and(|(_, _, c)| *c != '[') {
        let song_maybe = parse_playlist_song(playlist_iter);
        if let Some(song) = song_maybe {
            songs.push(PlaylistData::Path(PathBuf::from(song)));
        }
    }

    songs
}

fn parse_playlist_song(
    playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>,
) -> Option<String> {
    // Consume characters until there are no more in the line.
    // If any non-whitespace are encountered, set the whitespace flag.
    let mut all_whitespace = true;
    let mut filepath = String::new();
    while playlist_iter.peek().is_some_and(|(_, _, c)| *c != '\n') {
        let c = playlist_iter.next().unwrap().2;
        // advances iterator
        filepath.push(c);
        if !c.is_whitespace() {
            all_whitespace = false;
        }
    }

    // Consume the final newline, if it exists.
    if playlist_iter.peek().is_some_and(|(_, _, c)| *c == '\n') {
        playlist_iter.next();
    }

    // If all whitespace, it's just an empty line. Otherwise it's a filepath. (Maybe.)
    if all_whitespace { None } else { Some(filepath) }
}

#[derive(Debug)]
pub enum PlaylistParseError {
    UnclosedBracket { line: usize, col: usize },
}

impl Error for PlaylistParseError {}

impl Display for PlaylistParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::UnclosedBracket { line, col } => {
                write!(f, "Parse Error: Unclosed bracket at line {line}, col {col}")
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum ExpandDirOptions {
    KeepFolderNames,
    DiscardFolderNames,
}

// Converts the list of folder & filenames into just a list of filenames.
// Prints errors.
pub fn get_playlist_filepaths(
    filepaths: &Vec<String>,
    keep_folders: ExpandDirOptions,
) -> (Vec<(u8, PathBuf)>, Vec<io::Error>) {
    let mut errs: Vec<io::Error> = Vec::new();
    let mut result: Vec<(u8, PathBuf)> = Vec::new();

    for filepath_str in filepaths {
        let path = PathBuf::from(filepath_str);
        let (mut path_result, mut path_errs) = expand_playlist_directory(path, 0, keep_folders);
        errs.append(&mut path_errs);
        result.append(&mut path_result);
    }

    (result, errs)
}

// Receives a path that may or may not be a directory,
// and returns every file & folder in that directory.
// Runs recursively on each subdirectory.
fn expand_playlist_directory(
    dir: PathBuf,
    layer: u8,
    keep_folders: ExpandDirOptions,
) -> (Vec<(u8, PathBuf)>, Vec<io::Error>) {
    if dir.is_dir() {
        // for each entry in dir_path, find its name as a string,
        // then call expand playlist directory recursively on each
        let dir_contents_maybe = dir.read_dir();
        if let Err(err) = dir_contents_maybe {
            return (Vec::new(), vec![err]);
        }
        let dir_contents = dir_contents_maybe.unwrap();

        // Split dir_contents into directories and files,
        // then sort both.
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        // Errors when reading files.
        let mut errs: Vec<io::Error> = Vec::new();
        for entry_maybe in dir_contents {
            match entry_maybe {
                Err(err) => errs.push(err),
                Ok(entry) => {
                    if entry.path().is_dir() {
                        dirs.push(entry.path());
                    } else {
                        files.push(entry.path());
                    }
                }
            }
        }
        dirs.sort();
        files.sort();
        let mut files: Vec<(u8, PathBuf)> = files.into_iter().map(|file| (layer, file)).collect();

        // Recursively obtain all directory contents. Then append this dir's files.
        let mut result: Vec<(u8, PathBuf)> = Vec::new();
        for dir in dirs {
            if let ExpandDirOptions::KeepFolderNames = keep_folders {
                result.push((layer, dir.clone()));
            }
            let mut contents =
                expand_playlist_directory(dir, layer.saturating_add(1), keep_folders);
            result.append(&mut contents.0);
            errs.append(&mut contents.1);
        }
        result.append(&mut files);

        // for entry_maybe in dir_contents {
        //     match entry_maybe {
        //         Err(err) => errs.push(err),
        //         Ok(entry) => {
        //             let mut entry_contents =
        //                 expand_playlist_directory(entry.path(), layer.saturating_add(1));
        //             result.append(&mut entry_contents.0);
        //             errs.append(&mut entry_contents.1);
        //         }
        //     }
        // }

        (result, errs)
    } else {
        // We assume all file paths are UTF-8.
        (vec![(layer, dir)], Vec::new())
    }
}

// Filter the filetypes to be mp3, ogg, wav, perhaps others if compatible
pub fn filter_filetypes(filepaths: &mut Vec<(u8, PathBuf)>) {
    filepaths.retain(|(_, filepath)| {
        // filepath.extension().is_some_and(|extension| {
        //     extension == "mp3" || extension == "ogg" || extension == "wav" || extension == "flac"
        // })
        is_valid_filetype(filepath)
    });
}

pub fn is_valid_filetype(path: &Path) -> bool{
    path.extension().is_some_and(|extension| extension == "mp3" || extension == "ogg" || extension == "wav" || extension == "flac")
}