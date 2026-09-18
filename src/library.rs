use std::{collections::HashMap, io, path::PathBuf};

use slint::{Model, ModelNotify, SharedString};
use crate::ui::LibraryElem;

/// Slint-compatible element for the Song Library.
/// Can be either a folder or a song.
/// Normally in Rust we'd represent that with a sum type,
/// but Slint doesn't have those. So bool it is.
/// They have the same fields anyway.
// #[derive(Clone, Debug)]
// pub struct SlintLibraryElem {
//     nest_level: i32,
//     name: SharedString,
//     is_folder: bool,
// }

// struct Library {
//     paths: Vec<PathBuf>,
//     model: ModelRc<LibraryElem>,
// }

pub struct Library {
    contents: LibraryFolder,
    tracker: ModelNotify,
}

impl Model for Library {
    type Data = LibraryElem;

    fn row_count(&self) -> usize {
        self.contents.elements
    }

    fn row_data(&self, row: usize) -> Option<Self::Data> {
        self.contents.find(row, 0, 0)
    }

    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        &self.tracker
    }
}

struct LibraryFolder {
    // The folders contained in the folder.
    subfolders: Vec<Box<LibraryFolder>>,
    // The songs contained in the folder.
    songs: Vec<LibrarySong>,
    // The folder's name.
    name: SharedString,
    // Number of child elements (computed recursively). Includes both folders & songs.
    elements: usize,
    // Whether the folder is hidden in the UI.
    hidden: bool,
}

impl LibraryFolder {
    fn find(&self, index: usize, mut prev_elems: usize, nest_level: usize) -> Option<LibraryElem>{
        // Attempts to find the element at the requested index.
        // Algorithm explanation:
        // We keep track of the number of previously-encountered elements.
        // First, iterate over folders.
        // If the current index is exactly equal to the number of previously-encountered elements,
        // we have encountered the element we want to return (a folder). So we return it.
        // If the current index is between the current prev elems 
        // and what the prev elems would be after skipping this folder, then it's in that folder.
        // So we enter it.
        // If the current index is greater than what the prev elems would be after skipping this folder,
        // then we skip this folder.
        // After all folders have been exhausted, we know that it must be in this folder. 
        // So we return the appropriate array index.
        for folder in &self.subfolders{
            if index == prev_elems{
                // Must be this folder specifically.
                return Some(LibraryElem{
                    nest_level: nest_level as i32,
                    name: folder.name.clone(),
                    is_folder: true
                })
            }

            if index < folder.elements + prev_elems + 1{
                // element is in this folder
                return folder.find(index, prev_elems + 1, nest_level+1);
            }else{
                // skip this folder
                prev_elems += 1 + folder.elements;
            }
        }

        // no more folders; must be a song within this folder
        if let Some(song) = self.songs.get(index - prev_elems){
            Some(LibraryElem{
                nest_level: nest_level as i32,
                name: song.name.clone(),
                is_folder: false
            })
        }else{
            None
        }
    }
}

struct LibrarySong {
    name: SharedString,
    path: PathBuf,
}

impl Library {
    // TODO: Remove non-audio files.
    pub fn get_library(playlists: &HashMap<String, Vec<String>>) -> Library {
        let Some(library_paths) = playlists.get("Library") else{
            return Library { 
                contents: LibraryFolder { 
                        subfolders: Vec::new(), 
                        songs: Vec::new(), 
                        name: "".into(), 
                        elements: 0, 
                        hidden: false 
                    },
                tracker: ModelNotify::default(),
            }
        };

        let mut subfolders = Vec::new();
        let mut songs = Vec::new();

        for filepath_str in library_paths{
            let path = PathBuf::from(filepath_str);

            if path.is_dir(){
                let (folder_maybe, _) = Self::expand_library_dir(path, 0);
                if let Some(folder) = folder_maybe{
                    subfolders.push(Box::new(folder));
                }
            }else{
                if let Some(name) = path.file_name(){
                    songs.push(LibrarySong{
                        name: name.to_string_lossy().to_string().into(),
                        path
                    });
                }
            }
        }

        Library{
            contents: LibraryFolder{
                elements: songs.len()
                    + subfolders
                        .iter()
                        .fold(0, |acc, folder| acc + folder.elements + 1),
                subfolders,
                songs,
                name: "".into(),
                hidden: false,
            },
            tracker: ModelNotify::default(),
        }
    }

    // Assumes a directory has been received.
    // If a non-directory is passed (or the filesystem has some weird error),
    // returns None instead of a LibraryFolder.
    fn expand_library_dir(dir: PathBuf, layer: i32) -> (Option<LibraryFolder>, Vec<io::Error>) {
        let dir_contents = if dir.is_dir() {
            match dir.read_dir() {
                Ok(contents) => contents,
                Err(err) => return (None, vec![err]),
            }
        } else {
            return (None, Vec::new());
        };

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

        // Map the files into songs.
        let songs: Vec<LibrarySong> = files
            .into_iter()
            .filter_map(|path| {
                if let Some(name) = path.file_name() {
                    Some(LibrarySong {
                        name: name.to_string_lossy().to_string().into(),
                        path,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Recursively call this on all directories.
        let mut subfolders = Vec::new();
        for dir in dirs {
            let (folder_maybe, mut errors) = Self::expand_library_dir(dir, layer);
            errs.append(&mut errors);
            if let Some(folder) = folder_maybe {
                subfolders.push(Box::new(folder));
            }
        }

        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string().into())
            .unwrap_or("ERROR Couldn't read folder name".into());

        (
            Some(LibraryFolder {
                elements: songs.len()
                    + subfolders
                        .iter()
                        .fold(0, |acc, folder| acc + folder.elements + 1),
                subfolders,
                songs,
                name,
                hidden: false,
            }),
            errs,
        )
    }
}

// impl Library {
//     fn new(filepaths: Vec<(i32, PathBuf)>) -> Library {
//         let (nest_levels, paths): (Vec<_>, Vec<_>) = filepaths.into_iter().unzip();

//         // construct library elements
//         let library_elems = zip(nest_levels.iter(), paths.iter())
//             .filter_map(|(nest, path)| {
//                 if let Some(filename) = path.file_name() {
//                     Some(LibraryElem {
//                         nest_level: *nest,
//                         name: filename.to_string_lossy().into_owned().into(),
//                         is_folder: path.is_dir(),
//                     })
//                 } else {
//                     None
//                 }
//             })
//             .collect::<Vec<LibraryElem>>();
//         // let library_elems_inner = Rc::new(library_elems);
//         let library_elems_model = ModelRc::new(VecModel::from(library_elems));

//         Library{
//             paths,
//             model: library_elems_model,
//         }
//     }

//     fn get(&self, index: usize) -> Option<&PathBuf> {
//         self.paths.get(index)
//     }
// }
