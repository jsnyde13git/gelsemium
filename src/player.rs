use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Barrier, Condvar, Mutex};
use rodio::decoder::DecoderBuilder;
use rodio::{Decoder, Source};
use slint::{Model, ModelNotify, ModelRc, SharedString};

// Allowed because it triggers on the Arcs, which while *technically* don't
// need to be passed by value, I don't think there's really a lot of
// benefit to changing it.
#[allow(clippy::needless_pass_by_value)]
pub fn play_file_list<T: 'static + PlayerInterface, S: Source + Send + 'static>(
    player: Arc<T>,
    song_queue: Arc<Mutex<SongQueue>>,
    // ui_queue: Arc<Mutex<SongQueueOld>>,
    song_finished: &dyn Fn(),
    decode_song: &dyn Fn(&Path) -> Result<S, DecodingError>,
    command_queue: Arc<PlayerCommandQueue>,
) {
    // Put one song in the queue before the main loop.
    // That way we'll always have two songs in the queue, letting us do gapless. (Hopefully)
    // Not the best code I've written but a little break is fine in a short thing I'm sure.
    // TODO verify that the channel method works
    let (sender, receiver) = sync_channel(1);
    let player_copy = player.clone();
    let queue_copy = command_queue.clone();
    std::thread::spawn(move || append_thread(&*player_copy, &queue_copy, &receiver));
    #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
    while let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()} {
        // song_queue.lock().unwrap().next();
        // if let Ok(_) = append_next_song(&player, filepath) {
        //     println!("Playing {}", filepath.display());
        //     break;
        // }

        match decode_song(&filepath){
            Ok(source) => {
                let _ = sender.send((source, filepath));
                // let player_copy = player.clone();
                // let cmd_queue_copy = command_queue.clone();
                // println!("Playing {}", filepath.display());
                // std::thread::spawn(move || {
                //     player_copy.append(source, filepath);
                //     player_copy.sleep_until_end();
                //     cmd_queue_copy.add_command(PlayerCommand::SongFinished);
                // });
                #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
                song_queue.lock().unwrap().advance();

                break;
            }
            Err(err) => {
                eprintln!("Error decoding {}: {err}", filepath.display());
                song_queue.lock().unwrap().advance();
            }
        }
        #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
        song_queue.lock().unwrap().advance();
        // song_finished();
    }

    let mut last_result = WaitResult::Ok;

    while last_result != WaitResult::Stop {
        // If the last one was an immediate queue, we flushed the queue,
        // so we need to queue another.
        if last_result == WaitResult::QueuedImmediate{
            // if let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()}{
            //     // let filepath_copy = filepath.clone();
            //     let player_copy = player.clone();
            //     let cmd_queue_copy = command_queue.clone();
            //     let source = decode_song(&filepath).unwrap();
            //     std::thread::spawn(move || append_and_wait(&*player_copy, source, &filepath, &cmd_queue_copy));
            // }
            #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
            while let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()}{
                match decode_song(&filepath){
                    Ok(source) => {   
                        // let player_copy = player.clone();
                        // let cmd_queue_copy = command_queue.clone();
                        // std::thread::spawn(move || append_and_wait(&*player_copy, source, &filepath, &cmd_queue_copy));
                        println!("queue sending {}", filepath.display());
                        let _ = sender.send((source, filepath));
                        break;
                    }
                    Err(err) => {
                        eprintln!("Error decoding {}: {err}", filepath.display());
                        #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
                        song_queue.lock().unwrap().advance();
                    }
                } 
            }

            #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
            song_queue.lock().unwrap().advance();
            song_finished();
        }

        // Only append & wait if there's another song in the queue.
        // Otherwise, we go straight to waiting for commands.
        // if let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()}{
        //     // let filepath_copy = filepath.clone();
        //     let player_copy = player.clone();
        //     let cmd_queue_copy = command_queue.clone();
        //     let source = decode_song(&filepath).unwrap();
        //     std::thread::spawn(move || append_and_wait(&*player_copy, source, &filepath, &cmd_queue_copy));
        // }

        #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
        while let Some(filepath) = {song_queue.lock().unwrap().peek().cloned()}{
            match decode_song(&filepath){
                Ok(source) => {   
                    // let player_copy = player.clone();
                    // let cmd_queue_copy = command_queue.clone();
                    // std::thread::spawn(move || append_and_wait(&*player_copy, source, &filepath, &cmd_queue_copy));
                    println!("normal sending {}", filepath.display());
                    let _ = sender.send((source, filepath));
                    break;
                }
                Err(err) => {
                    eprintln!("Error decoding {}: {err}", filepath.display());
                    #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
                    song_queue.lock().unwrap().advance();
                }
            } 
        }
        
        // song_finished();
        // TODO verify this is fine
        // command_queue.take_commands();
        last_result = wait_for_command(&command_queue, &*player, &song_queue, song_finished);
        // {song_queue.lock().unwrap().advance()};
    }
    player.stop();
    // Ensure the player is in the playing state before exiting.
    // If this isn't done, then in GUI mode it'll still be paused
    // when we enter a new playlist, but the GUI won't recognize this,
    // so there'll be a UI/backend desync. 
    if player.is_paused(){
        player.play();
    }
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
                PlayerCommand::VolumeUp |
                PlayerCommand::VolumeDown => {eprintln!("Currently unsupported")}
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

#[allow(clippy::missing_errors_doc)]
pub fn decode_song(filepath: &Path) -> Result<Decoder<BufReader<File>>, DecodingError>{
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
            // eprintln!("Error reading file {}: {err}", filepath.display());
            return Err(DecodingError::DecodingError(err));
        }
    };

    Ok(source)
}

