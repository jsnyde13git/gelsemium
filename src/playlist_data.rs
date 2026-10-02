use std::{collections::HashMap, fmt::Display, path::PathBuf};

use crate::playlist_parser::{self, ExpandDirOptions};

// Holds the collection of all the playlists.
pub struct PlaylistCollection{
    playlists: Vec<Playlist>,
}

impl PlaylistCollection{
    pub fn new() -> Self{
        Self {
            playlists: Vec::new()
        }
    }

    pub fn push(&mut self, playlist: Playlist){
        self.playlists.push(playlist);
    }

    pub fn get(&self, name: &str) -> Option<&Playlist>{
        self.playlists.iter().find(|p| p.name == *name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str>{
        // Weird deref-into-take-address construct here.
        // What we're doing here is using that String implements Deref<str>,
        // and then taking the address of that str.
        // Basically we're taking a String and 
        // getting the address of its internal str.
        self.playlists.iter().map(|p| &*p.name)
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
pub struct Playlist{
    data: Vec<PlaylistData>,
    name: String,
}

impl Playlist{
    pub fn new(data: Vec<PlaylistData>, name: String) -> Self{
        Self{
            data,
            name,
        }
    }

    pub fn to_songs(&self, options: ExpandDirOptions) -> (Vec<(u8, PathBuf)>, Vec<std::io::Error>){
        let filepaths = self.to_strings();
        let (paths, errors) = playlist_parser::get_playlist_filepaths(&filepaths, options);

        (paths, errors)
    }

    // temporary messy method
    fn to_strings(&self) -> Vec<String>{
        let mut strs = Vec::new();
        for data in &self.data{
            if let PlaylistData::Path(p) = data{
                strs.push(p.to_string_lossy().to_string());
            }
        }

        strs
    }

    pub fn raw_data(&self) -> &Vec<PlaylistData>{
        &self.data
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
pub enum PlaylistData{
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