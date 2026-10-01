use std::{collections::HashMap, fmt::Display, path::PathBuf};

use crate::playlist_parser::{self, ExpandDirOptions};

// Holds the collection of all the playlists.
struct PlaylistCollection{
    playlists: Vec<Playlist>,
}

impl PlaylistCollection{
    fn get(&self, name: &String) -> Option<&Playlist>{
        self.playlists.iter().find(|p| p.name == *name)
    }
}

impl Display for PlaylistCollection{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for playlist in &self.playlists{
            writeln!(f, "{playlist}")?;
        }
        Ok(())
    }
}

// Holds a singular playlist, not including the name.
struct Playlist{
    data: Vec<PlaylistData>,
    name: String,
}

impl Playlist{
    fn into_songs(self, options: ExpandDirOptions) -> (Vec<(u8, PathBuf)>, Vec<std::io::Error>){
        let filepaths = self.into_strings();
        let (paths, errors) = playlist_parser::get_playlist_filepaths(&filepaths, options);

        (paths, errors)
    }

    // temporary messy method
    fn into_strings(self) -> Vec<String>{
        let mut strs = Vec::new();
        for data in &self.data{
            if let PlaylistData::Path(p) = data{
                strs.push(p.to_string_lossy().to_string());
            }
        }

        strs
    }
}

impl Display for Playlist{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "[{}]", self.name)?;
        for data in &self.data{
            writeln!(f, "{data}")?;
        }
        
        Ok(())
    }
}

// Holds one line of the playlists file.
// For now, this is either a file or a folder.
// I may add the ability to add additional annotations, though.
enum PlaylistData{
    Path(PathBuf),
}

impl Display for PlaylistData{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self{
            PlaylistData::Path(path) => {
                writeln!(f, "{}", path.display())?;
            }
        }

        Ok(())
    }
}