use std::{fmt::Display, path::PathBuf};

use crate::playlist_parser::{self, ExpandDirOptions};

// Holds the collection of all the playlists.
#[derive(Clone)]
pub struct PlaylistCollection{
    playlists: Vec<Playlist>,
}

impl PlaylistCollection{
    #[must_use]
    pub fn new() -> Self{
        Self {
            playlists: Vec::new()
        }
    }

    pub fn push(&mut self, playlist: Playlist){
        self.playlists.push(playlist);
    }

    #[must_use]
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

    pub fn rename(&mut self, original_name: &str, new_name: String) -> Result<(), ()>{
        let playlist_maybe = self.playlists.iter_mut().find(|p| p.name == *original_name);
        if let Some(playlist) = playlist_maybe{
            playlist.name = new_name;
            Ok(())
        }else{
            Err(())
        }
    }
}

impl Default for PlaylistCollection {
    fn default() -> Self {
        Self::new()
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

// Holds a singular playlist.
#[derive(Clone)]
pub struct Playlist{
    data: Vec<PlaylistSong>,
    name: String,
}

impl Playlist{
    #[must_use]
    pub fn new(data: Vec<PlaylistSong>, name: String) -> Self{
        Self{
            data,
            name,
        }
    }

    #[must_use]
    pub fn to_songs(&self, options: ExpandDirOptions) -> (Vec<(u8, PathBuf)>, Vec<std::io::Error>){
        let filepaths = self.to_strings();
        let (paths, errors) = playlist_parser::get_playlist_filepaths(&filepaths, options);

        (paths, errors)
    }

    // temporary messy method
    fn to_strings(&self) -> Vec<String>{
        let mut strs = Vec::new();
        for data in &self.data{
            strs.push(data.path.to_string_lossy().to_string());
        }

        strs
    }

    #[must_use]
    pub fn raw_data(&self) -> &Vec<PlaylistSong>{
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

impl Default for Playlist{
    fn default() -> Self{
        Playlist{
            data: Vec::new(),
            name: String::new(),
        }
    }
}

// Holds one song in the playlists file.
#[derive(Clone)]
pub struct PlaylistSong{
    path: PathBuf,
}

impl PlaylistSong{
    #[must_use]
    pub fn new(path: PathBuf) -> Self{
        Self{
            path
        }
    }

    #[must_use]
    pub fn get_path(&self) -> &PathBuf{
        &self.path
    }

    #[must_use]
    pub fn into_path(self) -> PathBuf{
        self.path
    }
}

impl Display for PlaylistSong{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.display())?;

        Ok(())
    }
}