#[derive(Debug)]
pub enum DecodingError{
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

// fn append_and_wait<T: PlayerInterface, S: Source + Send + 'static>(player: &T, source: S, path: &Path, cmd_queue: &PlayerCommandQueue){
//     player.append(source, path.to_owned());
//     // I'm not sure this works or will be stable.
//     println!("Playing {}", path.display());
//     player.sleep_until_end();
//     // println!("finished sleeping {}", path.display());
//     cmd_queue.add_command(PlayerCommand::SongFinished);
// }

fn append_thread<T: PlayerInterface, S: Source + Send + 'static>(player: &T, cmd_queue: &PlayerCommandQueue, channel: &Receiver<(S, PathBuf)>){
    while let Ok((source, path)) = channel.recv(){
        player.append(source, path.clone());
        println!("Playing {}", path.display());
        player.sleep_until_end();
        cmd_queue.add_command(PlayerCommand::SongFinished);
    }
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
    #[must_use]
    pub fn new() -> PlayerCommandQueue {
        PlayerCommandQueue {
            command_list: Mutex::new(Vec::new()),
            needs_update: Condvar::new(),
        }
    }

    /// Adds one command.
    /// Modifies the mutex.
    pub fn add_command(&self, cmd: PlayerCommand) {
        #[expect(clippy::missing_panics_doc, reason = "infallible if and only if no other thread panics")]
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

impl Default for PlayerCommandQueue{
    fn default() -> Self{
        Self::new()
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
    #[must_use]
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

// Returns in the order (handle for this, current_song, immediate_queue, playlist_queue) 
#[must_use]
pub fn create_song_models(songs: &[PathBuf]) -> (SharedString, ModelRc<SharedString>, ModelRc<SharedString>){
    let current_song: SharedString = songs.first().unwrap_or(&PathBuf::from(""))
        .file_name()
        .map_or("Error: couldn't read filename".into(), |s| s.to_string_lossy())
        .into_owned()
        .into();

    let playlist_queue: Rc<VecDequeModel<SharedString>> =
        Rc::new(
            VecDequeModel::new(
                songs.get(1..).unwrap_or(&[]).iter()
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
        current_song,
        immediate_model,
        playlist_model,
    )
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
    fn append<S: rodio::Source + Send + 'static>(&self, source: S, path: PathBuf);
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

    fn append<S: rodio::Source + Send + 'static>(&self, source: S, _: PathBuf){
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
    received_actions: Mutex<Vec<ReceivedInstruction>>,
    paused: Mutex<bool>,
    // True if it can be activated the moment test unlocks.
    // False if it still needs to wait for test.
    // sleep_lock: Mutex<bool>,
    sleep_barrier: Arc<Barrier>,
}
 
/// Received instructions for `FakePlayer`.
enum ReceivedInstruction
{
    Append{path: PathBuf},
    Play,
    Pause,
    Skip,
    Stop,
}

impl PartialEq for ReceivedInstruction{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Append { path: l_path }, Self::Append { path: r_path }) => l_path == r_path,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl std::fmt::Debug for ReceivedInstruction{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Append { path } => f.debug_struct("Append").field("path", path).finish(),
            Self::Play => write!(f, "Play"),
            Self::Pause => write!(f, "Pause"),
            Self::Skip => write!(f, "Skip"),
            Self::Stop => write!(f, "Stop"),
        }
    }
}

impl FakePlayer{
    // Used within tests.
    #[allow(unused)]
    fn new(barrier: Arc<Barrier>) -> Self{
        Self { 
            received_actions: Mutex::new(Vec::new()), 
            paused: Mutex::new(false),
            sleep_barrier: barrier,
        }
    }

    // fn new_with_dur(delay: Duration) -> Self{
    //     Self { 
    //         received_actions: Mutex::new(Vec::new()), 
    //         paused: Mutex::new(false),
    //         delay, 
    //         sleep_lock: Mutex::new(false), 
    //     }
    // }

    // Used *extensively* within the PlayerInterface impl.
    #[allow(unused)]
    fn lock_actions(&self) -> std::sync::MutexGuard<'_, Vec<ReceivedInstruction>>{
        self.received_actions.lock().unwrap()
    }
 
}

impl PlayerInterface for FakePlayer{
    fn sleep_until_end(&self){
        // Wait for the lock to both be open *and* True.
        // let mut done = false;
        // while !done{
        //     let mut lock = self.sleep_lock.lock().unwrap();
        //     if *lock{
        //         done = true;
        //         *lock = false;
        //     }
        // }
        self.sleep_barrier.wait();
        self.sleep_barrier.wait();
        // let _a = self.sleep_lock.lock();
        // std::thread::sleep(self.delay)
    }

    fn append<S>(&self, _: S, path: PathBuf) where S: rodio::Source + Send + 'static{
        self.lock_actions().push(
            ReceivedInstruction::Append { 
                path 
        });
    }
    
    fn play(&self) {
        self.lock_actions().push(ReceivedInstruction::Play);
        *self.paused.lock().unwrap() = false;
    }
    
    fn pause(&self) {
        self.lock_actions().push(ReceivedInstruction::Pause);
        *self.paused.lock().unwrap() = true;
    }
    
    fn is_paused(&self) -> bool {
        *self.paused.lock().unwrap()
    }
    
    fn skip_one(&self) {
        self.lock_actions().push(ReceivedInstruction::Skip);
    }

    fn stop(&self){
        self.lock_actions().push(ReceivedInstruction::Stop);
    }
}

#[cfg(test)]
mod test;
