use rodio::Player;
use slint::{ComponentHandle, ModelRc, PlatformError, SharedString, VecModel, Weak};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io::{self};
use std::iter::{Iterator, zip};
use std::path::PathBuf;
use std::process::exit;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use rust_music_player::player::{PlayerCommand, PlayerCommandQueue, SongQueue, play_file_list};
use rust_music_player::playlist_parser::get_playlist_filepaths;
use rust_music_player::playlist_parser::parse_playlists;
use rust_music_player::ui::AppWindow;

fn main() {
    // Take all non-initial CLI arguments and put them into a string.
    // Read the playlists.
    // Select the playlist in the hashmap.
    // Play it.
    let mut args = env::args().skip(1);
    let command_maybe = args.next();
    if command_maybe.is_none() {
        eprintln!("No arguments given. Valid arguments are: play <playlistname>");
    }
    match command_maybe {
        Some(s) if s == "play" => {
            // Play from the CLI.
            let playlist_name = args.collect::<Vec<String>>().join(" ");
            play_cli(playlist_name);
        }
        None => {
            // Default option. Opens the GUI.
            let res = play_gui();
            if let Err(error) = res {
                eprintln!("Error with the GUI: {error}");
            }
        }
        _ => eprintln!("Unrecognized command"),
    }
}

fn play_cli(playlist_name: String) {
    // let playlists_maybe = read_playlists();
    // if let Err(err) = playlists_maybe{
    //     eprintln!("Error reading playlists: {err}");
    //     exit(1);
    // }
    // let playlists = playlists_maybe.unwrap();
    let playlists = match read_playlists() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("Error reading playlists: {err}");
            exit(1);
        }
    };

    // Parse CLI args into a playlist name and access it.
    let playlist_filepaths_maybe = playlists.get(playlist_name.trim());
    if let None = playlist_filepaths_maybe {
        eprintln!("No playlist with that name found.");
        exit(1);
    }
    let playlist_filepaths = playlist_filepaths_maybe.unwrap();

    // Expand filepaths into a full list.
    let (playlist, errs) = get_playlist_filepaths(playlist_filepaths);
    for err in errs {
        eprintln!("Error: {err:?}");
    }

    // Play playlist.
    println!("Playing playlist {playlist_name}");
    let mut handle =
        rodio::DeviceSinkBuilder::open_default_sink().expect("Opening default audio stream failed");
    handle.log_on_drop(false);
    let player = Arc::new(rodio::Player::connect_new(&handle.mixer()));
    let command_queue = Arc::new(PlayerCommandQueue::new());
    let command_queue_cli_listener = command_queue.clone();
    std::thread::spawn(move || listen_for_cli_controls(&command_queue_cli_listener));
    println!("Controls:");
    println!("s: Skip");
    println!("d/e: Volume down/up");
    println!("p: Pause/Play");

    let filepaths = playlist.into_iter().map(|x| x.1).collect::<Vec<PathBuf>>();
    let playlist_paths = SongQueue::new(
        filepaths.clone(),
        None,
    );
    play_file_list(player, filepaths.into_iter(), playlist_paths.0, command_queue);
}

