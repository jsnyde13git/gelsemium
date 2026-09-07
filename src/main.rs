use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::error::Error;
use std::path::{PathBuf};
use std::process::exit;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::{fs::File};
use std::io::{self, BufReader};
use std::iter::Iterator;
use rodio::{Decoder, Source};
use slint::{Model, SharedString, ModelRc, ModelNotify, Weak};

use rust_music_player::playlist_parser::{PlaylistParseError, parse_playlists};
use rust_music_player::playlist_parser::get_playlist_filepaths;

slint::include_modules!();

fn main() {
    // Take all non-initial CLI arguments and put them into a string.
    // Read the playlists.
    // Select the playlist in the hashmap.
    // Play it.
    let mut args = env::args().skip(1);
    let command_maybe = args.next();
    if command_maybe.is_none(){
        eprintln!("No arguments given. Valid arguments are: play <playlistname>");
    }
    match command_maybe{
        Some(s) if s == "play" => {
            let playlists_maybe = read_playlists();
            if let Err(err) = playlists_maybe{
                eprintln!("Error reading playlists: {err}");
                exit(1);
            }
            let playlists = playlists_maybe.unwrap();

            // Parse CLI args into a playlist name and access it.
            let playlist_name = args.collect::<Vec<String>>().join(" ");
            let playlist_filepaths_maybe = playlists.get(playlist_name.trim());
            if let None = playlist_filepaths_maybe{
                eprintln!("No playlist with that name found.");
                exit(1);
            }
            let playlist_filepaths = playlist_filepaths_maybe.unwrap();

            // Expand filepaths into a full list.
            let (playlist, errs) = get_playlist_filepaths(playlist_filepaths);
            for err in errs{
                eprintln!("Error: {err:?}");
            }

            // Play playlist.
            println!("Playing playlist {playlist_name}");
            let mut handle = rodio::DeviceSinkBuilder::open_default_sink().expect("Opening default audio stream failed");
            handle.log_on_drop(false);
            let player = Arc::new(rodio::Player::connect_new(&handle.mixer()));
            let command_queue = Arc::new(PlayerCommandQueue::new());
            let command_queue_cli_listener = command_queue.clone();
            std::thread::spawn(move || listen_for_cli_controls(&command_queue_cli_listener));
            println!("Controls:");
            println!("s: Skip");
            println!("d/e: Volume down/up");
            println!("p: Pause/Play");
            // TODO: Remove this clone.
            // I plan to revamp the playlist data structure anyway, 
            // so a little clone here is only temporary.
            // xkcd 2730
            let playlist_paths = SongQueue::new(playlist.into_iter().map(|x| x.1.clone()).collect::<Vec<PathBuf>>(), None);
            play_file_list(player, playlist_paths.0, command_queue);
        },
        None => {
            // Default option. Opens the GUI.
            let ui = AppWindow::new().unwrap();
            // attempt at maximization code
            // might be bugged in slint itself?
            // ui.window().set_maximized(true);
            // let maximize_ptr = ui.as_weak();
            // slint::invoke_from_event_loop(move || {
            //     maximize_ptr.unwrap().window().set_maximized(true); println!("maximized")
            // }).unwrap();

            let playlists_maybe = read_playlists();
            if let Err(err) = playlists_maybe{
                eprintln!("Error reading playlists: {err}");
                exit(1);
            }
            let playlists = playlists_maybe.unwrap();

            // We could refactor this to make it more optimized.
            // The cloning is probably difficult to remove (the UI and backend both need access),
            // but we could remove the .keys and instead have playlists directly return names.
            // If load times become an issue that's an idea, but it's not worth it right now.
            let playlist_names  = 
                slint::ModelRc::new(
                slint::VecModel::from(
                        playlists.keys().map(|k| k.clone().into()).collect::<Vec<slint::SharedString>>()
            ));
            ui.set_playlist_names(playlist_names);


            let playlists_mutex = Arc::new(Mutex::new(playlists));
            let playlists_copy_for_play_ui = playlists_mutex.clone();
            let ui_weak_for_play_ui = ui.as_weak();

            let mut handle = rodio::DeviceSinkBuilder::open_default_sink().expect("Opening default audio stream failed");
            handle.log_on_drop(true);
            let player = Arc::new(rodio::Player::connect_new(&handle.mixer()));
            let player_for_on_play = player.clone();

            ui.on_play_playlist(move |playlist_name| {
                let ui = ui_weak_for_play_ui.unwrap();
                let playlists = playlists_copy_for_play_ui.lock().unwrap();
                // If we're at this point, the user clicked a play playlist button.
                // Given that that button had to exist for the user to click it,
                // I think it's safe to assume the playlist exists.
                let playlist_paths = playlists.get(&playlist_name.to_string()).expect("ERROR: Tried to play playlist that didn't exist; this is a bug");
                let (playlist, errs) = get_playlist_filepaths(playlist_paths);
                let (song_queue, song_model) = SongQueue::new(playlist.into_iter().map(|x| x.1.clone()).collect::<Vec<PathBuf>>(), Some(ui.as_weak()));

                // Spawn the playing thread.
                let command_queue = Arc::new(PlayerCommandQueue::new());
                let command_queue_player = command_queue.clone();
                let player2 = player_for_on_play.clone();
                std::thread::spawn(move || play_file_list(player2, song_queue, command_queue_player));

                // Connect the song model to the UI.
                ui.set_songs_for_selected(song_model); 

                println!("{playlist_name}")
            });


            let playlists_copy_for_get_song_list = playlists_mutex.clone();
            ui.on_get_playlist_song_list(move |name| 
                {
                    let lock = playlists_copy_for_get_song_list
                        .lock()
                        .unwrap();
                    let filepaths_initial = lock
                        .get(name.as_str())
                        .expect("ERROR: Playlist requested does not exist");
                    let (filepaths_final, errs) = get_playlist_filepaths(filepaths_initial);
                    for err in errs{
                        eprintln!("Error accessing file/folder: {err}");
                    }
                    return slint::ModelRc::new(slint::VecModel::from(
                        filepaths_final.into_iter().map(|(level, path)| SongOrFolder{nest_level: level as i32, name: path.as_os_str().to_str().unwrap().to_owned().into()}).collect::<Vec<SongOrFolder>>()
                    ));
                }
            );
            ui.run().expect("Something failed when starting the UI");
        }
        _ => eprintln!("Unrecognized command")
    }
}

