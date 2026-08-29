use std::collections::HashMap;
use std::iter::{Iterator, Peekable};
use std::path::PathBuf;
use std::io;

pub fn parse_playlists(playlist_str: &str) -> Result<HashMap<String, Vec<String>>, PlaylistParseError>{
    let mut playlist_str_iter = playlist_str
        .split_inclusive('\n')
        .enumerate()
        .flat_map(|(row, str)| 
            str
            .chars()
            .enumerate()
            .map(move |(col, char)| (row, col, char)))
        .peekable();
    let mut playlists = HashMap::new();
    while playlist_str_iter.peek().is_some_and(|(_, _, c)| *c == '['){
        let playlist_maybe = parse_playlist(&mut playlist_str_iter);
        if let Err(err) = playlist_maybe{
            return Err(err);
        }
        let playlist = playlist_maybe.unwrap();
        playlists.insert(playlist.0, playlist.1);
    }
    
    return Ok(playlists);
}

fn parse_playlist(playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>) -> Result<(String, Vec<String>), PlaylistParseError>{
    // Parse the title.
    let title_result = parse_playlist_title(playlist_iter);
    if let Err(err) = title_result{
        return Err(err);
    }
    let title = title_result.unwrap();

    // Consume tokens until a newline is encountered.
    // Anything on the line after the playlist name is simply ignored.
    while playlist_iter.peek().is_some_and(|(_, _, c)| *c != '\n'){
        println!("{:?}", playlist_iter.next());
    }

    if playlist_iter.peek().is_none(){
        return Ok((title, Vec::new()));
    }

    // Parse the songs in the list.
    let songs_result = parse_playlist_songs(playlist_iter);
    if let Err(err) = songs_result{
        return Err(err);
    }
    let songs = songs_result.unwrap();

    Ok((title, songs))
}

fn parse_playlist_title(playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>) -> Result<String, PlaylistParseError>{
    // Consume the initial [.
    let (start_line, start_col, _) = playlist_iter.next().unwrap();

    // Consume non-] characters until we reach EOF or one is found.
    let mut playlist_title = String::new();
    while playlist_iter.peek().is_some_and(|(_, _, c)| *c != ']' && *c != '\n'){
        // Advances iterator
        playlist_title.push(playlist_iter.next().unwrap().2);
    }

    // Assert that there is a ] at the end, and if not, return error
    if !playlist_iter.peek().is_some_and(|(_, _, c)| *c == ']'){
        return Err(PlaylistParseError::UnclosedBracket{line: start_line, col: start_col})
    }

    // Consume the closing ] and return the title
    playlist_iter.next();
    Ok(playlist_title)
}

fn parse_playlist_songs(playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>) -> Result<Vec<String>, PlaylistParseError>{
    // Parse playlist songs until a [ is encountered at the start of a line,
    // or we reach the end of the file.
    let mut songs = Vec::new();
    while playlist_iter.peek().is_some_and(|(_, _, c)| *c != '['){
        let song_maybe = parse_playlist_song(playlist_iter);
        if let Some(song) = song_maybe{
            songs.push(song);
        }
    }
    
    Ok(songs)
}

fn parse_playlist_song(playlist_iter: &mut Peekable<impl Iterator<Item = (usize, usize, char)>>) -> Option<String>{
    // Consume characters until there are no more in the line.
    // If any non-whitespace are encountered, set the whitespace flag.
    let mut all_whitespace = true;
    let mut filepath = String::new();
    while playlist_iter.peek().is_some_and(|(_, _, c)| *c != '\n'){
        let c = playlist_iter.next().unwrap().2;
        // advances iterator
        filepath.push(c);
        if !c.is_whitespace(){
            all_whitespace = false;
        }
    }

    // Consume the final newline, if it exists.
    if playlist_iter.peek().is_some_and(|(_, _, c)| *c == '\n'){
        playlist_iter.next();
    }
    
    // If all whitespace, it's just an empty line. Otherwise it's a filepath. (Maybe.)
    if all_whitespace{
        None
    }else{
        Some(filepath)
    }
}


#[derive(Debug)]
pub enum PlaylistParseError{
    UnclosedBracket{line: usize, col: usize}
}


// Converts the list of folder & filenames into just a list of filenames.
// Prints errors.
pub fn get_playlist_filepaths(filepaths: &Vec<String>) -> (Vec<(u8, PathBuf)>, Vec<io::Error>){
    let mut errs: Vec<io::Error> = Vec::new();
    let mut result: Vec<(u8, PathBuf)> = Vec::new();
    
    for filepath_str in filepaths{
        let path = PathBuf::from(filepath_str);
        let (mut path_result, mut path_errs) 
            = expand_playlist_directory(path, 0);
        errs.append(&mut path_errs);
        result.append(&mut path_result);
    }

    (result, errs)
}

// Receives a path that may or may not be a directory,
// and returns every file & folder in that directory.
// Runs recursively on each subdirectory.
fn expand_playlist_directory(dir: PathBuf, layer: u8) -> (Vec<(u8, PathBuf)>, Vec<io::Error>){
    if dir.is_dir(){
        // for each entry in dir_path, find its name as a string,
        // then call expand playlist directory recursively on each
        let dir_contents_maybe = dir.read_dir();
        if let Err(err) = dir_contents_maybe{
            return (Vec::new(), vec![err])
        }
        let dir_contents = dir_contents_maybe.unwrap();
        let mut result: Vec<(u8, PathBuf)> = Vec::new();
        let mut errs: Vec<io::Error> = Vec::new();

        for entry_maybe in dir_contents{
            match entry_maybe{
                Err(err) => errs.push(err),
                Ok(entry) => {
                    let mut entry_contents = 
                        expand_playlist_directory(entry.path(), layer.saturating_add(1));
                    result.append(&mut entry_contents.0);
                    errs.append(&mut entry_contents.1);
                }
            }
        }
        
        (result, errs)
    }else{
        // We assume all file paths are UTF-8.
        (vec![(layer, dir)], Vec::new())
    }
}


// Filter the filetypes to be mp3, ogg, wav, perhaps others if compatible
pub fn filter_filetypes(filepaths: &mut Vec<(u8, PathBuf)>){
    filepaths.retain(|(_, filepath)| 
        filepath
        .extension()
        .is_some_and(|extension| 
            extension == "mp3" || 
            extension == "ogg" || 
            extension == "wav" || 
            extension == "flac"));
}