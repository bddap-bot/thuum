use crate::game::{self, Command, Origin};
use crate::hold::{self, Edge, Hold, SIGNALS};
use crate::table::Table;
use crate::worker::Worker;
use std::collections::VecDeque;
use std::ffi::{c_char, c_void};
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const MESSAGING: u32 = 5;
const PRE_LOAD_GAME: u32 = 2;
const POST_LOAD_GAME: u32 = 3;
const NEW_GAME: u32 = 7;
const DATA_LOADED: u32 = 8;
const EXPIRY_US: u64 = 500_000;
const MAX_PENDING: usize = 4;
const _: () = assert!(std::mem::size_of::<VersionData>() == 0x350);
const _: () = assert!(std::mem::size_of::<Message>() == 0x18);

#[repr(C)]
pub struct VersionData {
    data_version: u32,
    plugin_version: u32,
    name: [u8; 256],
    author: [u8; 256],
    support_email: [u8; 252],
    version_independence_ex: u32,
    version_independence: u32,
    compatible_versions: [u32; 16],
    se_version_required: u32,
}

const fn padded<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0u8; N];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        out[i] = b[i];
        i += 1;
    }
    out
}

#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static SKSEPlugin_Version: VersionData = VersionData {
    data_version: 1,
    plugin_version: 1,
    name: padded("thuum"),
    author: padded(""),
    support_email: padded(""),
    version_independence_ex: 0,
    version_independence: 0,
    compatible_versions: {
        let mut v = [0; 16];
        v[0] = game::SE_1_7_104;
        v
    },
    se_version_required: 0,
};

#[repr(C)]
pub struct SkseInterface {
    skse_version: u32,
    runtime_version: u32,
    editor_version: u32,
    is_editor: u32,
    query_interface: extern "C" fn(u32) -> *mut c_void,
    get_plugin_handle: extern "C" fn() -> u32,
}

#[repr(C)]
pub struct PluginInfo {
    info_version: u32,
    name: *const c_char,
    version: u32,
}

#[repr(C)]
struct Message {
    sender: *const c_char,
    kind: u32,
    data_len: u32,
    data: *mut c_void,
}

#[repr(C)]
struct MessagingInterface {
    interface_version: u32,
    register_listener: extern "C" fn(u32, *const c_char, extern "C" fn(*mut Message)) -> bool,
}

#[repr(C)]
struct SinkVtable {
    drop: extern "C" fn(*mut Sink, u32) -> *mut Sink,
    receive: extern "C" fn(*mut Sink, *const usize, usize) -> u32,
}

#[repr(C)]
struct Sink {
    vtable: &'static SinkVtable,
}

static SINK_VTABLE: SinkVtable = SinkVtable {
    drop: drop_sink,
    receive: receive_input,
};
static mut SINK: Sink = Sink { vtable: &SINK_VTABLE };
static QUEUE: Mutex<VecDeque<Command>> = Mutex::new(VecDeque::new());
static LOG: OnceLock<Sender<String>> = OnceLock::new();
const LOG_ROTATE_BYTES: u64 = 4 << 20;
static DISABLED: AtomicBool = AtomicBool::new(false);
static OWNED: AtomicBool = AtomicBool::new(false);
static LISTENING: AtomicBool = AtomicBool::new(false);
static WORKER: OnceLock<Worker> = OnceLock::new();

pub fn log(line: &str) {
    if let Some(sender) = LOG.get() {
        let _ = sender.send(format!("{:.3} {line}", hold::clock_us() as f64 / 1000.0));
    }
}

fn open_log(game: &str) {
    let Some(mut file) = log_path(game).and_then(|p| {
        if std::fs::metadata(&p).is_ok_and(|m| m.len() > LOG_ROTATE_BYTES) {
            let _ = std::fs::rename(&p, p.with_extension("1.log"));
        }
        std::fs::OpenOptions::new().create(true).append(true).open(p).ok()
    }) else {
        return;
    };
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let _ = writeln!(file, "session start unix={unix}");
    let (sender, receiver) = channel::<String>();
    if LOG.set(sender).is_ok() {
        std::thread::spawn(move || {
            for line in receiver {
                let _ = writeln!(file, "{line}");
                let _ = file.flush();
            }
        });
    }
}

