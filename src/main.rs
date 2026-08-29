use std::env;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::{Arc, Condvar, Mutex};
use std::{collections::HashMap, fs::File};
use std::io::{self, BufReader};
use std::iter::Iterator;
use std::iter::Peekable;
use rodio::{Decoder, Source};

use rust_music_player::playlist_parser::parse_playlists;
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
            // Read playlists.
            let playlist_file = get_playlists_file();
            let playlist_file_contents_maybe = std::fs::read_to_string(playlist_file);
            if let Err(err) = playlist_file_contents_maybe{
                eprintln!("Error reading file: {err:?}");
                exit(1);
            }
            let playlist_file_contents = playlist_file_contents_maybe.unwrap();

            let playlists_maybe = parse_playlists(&playlist_file_contents);
            if let Err(err) = playlists_maybe{
                eprintln!("Error reading playlists: {err:?}");
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
            play_file_list(player, &playlist, command_queue);
        },
        None => {
            // Default option. Opens the GUI.
            let ui = AppWindow::new().unwrap();
            ui.on_play_playlist(move |playlist_name| println!("{playlist_name}"));
            ui.run().expect("Something failed when starting the UI");
        }
        _ => eprintln!("Unrecognized command")
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
    if cfg!(target_os = "linux"){
        return "playlists.txt".to_string();
    }
    
    panic!("Unsupported OS. Supported OSes are: Linux");
}



fn play_file_list(player: Arc<rodio::Player>, filepaths: &Vec<PathBuf>, command_queue: Arc<PlayerCommandQueue>){
    for filepath in filepaths.iter(){
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