fn play_gui() -> Result<(), PlatformError> {
    let ui = AppWindow::new()?;
    // attempt at maximization code
    // might be bugged in slint itself?
    // ui.window().set_maximized(true);
    // let maximize_ptr = ui.as_weak();
    // slint::invoke_from_event_loop(move || {
    //     maximize_ptr.unwrap().window().set_maximized(true); println!("maximized")
    // }).unwrap();

    let playlists = match read_playlists() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("Error reading playlists: {err}");
            exit(1);
        }
    };

    let lib_default = Vec::new();
    let library_initial = playlists.get("Library").unwrap_or(&lib_default);
    let (library_paths, _) = get_playlist_filepaths(library_initial);
    let library_paths = library_paths
        .into_iter()
        .map(|(depth, path)| (depth as i32, path))
        .collect::<Vec<(i32, PathBuf)>>();

    // We could refactor this to make it more optimized.
    // The cloning is probably difficult to remove (the UI and backend both need access),
    // but we could remove the .keys and instead have playlists directly return names.
    // If load times become an issue that's an idea, but it's not worth it right now.
    let playlist_names = slint::ModelRc::new(slint::VecModel::from(
        playlists
            .keys()
            .map(|k| k.clone().into())
            .collect::<Vec<slint::SharedString>>(),
    ));
    ui.set_playlist_names(playlist_names);

    let playlists_mutex = Arc::new(Mutex::new(playlists));
    let playlists_copy_for_play_ui = playlists_mutex.clone();
    let ui_weak_for_play_ui = ui.as_weak();

    let mut handle =
        rodio::DeviceSinkBuilder::open_default_sink().expect("Opening default audio stream failed");
    handle.log_on_drop(true);
    let player = Arc::new(rodio::Player::connect_new(&handle.mixer()));
    let player_for_on_play = player.clone();

    ui.on_play_playlist(move |playlist_name| {
        let ui = &ui_weak_for_play_ui;
        let player = &player_for_on_play;
        ui_on_play_playlist(playlist_name, &ui, &playlists_copy_for_play_ui, &player);
    });

    ui.run()
}

fn ui_on_play_playlist(
    playlist_name: SharedString,
    ui: &Weak<AppWindow>,
    playlists: &Mutex<HashMap<String, Vec<String>>>,
    player: &Arc<Player>,
) {
    let ui = ui.unwrap();
    let playlists = playlists.lock().unwrap();
    // If we're at this point, the user clicked a play playlist button.
    // Given that that button had to exist for the user to click it,
    // I think it's safe to assume the playlist exists.
    #[allow(clippy::unwrap_used)]
    let playlist_paths = playlists
        .get(&playlist_name.to_string())
        .expect("ERROR: Tried to play playlist that didn't exist; this is a bug");
    let (playlist, _) = get_playlist_filepaths(playlist_paths);
    let filepaths = playlist
            .into_iter()
            .map(|x| x.1.clone())
            .collect::<Vec<PathBuf>>();
    let (song_queue, song_model) = SongQueue::new(
        filepaths.clone(),
        Some(ui.as_weak()),
    );

    // Spawn the playing thread.
    let command_queue = Arc::new(PlayerCommandQueue::new());
    let command_queue_player = command_queue.clone();
    let player2 = player.clone();
    std::thread::spawn(move || play_file_list(player2, filepaths.into_iter(), song_queue, command_queue_player));

    // Connect the song model to the UI.
    ui.set_songs_for_selected(song_model);

    // Connect the command queue to the UI.
    let cqueue = command_queue.clone();
    ui.on_pause(move || cqueue.add_command(PlayerCommand::Pause));
    let cqueue = command_queue.clone();
    ui.on_skip(move || cqueue.add_command(PlayerCommand::Skip));
    let cqueue = command_queue.clone();
    ui.on_volume_up(move || cqueue.add_command(PlayerCommand::VolumeUp));
    let cqueue = command_queue.clone();
    ui.on_volume_down(move || cqueue.add_command(PlayerCommand::VolumeDown));

    println!("Playing {playlist_name}")
}

fn read_playlists() -> Result<HashMap<String, Vec<String>>, Box<dyn Error>> {
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

// Consider: Use raw mode?
// For now this uses normal mode
fn listen_for_cli_controls(cmd_queue: &PlayerCommandQueue) -> ! {
    let mut buffer = String::new();
    loop {
        let result = io::stdin().read_line(&mut buffer);
        if let Err(err) = result {
            eprintln!("Error reading from stdin: {err}");
            continue;
        }

        match buffer.to_lowercase().trim() {
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
            c => println!("Unrecognized command {c}"),
        }

        buffer.clear();
    }
}

fn get_playlists_file() -> String {
    // Check GELSEMIUM_MUSIC_PLAYER_DIR environment variable
    // If set, use that directory
    // Else check if Windows
    // If set, use <user>/Program Files/Local/GelsemiumMusicPlayer
    // Else check if Linux
    // If set, use ~/.local/share/GelsemiumMusicPlayer
    if cfg!(target_os = "linux") {
        return "playlists.txt".to_string();
    }

    panic!("Unsupported OS. Supported OSes are: Linux");
}
