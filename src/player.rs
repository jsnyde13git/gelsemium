use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use rodio::decoder::DecoderBuilder;
use rodio::{Decoder, Player, Source};
use slint::{Model, ModelNotify, ModelRc, SharedString, Weak};

use crate::ui::AppWindow;

// Allowed because it triggers on the Arcs, which while *technically* don't
// need to be passed by reference, I don't think there's really a lot of
// benefit to changing it.
#[allow(clippy::needless_pass_by_value)]
pub fn play_file_list(
    player: Arc<rodio::Player>,
    song_queue: Arc<Mutex<SongQueue>>,
    command_queue: Arc<PlayerCommandQueue>,
) {
    // Put one song in the queue before the main loop.
    // That way we'll always have two songs in the queue, letting us do gapless. (Hopefully)
    // Not the best code I've written but a little break is fine in a five-line thing I'm sure.
    // Also this reading mutex is mainly used later.
    let reading_mutex = Arc::new(Mutex::new(()));
    while let Some(filepath) = &song_queue.lock().unwrap().next() {
        // song_queue.lock().unwrap().next();
        if let Ok(_) = append_next_song(&player, filepath, &reading_mutex) {
            println!("Playing {}", filepath.display());
            break;
        }
    }

    // Create the end song detector thread.
    // MUST be after the first file is added to the queue.
    // Otherwise it'll just blow through all the filenames before one even gets added.
    // Well, now that there's the mutex thing, I could just lock the mutex,
    // spawn the thread, read the thing, and then unlock the mutex.
    // That seems more complicated and messy though.
    let player_ref = player.clone();
    let command_queue_ref = command_queue.clone();
    let finished_playing = Arc::new(true);
    let finished_playing_ref = finished_playing.clone();
    let reading_mutex_ref = reading_mutex.clone();
    let song_queue_ref = song_queue.clone();
    std::thread::spawn(move || {
        end_song_detector(
            &player_ref,
            &command_queue_ref,
            song_queue_ref,
            &finished_playing_ref,
            &reading_mutex_ref,
        )
    });

    while let Some(filepath) = {song_queue.lock().unwrap().next().clone()} {
        // player.append(source);
        // player.sleep_until_end();
        // std::thread::spawn(move || play_source(&player_ref, source, &command_queue_ref));
        // if let Err(_) = append_next_song(&player, filepath) {
        //     continue;
        // }
        let filepath_copy = filepath.clone();
        let player_copy = player.clone();
        let reading_mutex_copy = reading_mutex.clone();
        std::thread::spawn(move || append_next_song(&player_copy, &filepath_copy, &reading_mutex_copy));

        command_queue.take_commands();
        wait_for_command(&command_queue, player.clone(), reading_mutex.clone());
    }
    // We wait one time at the end
    wait_for_command(&command_queue, player.clone(), reading_mutex.clone());
}

fn wait_for_command(command_queue: &PlayerCommandQueue, player: Arc<Player>, reading_mutex: Arc<Mutex<()>>){
    // Read commands. If Skip or SongFinished appears, move to the next song.
    // Note that Skip, since it skips one and we only ever have one in the queue,
    // immediately causes the player thread to send a SongFinished event.
    let mut next_song = false;
    let mut queued_immediate = false;
    while !next_song {
        command_queue.wait_for_command();
        let cmds = command_queue.take_commands();
        for cmd in cmds {
            match cmd {
                PlayerCommand::Pause => {
                    if player.is_paused() {
                        player.play();
                    } else {
                        player.pause();
                    }
                }
                PlayerCommand::VolumeUp => {}
                PlayerCommand::VolumeDown => {}
                PlayerCommand::Skip => {
                    player.skip_one();
                }
                PlayerCommand::SongFinished => {
                    next_song = true;
                    if queued_immediate{
                        player.skip_one();
                    }
                },
                PlayerCommand::QueueImmediate(_) =>{
                    // let player_copy = player.clone();
                    // let reading_mutex_copy = reading_mutex.clone();
                    // std::thread::spawn(move || append_next_song(&player_copy, &path, &reading_mutex_copy));
                    queued_immediate = true;
                }
            }
        }
    }
}

fn append_songs_thread(){
    // Append a song, and get its duration.
    // Then, loop until kill command given:
    //   Wait until either N-1 seconds are up or the skip command has been given.
    //   Get the next song & its duration.
    //   Append the song to the player. 
}

fn append_next_song(player: &rodio::Player, filepath: &PathBuf, reading_mutex: &Mutex<()>) -> Result<(), ()> {
    // Lock the mutex.
    let _lock = reading_mutex.lock();

    // Open file.
    // Either open file as a BufReader, or skip to the next one if it fails.
    let file = match File::open(filepath) {
        Ok(f) => BufReader::new(f),
        Err(err) => {
            eprintln!("Error reading file {}: {err}", filepath.display());
            return Err(());
        }
    };

    // Try to get an audio source from that file
    let source = match DecoderBuilder::new()
        .with_data(file)
        .with_gapless(true)
        .build()
    {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Error reading file {}: {err}", filepath.display());
            return Err(());
        }
    };

    // Play file
    player.append(source);
    Ok(())
}

