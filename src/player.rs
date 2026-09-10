use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};

use rodio::{Decoder, Source};
use slint::{Model, ModelNotify, ModelRc, SharedString, Weak};

use crate::ui::AppWindow;

// Allowed because it triggers on the Arcs, which while *technically* don't
// need to be passed by reference, I don't think there's really a lot of
// benefit to changing it.
#[allow(clippy::needless_pass_by_value)]
pub fn play_file_list<T: Iterator<Item = PathBuf>>(
    player: Arc<rodio::Player>,
    mut filepaths: T,
    command_queue: Arc<PlayerCommandQueue>,
) {
    while let Some(filepath) = &filepaths.next() {
        // Open file.
        // Either open file as a BufReader, or skip to the next one if it fails.
        let file = match File::open(filepath) {
            Ok(f) => BufReader::new(f),
            Err(err) => {
                eprintln!("Error reading file {}: {err}", filepath.display());
                continue;
            }
        };

        // Try to get an audio source from that file
        let source = match Decoder::try_from(file) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("Error reading file {}: {err}", filepath.display());
                continue;
            }
        };

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
                    PlayerCommand::SongFinished => next_song = true,
                }
            }
        }
    }
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

pub enum PlayerCommand {
    Skip,
    Pause,
    VolumeUp,
    VolumeDown,
    SongFinished,
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
    // names_model: ModelRc<SharedString>,
    // names_rc: Rc<SongNames>,
    names_ui: Option<Weak<AppWindow>>,
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
            });
        }

        if !self.immediate.is_empty() {
            return self.immediate.pop_front();
        } else {
            return self.back.pop_front();
        }
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
        let inner_names = self
            .immediate
            .iter()
            .chain(self.back.iter())
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned().into())
            .collect::<VecDeque<SharedString>>();

        (RefCell::new(VecDeque::new()), RefCell::new(inner_names))
    }

    pub fn queue_immediate(&mut self, item: PathBuf) {
        // TODO: Update SongNames
        self.immediate.push_back(item);
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
