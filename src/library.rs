use std::path::PathBuf;

use slint::{Model, SharedString};

/// Slint-compatible element for the Song Library.
/// Can be either a folder or a song.
/// Normally in Rust we'd represent that with a sum type,
/// but Slint doesn't have those. So bool it is.
/// They have the same fields anyway.
#[derive(Clone)]
struct SlintLibraryElem {
    nest_level: i32,
    name: SharedString,
    is_folder: bool,
}

// struct Library {
//     paths: Vec<PathBuf>,
//     model: ModelRc<LibraryElem>,
// }

struct Library{
    contents: LibraryFolder,
}

impl Model for Library{
    type Data = SlintLibraryElem;

    fn row_count(&self) -> usize {
        todo!()
    }

    fn row_data(&self, row: usize) -> Option<Self::Data> {
        todo!()
    }

    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        todo!()
    }
}

struct LibraryFolder{
    // The folders contained in the folder.
    subfolders: Vec<Box<LibraryFolder>>,
    // The songs contained in the folder.
    songs: Vec<LibrarySong>,
    // The folder's name.
    name: SharedString,
    // Whether the folder is hidden in the UI.
    hidden: bool,
}

struct LibrarySong{
    name: SharedString,
    path: PathBuf,
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
