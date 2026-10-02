use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use rodio::decoder::DecoderBuilder;
use rodio::{Decoder};
use slint::{Model, ModelNotify, ModelRc, SharedString};

// Allowed because it triggers on the Arcs, which while *technically* don't
// need to be passed by reference, I don't think there's really a lot of
// benefit to changing it.
#[allow(clippy::needless_pass_by_value)]
pub fn play_file_list<T: 'static + PlayerInterface>(
    player: Arc<T>,
    song_queue: Arc<Mutex<SongQueue>>,
    // ui_queue: Arc<Mutex<SongQueueOld>>,
    song_finished: &dyn Fn(),
    command_queue: Arc<PlayerCommandQueue>,
) {
    // Put one song in the queue before the main loop.
    // That way we'll always have two songs in the queue, letting us do gapless. (Hopefully)
    // Not the best code I've written but a little break is fine in a five-line thing I'm sure.
    while let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()} {
        // song_queue.lock().unwrap().next();
        // if let Ok(_) = append_next_song(&player, filepath) {
        //     println!("Playing {}", filepath.display());
        //     break;
        // }

        if let Ok(source) = decode_song(&filepath){
            let player_copy = player.clone();
            let cmd_queue_copy = command_queue.clone();
            std::thread::spawn(move || {
                player_copy.append(source);
                player_copy.sleep_until_end();
                cmd_queue_copy.add_command(PlayerCommand::SongFinished);
            });
            println!("Playing {}", filepath.display());
            {song_queue.lock().unwrap().advance()};

            break;
        }
        {song_queue.lock().unwrap().advance()};
        // song_finished();
    }

    let mut last_result = WaitResult::Ok;

    while last_result != WaitResult::Stop {
        // If the last one was an immediate queue, we flushed the queue,
        // so we need to queue another.
        if last_result == WaitResult::QueuedImmediate{
            if let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()}{
                // let filepath_copy = filepath.clone();
                let player_copy = player.clone();
                let cmd_queue_copy = command_queue.clone();
                let source = decode_song(&filepath).unwrap();
                std::thread::spawn(move || append_and_wait(&*player_copy, source, &filepath, &cmd_queue_copy));
            }
            song_queue.lock().unwrap().advance();
            song_finished();
        }

        // Only append & wait if there's another song in the queue.
        // Otherwise, we go straight to waiting for commands.
        if let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()}{
            // let filepath_copy = filepath.clone();
            let player_copy = player.clone();
            let cmd_queue_copy = command_queue.clone();
            let source = decode_song(&filepath).unwrap();
            std::thread::spawn(move || append_and_wait(&*player_copy, source, &filepath, &cmd_queue_copy));
        }
        
        // song_finished();
        command_queue.take_commands();
        last_result = wait_for_command(&command_queue, &*player, &song_queue, song_finished);
        // {song_queue.lock().unwrap().advance()};
    }
    player.stop();
    
    // // We wait one time at the end
    // wait_for_command(&command_queue, player.clone());
}

// Returns true if we should stop playing.
fn wait_for_command<T: PlayerInterface>(command_queue: &PlayerCommandQueue, player: &T, song_queue: &Mutex<SongQueue>, song_finished: &dyn Fn()) -> WaitResult{
    use WaitResult::{Stop, QueuedImmediate, Ok};
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
                    }else{
                        song_queue.lock().unwrap().advance();
                        song_finished();
                    }
                },
                PlayerCommand::QueueImmediate =>{
                    // let player_copy = player.clone();
                    // let reading_mutex_copy = reading_mutex.clone();
                    // std::thread::spawn(move || append_next_song(&player_copy, &path, &reading_mutex_copy));
                    queued_immediate = true;
                },
                PlayerCommand::Stop => {
                    return Stop;
                }
            }
        }
    }

    if queued_immediate{
        QueuedImmediate
    }else{
        Ok
    }
}

#[derive(PartialEq)]
enum WaitResult{
    Stop,
    QueuedImmediate,
    Ok,
}