/// Meant to run in a separate thread.
/// Will push a `PlayerCommand::SongFinished` update when done.
fn play_source<T: Source + Send + 'static>(
    player: &rodio::Player,
    source: T,
    command_queue: &PlayerCommandQueue,
) {
    player.append(source);
    player.sleep_until_end();
    command_queue.add_command(PlayerCommand::SongFinished);
}

fn end_song_detector(
    player: &rodio::Player,
    command_queue: &PlayerCommandQueue,
    song_queue: Arc<Mutex<SongQueue>>,
    keep_detecting: &bool,
    sync_mutex: &Mutex<()>,
) {
    while *keep_detecting {
        // Attempt locking the mutex.
        // If a song is currently being read, 
        // the mutex will be locked by the main player,
        // so this won't be able to send song queue updates until that's done.
        {
            let _lock = sync_mutex.lock();
        }
        player.sleep_until_end();
        command_queue.add_command(PlayerCommand::SongFinished);
        // if let Some(song) = song_queue.lock().unwrap().next() {
        //     println!("Playing {}", song.display());
        // }
    }
}

#[derive(Debug)]
pub enum PlayerCommand {
    Skip,
    Pause,
    VolumeUp,
    VolumeDown,
    SongFinished,
    QueueImmediate(PathBuf),
}

pub struct PlayerCommandQueue {
    command_list: Mutex<Vec<PlayerCommand>>,
    needs_update: Condvar,
}

impl PlayerCommandQueue {
    pub fn new() -> PlayerCommandQueue {
        PlayerCommandQueue {
            command_list: Mutex::new(Vec::new()),
            needs_update: Condvar::new(),
        }
    }

    /// Adds one command.
    /// Modifies the mutex.
    pub fn add_command(&self, cmd: PlayerCommand) {
        let mut lock = self.command_list.lock().unwrap();
        lock.push(cmd);
        self.needs_update.notify_all();
    }

    /// Takes the commands, *replacing them with `Vec::new()`*.
    /// Modifies the mutex.
    fn take_commands(&self) -> Vec<PlayerCommand> {
        let mut lock = self.command_list.lock().unwrap();
        std::mem::take(&mut lock)
    }

    fn wait_for_command(&self) {
        let lock = self.command_list.lock().unwrap();
        let _guard = self
            .needs_update
            .wait_while(lock, |vec| vec.is_empty())
            .unwrap();
    }
}

pub struct SongQueue {
    immediate: VecDeque<PathBuf>,
    back: VecDeque<PathBuf>,
    // Holds the currently-playing song.
    // If this = None, nothing has been pulled yet,
    // so it'll be the last thing we pulled.
    // If this = Some, then something has been pulled,
    // and a request coming in means it's for the one to play *after* this.
    currently_playing: Option<PathBuf>,
    next_playing: NextPlayingStates,
    last_pull_was_back: bool,
    // names_model: ModelRc<SharedString>,
    // names_rc: Rc<SongNames>,
    names_ui: Option<Weak<AppWindow>>,
}

#[derive(Debug)]
pub enum NextPlayingStates{
    None,
    Some(PathBuf),
    JustQueued(PathBuf),
}

struct SongNames {
    immediate: RefCell<VecDeque<SharedString>>,
    back: RefCell<VecDeque<SharedString>>,
    notify: slint::ModelNotify,
}

impl Iterator for SongQueue {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        // Pop the first from the song names.
        let next = if !self.immediate.is_empty() {
            self.last_pull_was_back = false;
            self.immediate.pop_front()
        } else {
            self.last_pull_was_back = true;
            let next = self.back.pop_front();
            next
        };

        // println!("\n\nBEFORE:\n\ncurrent: {:?}\nnext: {:?}\nrest: {:?}\nret: {:?}", self.currently_playing, self.next_playing, self.back, next);

        // First pull
        if self.currently_playing.is_none(){
            self.currently_playing = next.clone();
        }
        // Second pull
        else if let NextPlayingStates::None = self.next_playing{
            self.next_playing = if let Some(n) = &next{
                NextPlayingStates::Some(n.clone())
            }else{
                NextPlayingStates::None
            };
        }
        // Last pull was immediate
        else if let NextPlayingStates::JustQueued(next_p) = &self.next_playing{
            self.currently_playing = Some(next_p.clone());
            self.next_playing = NextPlayingStates::None;
        }
        // Otherwise
        else{
            match &mut self.next_playing{
                NextPlayingStates::Some(n) => {
                    if let Some(nxt) = &next{
                        self.currently_playing = Some(std::mem::replace(n, nxt.clone()));
                    }else{
                        self.currently_playing = Some(n.clone());
                        self.next_playing = NextPlayingStates::None;
                    };
                }
                NextPlayingStates::JustQueued(n) => {
                    if let Some(nxt) = &next{
                        self.currently_playing = Some(std::mem::replace(n, nxt.clone()));
                    }else{
                        self.currently_playing = Some(n.clone());
                        self.next_playing = NextPlayingStates::None;
                    };
                }
                NextPlayingStates::None => self.currently_playing = None,
            }
            // self.currently_playing = std::mem::replace(&mut self.next_playing, next.clone());
        }


