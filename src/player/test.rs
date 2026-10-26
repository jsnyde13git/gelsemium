use std::{path::{Path, PathBuf}, sync::{Arc, Barrier, Mutex}};

use rodio::{Source, source::SineWave};

use crate::player::{DecodingError, FakePlayer, PlayerCommand, PlayerCommandQueue, ReceivedInstruction, SongQueue, play_file_list};

fn test_decode(_: &Path) -> Result<SineWave, DecodingError>{
    Ok(SineWave::new(440.0))
}

fn test_source() -> Mutex<Box<dyn Source + Send + 'static>>{
    Mutex::new(Box::new(SineWave::new(440.0)))
}

#[test]
fn test1(){
    let barrier = Arc::new(Barrier::new(2));
    let player = Arc::new(FakePlayer::new(barrier.clone()));
    let song_queue = Arc::new(Mutex::new(SongQueue::new(vec![PathBuf::from("")])));
    let command_queue = Arc::new(PlayerCommandQueue::new());
    let song_finished = ||{};
    let decode = test_decode;

    let handle = {
        let player_copy = player.clone();
        let song_queue_copy = song_queue.clone();
        let command_queue_copy = command_queue.clone();
        std::thread::spawn(move || play_file_list(player_copy, song_queue_copy, &song_finished, &decode, command_queue_copy))
    };

    command_queue.add_command(PlayerCommand::Stop);
    handle.join().unwrap();
}

// TODO: Flaky test.
// There's a small race condition between the first two songs played.
// So sometimes this can do [b, a] instead of just [a] on the first verification.
#[test]
fn play_2(){
    let barrier = Arc::new(Barrier::new(2));
    let player = Arc::new(FakePlayer::new(barrier.clone()));
    let song_queue = Arc::new(Mutex::new(SongQueue::new(vec![PathBuf::from("a"), PathBuf::from("b")])));
    let command_queue = Arc::new(PlayerCommandQueue::new());
    let song_ct = Arc::new(Mutex::new(0));
    let song_ct_copy = song_ct.clone();
    let song_finished = move ||{*song_ct_copy.lock().unwrap() += 1};
    let decode = test_decode;

    let handle = {
        let player_copy = player.clone();
        let song_queue_copy = song_queue.clone();
        let command_queue_copy = command_queue.clone();
        std::thread::spawn(move || play_file_list(player_copy, song_queue_copy, &song_finished, &decode, command_queue_copy))
    };

    barrier.wait();
    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")}]);
    barrier.wait();

    barrier.wait();
    barrier.wait();

    command_queue.add_command(PlayerCommand::Stop);
    handle.join().unwrap();

    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},
        ReceivedInstruction::Stop]);
}

#[test]
fn skip_2(){
    let barrier = Arc::new(Barrier::new(2));
    let player = Arc::new(FakePlayer::new(barrier.clone()));
    let song_queue = Arc::new(Mutex::new(SongQueue::new(vec![PathBuf::from("a"), PathBuf::from("b")])));
    let command_queue = Arc::new(PlayerCommandQueue::new());
    let song_ct = Arc::new(Mutex::new(0));
    let song_ct_copy = song_ct.clone();
    let song_finished = move ||{*song_ct_copy.lock().unwrap() += 1};
    let decode = test_decode;

    let handle = {
        let player_copy = player.clone();
        let song_queue_copy = song_queue.clone();
        let command_queue_copy = command_queue.clone();
        std::thread::spawn(move || play_file_list(player_copy, song_queue_copy, &song_finished, &decode, command_queue_copy))
    };

    barrier.wait();
    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")}]);
    command_queue.add_command(PlayerCommand::Skip);
    barrier.wait();

    barrier.wait();
    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Skip,
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")}]);
    barrier.wait();

    command_queue.add_command(PlayerCommand::Stop);
    handle.join().unwrap();

    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Skip,
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},
        ReceivedInstruction::Stop]);
}

// TODO: Flaky test (see inner TODO)
#[test]
fn queue_1(){
    let barrier = Arc::new(Barrier::new(2));
    let player = Arc::new(FakePlayer::new(barrier.clone()));
    let song_queue = Arc::new(Mutex::new(SongQueue::new(vec![PathBuf::from("a"), PathBuf::from("b")])));
    let command_queue = Arc::new(PlayerCommandQueue::new());
    let song_ct = Arc::new(Mutex::new(0));
    let song_ct_copy = song_ct.clone();
    let song_finished = move ||{*song_ct_copy.lock().unwrap() += 1};
    let decode = test_decode;

    let handle = {
        let player_copy = player.clone();
        let song_queue_copy = song_queue.clone();
        let command_queue_copy = command_queue.clone();
        std::thread::spawn(move || play_file_list(player_copy, song_queue_copy, &song_finished, &decode, command_queue_copy))
    };


    // A playing
    barrier.wait();
    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")}]);
    song_queue.lock().unwrap().queue(PathBuf::from("c"));
    command_queue.add_command(PlayerCommand::QueueImmediate);
    println!("{:?}", song_queue.lock().unwrap().immediate_queue);
    println!("{:?}", song_queue.lock().unwrap().normal_queue);
    barrier.wait();

    // A finished
    barrier.wait();
    // Skip *may or may not* have been received by the time this test runs,
    // since the skip command is unsynchronized. 
    // TODO: Fix this error: [Append { path: "a" }, Skip, Append { path: "b" }]
    // I think synchronizing the skip command sent due to queue by sending it along the channel should do it? 
    let x = player.received_actions.lock().unwrap();
    println!("{:?}", *x);
    assert!(*x ==
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},
        ReceivedInstruction::Skip]
        ||
        *x ==
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},]
    );
    drop(x);
    println!("{:?}", song_queue.lock().unwrap().immediate_queue);
    println!("{:?}", song_queue.lock().unwrap().normal_queue);
    barrier.wait();

    // B "finished"; C playing
    barrier.wait();
    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},
        ReceivedInstruction::Skip,
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("c")}]);
    println!("{:?}", song_queue.lock().unwrap().immediate_queue);
    println!("{:?}", song_queue.lock().unwrap().normal_queue);
    barrier.wait();

    // C finished
    barrier.wait();
    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},
        ReceivedInstruction::Skip,
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("c")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")}]);
    barrier.wait();

    command_queue.add_command(PlayerCommand::Stop);
    handle.join().unwrap();

    // Player stopped
    assert_eq!(*player.received_actions.lock().unwrap(),
        vec![ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("a")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},
        ReceivedInstruction::Skip,
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("c")},
        ReceivedInstruction::Append{source: test_source(), path: PathBuf::from("b")},
        ReceivedInstruction::Stop]);
}