// fn append_next_song(player: &rodio::Player, source: Decoder<BufReader<File>>) {
//     // Play file
//     player.append(source);
// }

fn decode_song(filepath: &Path) -> Result<Decoder<BufReader<File>>, DecodingError>{
    // Open file.
    let file = match File::open(filepath) {
        Ok(f) => BufReader::new(f),
        Err(err) => {
            return Err(DecodingError::FileError(err));
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
            return Err(DecodingError::DecodingError(err));
        }
    };

    Ok(source)
}

#[derive(Debug)]
enum DecodingError{
    FileError(std::io::Error),
    DecodingError(rodio::decoder::DecoderError),
}

impl Error for DecodingError{}
impl Display for DecodingError{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self{
            Self::FileError(err) => write!(f, "{err}"),
            Self::DecodingError(err) => write!(f, "{err}"),
        }
    }
}

fn append_and_wait<T: PlayerInterface>(player: &T, source: Decoder<BufReader<File>>, path: &PathBuf, cmd_queue: &PlayerCommandQueue){
    player.append(source);
    // I'm not sure this works or will be stable.
    println!("Playing {}", path.display());
    player.sleep_until_end();
    // println!("finished sleeping {}", path.display());
    cmd_queue.add_command(PlayerCommand::SongFinished);
}

// /// Meant to run in a separate thread.
// /// Will push a `PlayerCommand::SongFinished` update when done.
// fn play_source<T: Source + Send + 'static>(
//     player: &rodio::Player,
//     source: T,
//     command_queue: &PlayerCommandQueue,
// ) {
//     player.append(source);
//     player.sleep_until_end();
//     command_queue.add_command(PlayerCommand::SongFinished);
// }

// fn end_song_detector(
//     player: &rodio::Player,
//     command_queue: &PlayerCommandQueue,
//     song_queue: Arc<Mutex<SongQueueOld>>,
//     keep_detecting: &bool,
//     sync_mutex: &Mutex<()>,
// ) {
//     while *keep_detecting {
//         // Attempt locking the mutex.
//         // If a song is currently being read, 
//         // the mutex will be locked by the main player,
//         // so this won't be able to send song queue updates until that's done.
//         {
//             let _lock = sync_mutex.lock();
//         }
//         player.sleep_until_end();
//         command_queue.add_command(PlayerCommand::SongFinished);
//         // if let Some(song) = song_queue.lock().unwrap().next() {
//         //     println!("Playing {}", song.display());
//         // }
//     }
// }

#[derive(Debug)]
pub enum PlayerCommand {
    Skip,
    Pause,
    VolumeUp,
    VolumeDown,
    SongFinished,
    QueueImmediate,
    Stop,
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

#[derive(Debug)]
pub enum NextPlayingStates{
    None,
    Some(PathBuf),
    JustQueued(PathBuf),
}


// The song queue for the backend.
pub struct SongQueue{
    normal_queue: VecDeque<PathBuf>,
    immediate_queue: VecDeque<PathBuf>,
}

impl SongQueue{
    pub fn new(songs: Vec<PathBuf>) -> SongQueue{
        SongQueue{
            normal_queue: songs.into(),
            immediate_queue: VecDeque::new(),
        }
    }

    fn peek(&self) -> Option<&PathBuf>{
        if self.immediate_queue.is_empty(){
            self.normal_queue.front()
        }else{
            self.immediate_queue.front()
        }
    }

    fn advance(&mut self){
        if self.immediate_queue.is_empty(){
            self.normal_queue.pop_front();
        }else{
            self.immediate_queue.pop_front();
        }
    }