fn play_playlist_ui(playlist_name: &SharedString, playlists: HashMap<String, Vec<String>>){
    // get playlist
    // expand it into filenames
    // create songqueue 
}

fn read_playlists() -> Result<HashMap<String, Vec<String>>, Box<dyn Error>>{
    // Read playlists.
    let playlist_file = get_playlists_file();
    let playlist_file_contents = std::fs::read_to_string(playlist_file)?;
    // if let Err(err) = playlist_file_contents_maybe{
    //     eprintln!("Error reading file: {err:?}");
    //     exit(1);
    // }
    // let playlist_file_contents = playlist_file_contents_maybe.unwrap();

    let playlists = parse_playlists(&playlist_file_contents)?;
    Ok(playlists)
}


// Total song data.
struct Song{
    path: PathBuf
}

// Slint-compatible song data.
struct SongSlint{
    name: SharedString,
    nest_level: i32,
}

struct SongQueue{
    immediate: VecDeque<PathBuf>,
    back: VecDeque<PathBuf>,
    // names_model: ModelRc<SharedString>,
    // names_rc: Rc<SongNames>,
    names_ui: Option<Weak<AppWindow>>,
}

struct SongNames{
    immediate: RefCell<VecDeque<SharedString>>,
    back: RefCell<VecDeque<SharedString>>,
    notify: slint::ModelNotify,
}

