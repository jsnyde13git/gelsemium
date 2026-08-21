use std::collections::HashMap;
use std::iter::{Iterator, Peekable};

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