    pub fn queue(&mut self, path: PathBuf){
        self.immediate_queue.push_back(path);
    }
}

// The visual representation for the song list in the UI.
pub struct SongModelUI{
    current_song: Option<SharedString>,
    immediate_queue: Rc<VecDequeModel<SharedString>>,
    playlist_queue: Rc<VecDequeModel<SharedString>>,
}

impl SongModelUI{
    // Returns in the order (handle for this, current_song, immediate_queue, playlist_queue) 
    #[must_use]
    pub fn new(songs: &[PathBuf]) -> (SongModelUI, SharedString, ModelRc<SharedString>, ModelRc<SharedString>){
        let current_song: SharedString = songs.get(0).unwrap_or(&PathBuf::from(""))
            .file_name()
            .map_or("Error: couldn't read filename".into(), |s| s.to_string_lossy())
            .into_owned()
            .into();

        let playlist_queue: Rc<VecDequeModel<SharedString>> =
            Rc::new(
                VecDequeModel::new(
                    songs.get(1..).unwrap_or(&[]).into_iter()
                    .map(|path| path
                        .file_name()
                        .map_or("Error: couldn't read filename".into(), |s| s.to_string_lossy())
                        .into_owned()
                        .into())
                    .collect()
                )
            );

        let playlist_model = ModelRc::from(playlist_queue.clone());

        let immediate_queue = Rc::new(VecDequeModel::new(VecDeque::new()));
        let immediate_model = ModelRc::from(immediate_queue.clone());

        (
            SongModelUI{
                current_song: Some(current_song.clone()),
                playlist_queue,
                immediate_queue
            },
            current_song,
            immediate_model,
            playlist_model,
        )
    }

    // pub fn song_finished(&mut self) -> Option<SharedString>{
    //     if self.immediate_queue.is_empty(){
    //         self.current_song = self.playlist_queue.pop_front();
    //     }else{
    //         self.current_song = self.immediate_queue.pop_front();
    //     }
    //     self.current_song.clone()
    // }
}

pub struct VecDequeModel<T>{
    queue: RefCell<VecDeque<T>>,
    notify: ModelNotify,
}

impl<T> VecDequeModel<T>{
    #[must_use]
    pub fn new(data: VecDeque<T>) -> VecDequeModel<T>{
        VecDequeModel{
            queue: RefCell::new(data),
            notify: ModelNotify::default()
        }
    }

    pub fn pop_front(&self) -> Option<T>{
        let data = self.queue.borrow_mut().pop_front();
        self.notify.reset();
        data
    }

    pub fn push_back(&self, data: T){
        self.queue.borrow_mut().push_back(data);
        // TODO check if this works
        // self.notify.row_added(self.queue.borrow().len(), 1);
        self.notify.reset();
    }

    pub fn is_empty(&self) -> bool{
        self.queue.borrow().len() == 0
    }
}

impl Model for VecDequeModel<SharedString>{
    type Data = SharedString;

    fn row_count(&self) -> usize {
        self.queue.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<Self::Data> {
        self.queue.borrow().get(row).cloned()
    }

    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn core::any::Any { self }
}


// Trait used so we can dependency-inject a mockup rodio::Player. 
pub trait PlayerInterface: Send + Sync{
    fn sleep_until_end(&self);
    fn append<S: rodio::Source + Send + 'static>(&self, source: S);
    fn play(&self);
    fn pause(&self);
    fn is_paused(&self) -> bool;
    fn skip_one(&self);
    fn stop(&self);
}

impl PlayerInterface for rodio::Player{
    fn sleep_until_end(&self){
        self.sleep_until_end();
    }

    fn append<S: rodio::Source + Send + 'static>(&self, source: S){
        self.append(source);
    }
    
    fn play(&self) {
        self.play();
    }
    
    fn pause(&self) {
        self.pause();
    }
    
    fn is_paused(&self) -> bool{
        self.is_paused()
    }
    
    fn skip_one(&self) {
        self.skip_one();
    }

    fn stop(&self){
        self.stop();
    }
}

struct FakePlayer{

}

impl PlayerInterface for FakePlayer{
    fn sleep_until_end(&self){
        todo!()
    }

    fn append<S>(&self, source: S) where S: rodio::Source + Send + 'static{
        todo!()
    }
    
    fn play(&self) {
        todo!()
    }
    
    fn pause(&self) {
        todo!()
    }
    
    fn is_paused(&self) -> bool {
        todo!()
    }
    
    fn skip_one(&self) {
        todo!()
    }

    fn stop(&self){
        todo!()
    }
}