fn guarded<R>(fallback: R, body: impl FnOnce() -> R) -> R {
    if DISABLED.load(Ordering::Acquire) {
        return fallback;
    }
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| {
        disable("a panic");
        fallback
    })
}

fn disable(reason: &str) {
    DISABLED.store(true, Ordering::Release);
    if let Some(worker) = WORKER.get() {
        worker.stop.store(true, Ordering::Release);
    }
    log(&format!("disabled after {reason}"));
}

fn log_path(game: &str) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok()?;
        let documents = path.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as _));
        let directory = std::path::PathBuf::from(documents?).join("My Games").join(game).join("SKSE");
        std::fs::create_dir_all(&directory).ok()?;
        Some(directory.join("thuum.log"))
    }
}

fn thread_id() -> u32 {
    unsafe { windows::Win32::System::Threading::GetCurrentThreadId() }
}

pub fn submit(command: Command) {
    let line = format!("{command:?}");
    let accepted = {
        let mut queue = QUEUE.lock().unwrap();
        let room = queue.len() < MAX_PENDING;
        if room {
            queue.push_back(command);
        }
        room
    };
    log(&format!("{} {line}", if accepted { "queued" } else { "dropped" }));
}

extern "C" fn drop_sink(sink: *mut Sink, _flags: u32) -> *mut Sink {
    sink
}

extern "C" fn receive_input(_sink: *mut Sink, _event: *const usize, _source: usize) -> u32 {
    guarded(0, drain)
}

fn drain() -> u32 {
    if game::PENDING.load(Ordering::Acquire) {
        if let Some(outcome) = game::settle() {
            log(&format!("settle thread={:#x} outcome={outcome}", thread_id()));
        }
    }
    let commands: Vec<Command> = QUEUE.lock().unwrap().drain(..).collect();
    let now = hold::clock_us();
    for command in commands {
        let age = now.saturating_sub(command.issued_us);
        let outcome = if age > EXPIRY_US {
            format!("rejected expired age_ms={:.0}", age as f64 / 1000.0)
        } else {
            game::execute(&command)
        };
        log(&format!("ran {command:?} thread={:#x} outcome={outcome}", thread_id()));
    }
    0
}

extern "C" fn shout_button(handler: usize, event: usize, data: usize) {
    if !guarded(false, || take_button(event)) {
        game::vanilla_shout_button(handler, event, data);
    }
}

fn take_button(event: usize) -> bool {
    let (value, held) = game::button(event);
    match hold::edge(value, held) {
        Edge::Press => {
            if LISTENING.swap(false, Ordering::AcqRel) {
                SIGNALS.release(true);
            }
            let owned = game::voice_power_selected();
            OWNED.store(owned, Ordering::Release);
            if owned {
                SIGNALS.press();
                LISTENING.store(true, Ordering::Release);
                log("hold pressed");
            }
            owned
        }
        Edge::Held => OWNED.load(Ordering::Acquire),
        Edge::Release => {
            if LISTENING.swap(false, Ordering::AcqRel) {
                SIGNALS.release(false);
                log(&format!("hold released held_s={held:.2}"));
            }
            OWNED.swap(false, Ordering::AcqRel)
        }
    }
}

const CHUNK_MS: u32 = (hold::CHUNK * 1000 / 16000) as u32;

fn on_hold(table: &Table, hold: Hold) {
    let [(best, score), (second, second_score)] = hold.ranked;
    log(&format!(
        "hold decided phrase={} score={score:.2} runner_up={} runner_up_score={second_score:.2} margin={:.2} hold_ms={} peak_rms={:.0} classify_ms={:.0} released={:.3}",
        table.phrases[best].id,
        table.phrases[second].id,
        score - second_score,
        hold.chunks * CHUNK_MS,
        hold.peak_rms,
        hold.classify_ms,
        hold.released_us as f64 / 1000.0
    ));
    let phrase = &table.phrases[best];
    submit(Command {
        shout: phrase.shout,
        tier: phrase.tier,
        origin: Origin::Hold { chunks: hold.chunks },
        issued_us: hold.released_us,
    });
}