impl Iterator for SongQueue{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item>{
        // Pop the first from the song names.
        // Horrible high-coupling nonsense, but it's the only
        // way I've found to modify a Slint model from another thread.
        // Not only that, but *updating* the model didn't work,
        // so I have to recreate it every time, with a mountain of clone() calls.
        if let Some(ui) = &self.names_ui{
            let (immediate, back) = self.new_model();
            let _ = ui.upgrade_in_event_loop(move |ui| {
                // I tried doing the downcast thing, but ui.get_songs_for_selected() 
                // didn't want to downcast into SongNames, so it didn't really work.
                ui.set_songs_for_selected(ModelRc::new(Rc::new(SongNames{
                    immediate: immediate,
                    back: back,
                    notify: ModelNotify::default(),
                })));
            });
        }

        if !self.immediate.is_empty(){
            return self.immediate.pop_front();
        }else{
            return self.back.pop_front();
        }
    }
}

impl SongQueue{
    fn new(songs: Vec<PathBuf>, ui_weak: Option<Weak<AppWindow>>) -> (SongQueue, ModelRc<SharedString>){
        // Construct the list of song names.
        // We use lossy conversion from OSString here; 
        // a malformed song name really isn't a big deal.
        // (Well, for now anyway. But it's better than a crash.)
        let inner_names = 
            songs
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned().into())
            .collect::<VecDeque<SharedString>>();
        let names_rc = Rc::new(SongNames{
            immediate: RefCell::new(VecDeque::new()),
            back: RefCell::new(inner_names),
            notify: ModelNotify::default(),
        });

        let names_model = ModelRc::from(names_rc.clone());

        (SongQueue{
            immediate: VecDeque::new(),
            back: VecDeque::from(songs),
            names_ui: ui_weak,
        }, names_model)
    }

    fn new_model(&self) -> (RefCell<VecDeque<SharedString>>, RefCell<VecDeque<SharedString>>){
        let inner_names = 
            self.immediate.iter().chain(self.back.iter())
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned().into())
            .collect::<VecDeque<SharedString>>();
        
            (RefCell::new(VecDeque::new()),
            RefCell::new(inner_names))
    }

    fn queue_immediate(&mut self, item: PathBuf){
        self.immediate.push_back(item);
    }
}

impl Model for SongNames{
    type Data = SharedString;
    
    fn row_count(&self) -> usize{
        self.immediate.borrow().len() + self.back.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<Self::Data>{
        if row < self.immediate.borrow().len(){
            return self.immediate.borrow().get(row).cloned();
        }
        let row_adj = row - self.immediate.borrow().len();
        if row_adj < self.back.borrow().len(){
            return self.back.borrow().get(row_adj).cloned();
        }
        return None;
    }

    fn model_tracker(&self) -> &dyn slint::ModelTracker{
        &self.notify
    }
}

impl SongNames{
    // Interior mutability
    fn pop_front(&self){
        println!("songnames called");
        if !self.immediate.borrow().is_empty(){
            self.immediate.borrow_mut().pop_front();
        }else{
            self.back.borrow_mut().pop_front();
        }
        self.notify.reset();
    }
}

enum PlayerCommand{
    Skip,
    Pause,
    VolumeUp,
    VolumeDown,
    SongFinished,
}

struct PlayerCommandQueue{
    command_list: Mutex<Vec<PlayerCommand>>,
    needs_update: Condvar,
}

impl PlayerCommandQueue{
    fn new() -> PlayerCommandQueue{
        PlayerCommandQueue{
            command_list: Mutex::new(Vec::new()),
            needs_update: Condvar::new(),
        }
    }

    /// Adds one command.
    /// Modifies the mutex.
    fn add_command(&self, cmd: PlayerCommand){
        let mut lock = self.command_list.lock().unwrap();
        lock.push(cmd);
        self.needs_update.notify_all();
    }

    /// Adds multiple commands.
    /// Modifies the mutex.
    fn add_commands(&self, mut cmds: Vec<PlayerCommand>){
        let mut lock = self.command_list.lock().unwrap();
        lock.append(&mut cmds);
        self.needs_update.notify_all();
    }