        // println!("\n\nAFTER:\n\ncurrent: {:?}\nnext: {:?}\nrest: {:?}\nret: {:?}", self.currently_playing, self.next_playing, self.back, next);
        

        // Horrible high-coupling nonsense, but it's the only
        // way I've found to modify a Slint model from another thread.
        // Not only that, but *updating* the model didn't work,
        // so I have to recreate it every time, with a mountain of clone() calls.
        if let Some(ui) = &self.names_ui {
            let (immediate, back) = self.new_model();
            let _ = ui.upgrade_in_event_loop(move |ui| {
                // I tried doing the downcast thing, but ui.get_songs_for_selected()
                // didn't want to downcast into SongNames, so it didn't really work.
                ui.set_songs_for_selected(ModelRc::new(Rc::new(SongNames {
                    immediate: immediate,
                    back: back,
                    notify: ModelNotify::default(),
                })));
                println!("Updated songs");
            });
        }
        
        next
    }
}

impl SongQueue {
    pub fn new(
        songs: Vec<PathBuf>,
        ui_weak: Option<Weak<AppWindow>>,
    ) -> (SongQueue, ModelRc<SharedString>) {
        // Construct the list of song names.
        // We use lossy conversion from OSString here;
        // a malformed song name really isn't a big deal.
        // (Well, for now anyway. But it's better than a crash.)
        let inner_names = songs
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned().into())
            .collect::<VecDeque<SharedString>>();
        let names_rc = Rc::new(SongNames {
            immediate: RefCell::new(VecDeque::new()),
            back: RefCell::new(inner_names),
            notify: ModelNotify::default(),
        });

        let names_model = ModelRc::from(names_rc.clone());

        (
            SongQueue {
                immediate: VecDeque::new(),
                back: VecDeque::from(songs),
                currently_playing: None,
                next_playing: NextPlayingStates::None,
                last_pull_was_back: false,
                names_ui: ui_weak,
            },
            names_model,
        )
    }

    fn new_model(
        &self,
    ) -> (
        RefCell<VecDeque<SharedString>>,
        RefCell<VecDeque<SharedString>>,
    ) {
        let mut inner_names = self
            .immediate
            .iter()
            .chain(self.back.iter())
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned().into())
            .collect::<VecDeque<SharedString>>();
        if let Some(current) = &self.currently_playing{
            if let NextPlayingStates::Some(next) = &self.next_playing{
                inner_names.push_front(next.file_name().unwrap_or_default().to_string_lossy().into_owned().into());
            }
            inner_names.push_front(current.file_name().unwrap_or_default().to_string_lossy().into_owned().into());
        }

        (RefCell::new(VecDeque::new()), RefCell::new(inner_names))
    }

    fn update_names(&self){
        if let Some(ui) = &self.names_ui {
            let (immediate, back) = self.new_model();
            let _ = ui.upgrade_in_event_loop(move |ui| {
                // I tried doing the downcast thing, but ui.get_songs_for_selected()
                // didn't want to downcast into SongNames, so it didn't really work.
                ui.set_songs_for_selected(ModelRc::new(Rc::new(SongNames {
                    immediate: immediate,
                    back: back,
                    notify: ModelNotify::default(),
                })));
            });
        }
    }

    pub fn queue_immediate(&mut self, item: PathBuf) {
        self.immediate.push_back(item.clone());
        if self.last_pull_was_back{
            if let NextPlayingStates::Some(next) = &self.next_playing && self.last_pull_was_back{
                self.back.push_front(next.clone());
                self.next_playing = NextPlayingStates::JustQueued(item.clone());
            }
            if let NextPlayingStates::None = &self.next_playing{
                self.next_playing = NextPlayingStates::JustQueued(item.clone());
            }
        }
        self.update_names();
    }
}

impl Model for SongNames {
    type Data = SharedString;

    fn row_count(&self) -> usize {
        self.immediate.borrow().len() + self.back.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<Self::Data> {
        if row < self.immediate.borrow().len() {
            return self.immediate.borrow().get(row).cloned();
        }
        let row_adj = row - self.immediate.borrow().len();
        if row_adj < self.back.borrow().len() {
            return self.back.borrow().get(row_adj).cloned();
        }
        None
    }

    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        &self.notify
    }
}

impl SongNames {
    // Interior mutability
    fn pop_front(&self) {
        println!("songnames called");
        if !self.immediate.borrow().is_empty() {
            self.immediate.borrow_mut().pop_front();
        } else {
            self.back.borrow_mut().pop_front();
        }
        self.notify.reset();
    }
}