#[cfg(feature = "test-hook")]
fn test_hook(directory: std::path::PathBuf) {
    let table = Table::embedded();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(20));
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            let mut files: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "cmd"))
                .collect();
            files.sort();
            for path in files {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let _ = std::fs::remove_file(&path);
                let mut parts = text.split_whitespace();
                let (Some(id), Some(tier)) = (parts.next(), parts.next().and_then(|t| t.parse::<u8>().ok())) else {
                    log(&format!("test hook: bad command {text:?}"));
                    continue;
                };
                match table.shouts.iter().position(|s| s.id == id) {
                    Some(shout) if (1..=3).contains(&tier) => {
                        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                        submit(Command {
                            shout,
                            tier,
                            origin: Origin::Test(name),
                            issued_us: hold::clock_us(),
                        })
                    }
                    _ => log(&format!("test hook: unknown command {text:?}")),
                }
            }
        }
    });
}

extern "C" fn on_message(message: *mut Message) {
    guarded((), || handle_message(unsafe { &*message }))
}

fn handle_message(message: &Message) {
    if matches!(message.kind, PRE_LOAD_GAME | POST_LOAD_GAME | NEW_GAME) {
        let dropped = QUEUE.lock().unwrap().drain(..).count();
        if LISTENING.swap(false, Ordering::AcqRel) {
            SIGNALS.release(true);
        }
        game::reset();
        log(&format!("game state change {} dropped={dropped}", message.kind));
        return;
    }
    if message.kind != DATA_LOADED {
        return;
    }
    log(&format!("data loaded on thread {:#x}", thread_id()));
    game::resolve_forms();
    if !game::resolve_voice_power() {
        disable("the voice power is missing");
        return;
    }
    if !game::add_input_sink(&raw mut SINK as usize) {
        disable("input sink registration failed");
        return;
    }
    log("input sink registered");
    if !game::hook_shout_button(shout_button) {
        disable("shout handler hook failed");
        return;
    }
    log("shout handler hooked");
    game::label_spoken_words();
    #[cfg(feature = "test-hook")]
    if let Ok(directory) = std::env::var("THUUM_TEST_DIR") {
        log(&format!("test hook watching {directory}"));
        test_hook(directory.into());
    }
    let table = Table::embedded();
    let worker = Worker::spawn(move |h| on_hold(&table, h));
    let counters = worker.counters.clone();
    let _ = WORKER.set(worker);
    std::thread::spawn(move || {
        let mut last = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let c = &counters.capture;
            let state = format!(
                "capture opens={} stalls={} last_error={:#010x} recognizer_errors={}",
                c.opens.load(Ordering::Relaxed),
                c.stalls.load(Ordering::Relaxed),
                c.last_error.load(Ordering::Relaxed),
                counters.recognizer_errors.load(Ordering::Relaxed)
            );
            if let Some(failure) = counters.failure.lock().unwrap().take() {
                log(&format!("recognizer error: {failure}"));
            }
            if state != last {
                log(&format!(
                    "{state} errors={} samples={}",
                    c.errors.load(Ordering::Relaxed),
                    c.samples.load(Ordering::Relaxed)
                ));
                last = state;
            }
        }
    });
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn SKSEPlugin_Query(_skse: *const SkseInterface, info: *mut PluginInfo) -> bool {
    guarded(false, || {
        unsafe {
            *info = PluginInfo {
                info_version: 1,
                name: c"thuum".as_ptr(),
                version: 1,
            }
        };
        game::this_runtime().is_some()
    })
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn SKSEPlugin_Load(skse: *const SkseInterface) -> bool {
    guarded(false, || load(unsafe { &*skse }))
}

fn load(skse: &SkseInterface) -> bool {
    let runtime = game::this_runtime();
    open_log(runtime.unwrap_or(&game::SE).game);
    std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    log(&format!(
        "load skse={:#x} runtime={:#x}",
        skse.skse_version, skse.runtime_version
    ));
    let Some(runtime) = runtime else {
        log(&format!(
            "unsupported executable image_timestamp={:#x}",
            game::image_timestamp()
        ));
        return false;
    };
    game::select(runtime);
    log(runtime.game);
    let messaging = (skse.query_interface)(MESSAGING) as *const MessagingInterface;
    if messaging.is_null() {
        log("missing SKSE messaging interface");
        return false;
    }
    let handle = (skse.get_plugin_handle)();
    let ok = unsafe { ((*messaging).register_listener)(handle, c"SKSE".as_ptr(), on_message) };
    log(&format!("listener registered={ok}"));
    ok
}
