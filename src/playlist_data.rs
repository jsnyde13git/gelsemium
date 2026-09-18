use std::{collections::HashMap, path::PathBuf};

// Holds the collection of all the playlists & their names.
struct PlaylistCollection{
    playlists: HashMap<String, Playlist>,
}

impl PlaylistCollection{
    // Write the entire contents of all playlists to a file, properly-formatted.
    fn write<T>(writer: &mut T)
    where T: std::io::Write{
        todo!()
    }

    fn get(&self, name: &String) -> Option<&Playlist>{
        self.playlists.get(name)
    }
}

// Holds a singular playlist, not including the name.
struct Playlist{
    data: Vec<PlaylistData>,
}

impl Playlist{
    fn write<T>(writer: &mut T)
    where T: std::io::Write{
        todo!()
    }
}

// Holds one line of the playlists file.
// For now, this is either a file or a folder.
// I may add the ability to add additional annotations, though.
enum PlaylistData{
    Path(PathBuf),
}