    /// Takes the commands, *replacing them with Vec::new()*.
    /// Modifies the mutex.
    fn take_commands(&self) -> Vec<PlayerCommand>{
        let mut lock = self.command_list.lock().unwrap();
        std::mem::take(&mut lock)
    }

    fn wait_for_command(&self) {
        let lock = self.command_list.lock().unwrap();
        let _guard = self.needs_update.wait_while(lock, |vec| vec.is_empty()).unwrap();
    }
}

// Consider: Use raw mode?
// For now this uses normal mode
fn listen_for_cli_controls(cmd_queue: &PlayerCommandQueue) -> !{
    let mut buffer = String::new();
    loop{
        let result = io::stdin().read_line(&mut buffer);
        if let Err(err) = result{
            eprintln!("Error reading from stdin: {err}");
            continue;
        }

        match buffer.to_lowercase().trim(){
            "s" => {
                // Skip button
                cmd_queue.add_command(PlayerCommand::Skip);
            }
            "d" => {
                // Volume down
                cmd_queue.add_command(PlayerCommand::VolumeDown);
            }
            "e" => {
                // Volume up
                cmd_queue.add_command(PlayerCommand::VolumeUp);
            }
            "p" => {
                // Pause
                cmd_queue.add_command(PlayerCommand::Pause);
            }
            c => println!("Unrecognized command {c}")
        }

        buffer.clear();
    }
}

fn get_playlists_file() -> String{
    // Check GELSEMIUM_MUSIC_PLAYER_DIR environment variable
    // If set, use that directory
    // Else check if Windows
    // If set, use <user>/Program Files/Local/GelsemiumMusicPlayer
    // Else check if Linux
    // If set, use ~/.local/share/GelsemiumMusicPlayer
    if cfg!(target_os = "linux"){
        return "playlists.txt".to_string();
    }
    
    panic!("Unsupported OS. Supported OSes are: Linux");
}



fn play_file_list<T: Iterator<Item = PathBuf>>(player: Arc<rodio::Player>, mut filepaths: T, command_queue: Arc<PlayerCommandQueue>){
    // let mut file_iter = filepaths.iter();

    while let Some(filepath) = &filepaths.next(){
        // Open file.
        let file_maybe = File::open(filepath);
        if let Err(error) = file_maybe{
            println!("Error reading file {}: {error}", filepath.display());
            continue;
        }
        let file = BufReader::new(file_maybe.unwrap());

        // Try to get an audio source from that file
        let source_maybe = Decoder::try_from(file);
        if let Err(error) = source_maybe{
            println!("Error reading file {}: {error}", filepath.display());
            continue;
        }
        let source = source_maybe.unwrap();

        // Play file
        println!("Playing {}", filepath.display());
        // player.append(source);
        // player.sleep_until_end();
        let player_ref = player.clone();
        let command_queue_ref = command_queue.clone();
        std::thread::spawn(move || play_source(&player_ref, source, &command_queue_ref));

        // Read commands. If Skip or SongFinished appears, move to the next song.
        // Note that Skip, since it skips one and we only ever have one in the queue,
        // immediately causes the player thread to send a SongFinished event.
        let mut next_song = false;
        while !next_song{
            command_queue.wait_for_command();
            let cmds = command_queue.take_commands();
            for cmd in cmds{
                match cmd{
                    PlayerCommand::Pause => {
                        if player.is_paused(){
                            player.play();
                        }else{
                            player.pause();
                        }
                    }
                    PlayerCommand::VolumeUp => {}
                    PlayerCommand::VolumeDown => {}
                    PlayerCommand::Skip => {
                        player.skip_one(); 
                    }
                    PlayerCommand::SongFinished => {next_song = true}
                }
            }
        }
    }
}

/// Meant to run in a separate thread.
/// Will push a PlayerCommand::SongFinished update when done.
fn play_source<T: Source + Send + 'static>(player: &rodio::Player, source: T, command_queue: &PlayerCommandQueue){
    player.append(source);
    player.sleep_until_end();
    command_queue.add_command(PlayerCommand::SongFinished);
}


