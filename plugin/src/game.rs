use crate::esp;
use crate::hold::clock_us;
use crate::skse::log;
use crate::table::{self, Table};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const FORM_TYPE: usize = 0x1A;
const FORM_FLAGS: usize = 0x10;
const FORM_ID: usize = 0x14;
const SHOUT_TYPE: u8 = 0x77;
const WORD_TYPE: u8 = 0x76;
const WORD_TRANSLATION: usize = 0x30;
const SHOUT_WORDS: usize = 0x60;
const WORD_STRIDE: usize = 0x18;
const WORD_UNLOCKED: u32 = 0x10000;
const PAUSING_MENUS: usize = 0x160;
const KINECT_SHOUT_CONTROLS: u32 = 0x840;
const SPEECH_SHOUT: usize = 0x30;
const SPEECH_PREVIOUS: usize = 0x38;
const PARENT_CELL: usize = 0x60;
const LOADED_STATE: usize = 0x68;
const PROCESS_BUTTON_SLOT: usize = 6;
const INLINE_FILES: usize = 255;
const FILE_NAME: usize = 0x58;
const FILE_INDEX: usize = 0x478;
const FILE_LIGHT_INDEX: usize = 0x47A;

pub struct Runtime {
    pub game: &'static str,
    image_timestamp: u32,
    player: usize,
    lookup_form_by_id: usize,
    make_string: usize,
    equip_manager: usize,
    equip_shout: usize,
    equip_spell: usize,
    start_shout: usize,
    voice_state: usize,
    voice_recovery: usize,
    input_dispatcher: usize,
    add_event_sink: usize,
    speech: usize,
    equipped_shout: usize,
    ui: usize,
    control_map: usize,
    enabled_controls: usize,
    menu_controls: usize,
    beast_form: usize,
    shout_handler_vtable: usize,
    shout_process_button: usize,
    button_value: usize,
    button_held_secs: usize,
    data_handler: usize,
    files: Files,
}

enum Files {
    Collection(usize),
    Inline { count: usize, entries: usize },
}

pub const SE_1_7_104: u32 = 0x0107_0680;

pub static SE: Runtime = Runtime {
    game: "Skyrim Special Edition",
    image_timestamp: 0x6A8C_7046,
    player: 0x0323_0778,
    lookup_form_by_id: 0x001E_5860,
    make_string: 0x00EB_0F90,
    equip_manager: 0x031A_42F8,
    equip_shout: 0x006D_C5D0,
    equip_spell: 0x006D_C4F0,
    start_shout: 0x006D_8DF0,
    voice_state: 0x006D_9250,
    voice_recovery: 0x006D_92B0,
    input_dispatcher: 0x0320_4B30,
    add_event_sink: 0x005E_B480,
    speech: 0x0321_13E8,
    equipped_shout: 0x1E8,
    ui: 0x0219_E5C0,
    control_map: 0x031A_5690,
    enabled_controls: 0x120,
    menu_controls: 0x031D_FD78,
    beast_form: 0x81,
    shout_handler_vtable: 0x0193_5528,
    shout_process_button: 0x007B_3960,
    button_value: 0x28,
    button_held_secs: 0x2C,
    data_handler: 0x0219_DED8,
    files: Files::Collection(0xD70),
};

pub static VR: Runtime = Runtime {
    game: "Skyrim VR",
    image_timestamp: 0x5AEA_DAA0,
    player: 0x02FE_B9F0,
    lookup_form_by_id: 0x001A_3F60,
    make_string: 0x00C6_DB20,
    equip_manager: 0x02F8_96D8,
    equip_shout: 0x0064_0CB0,
    equip_spell: 0x0064_0BD0,
    start_shout: 0x0063_CFE0,
    voice_state: 0x0063_D460,
    voice_recovery: 0x0063_D4C0,
    input_dispatcher: 0x02FE_A520,
    add_event_sink: 0x0057_1C00,
    speech: 0x02FE_A708,
    equipped_shout: 0x1E0,
    ui: 0x01F8_3200,
    control_map: 0x02F8_AAA0,
    enabled_controls: 0x138,
    menu_controls: 0x02FC_52E8,
    beast_form: 0x89,
    shout_handler_vtable: 0x016F_2488,
    shout_process_button: 0x0073_1900,
    button_value: 0x30,
    button_held_secs: 0x34,
    data_handler: 0x01F8_2AD8,
    files: Files::Inline {
        count: 0xD70,
        entries: 0xD78,
    },
};

static RUNTIME: OnceLock<&'static Runtime> = OnceLock::new();

fn rt() -> &'static Runtime {
    RUNTIME.get().expect("runtime selected at load")
}

pub fn this_runtime() -> Option<&'static Runtime> {
    let timestamp = image_timestamp();
    [&SE, &VR].into_iter().find(|r| r.image_timestamp == timestamp)
}

pub fn select(runtime: &'static Runtime) {
    let _ = RUNTIME.set(runtime);
}

#[derive(Debug, Clone)]
pub enum Origin {
    Hold {
        chunks: u32,
    },
    #[cfg(feature = "test-hook")]
    Test(String),
}

#[derive(Debug, Clone)]
pub struct Command {
    pub shout: usize,
    pub tier: u8,
    pub origin: Origin,
    pub issued_us: u64,
}

pub type ProcessButton = extern "C" fn(usize, usize, usize);

static SHOUTS: OnceLock<Vec<usize>> = OnceLock::new();
static VANILLA_BUTTON: OnceLock<ProcessButton> = OnceLock::new();
pub static PENDING: AtomicBool = AtomicBool::new(false);
static PREVIOUS: AtomicUsize = AtomicUsize::new(0);
static VOICE_POWER: AtomicUsize = AtomicUsize::new(0);
static VOICE_SLOT: AtomicUsize = AtomicUsize::new(0);

fn base() -> usize {
    unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap().0 as usize }
}

unsafe fn read<T: Copy>(address: usize) -> T {
    unsafe { *(address as *const T) }
}

unsafe fn write<T>(address: usize, value: T) {
    unsafe { *(address as *mut T) = value }
}

unsafe fn function<F: Copy>(rva: usize) -> F {
    unsafe { std::mem::transmute_copy(&(base() + rva)) }
}

pub fn image_timestamp() -> u32 {
    let base = base();
    unsafe { read::<u32>(base + read::<u32>(base + 0x3C) as usize + 8) }
}

pub fn add_input_sink(sink: usize) -> bool {
    let dispatcher = unsafe { read::<usize>(base() + rt().input_dispatcher) };
    if dispatcher == 0 {
        return false;
    }
    let add: extern "C" fn(usize, usize) = unsafe { function(rt().add_event_sink) };
    add(dispatcher, sink);
    true
}

pub fn hook_shout_button(replacement: ProcessButton) -> bool {
    use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect};
    let slot = base() + rt().shout_handler_vtable + PROCESS_BUTTON_SLOT * 8;
    let vanilla = unsafe { read::<usize>(slot) };
    if vanilla != base() + rt().shout_process_button {
        log(&format!(
            "shout handler slot holds {:#x}, not the vanilla handler",
            vanilla.wrapping_sub(base())
        ));
        return false;
    }
    let _ = VANILLA_BUTTON.set(unsafe { std::mem::transmute::<usize, ProcessButton>(vanilla) });
    let mut old = PAGE_PROTECTION_FLAGS(0);
    unsafe {
        if let Err(e) = VirtualProtect(slot as _, 8, PAGE_READWRITE, &mut old) {
            log(&format!("shout handler slot not writable: {e}"));
            return false;
        }
        write::<usize>(slot, replacement as usize);
        let _ = VirtualProtect(slot as _, 8, old, &mut old);
    }
    true
}

pub fn vanilla_shout_button(handler: usize, event: usize, data: usize) {
    if let Some(vanilla) = VANILLA_BUTTON.get() {
        vanilla(handler, event, data);
    }
}

pub fn button(event: usize) -> (f32, f32) {
    unsafe { (read::<f32>(event + rt().button_value), read::<f32>(event + rt().button_held_secs)) }
}

pub fn resolve_forms() {
    let lookup: extern "C" fn(u32) -> usize = unsafe { function(rt().lookup_form_by_id) };
    let forms = Table::embedded()
        .shouts
        .iter()
        .map(|s| {
            let Some(id) = file_form_id(&s.file, s.local) else {
                log(&format!("{}: {} is not loaded", s.id, s.file));
                return 0;
            };
            let form = lookup(id);
            let kind = if form == 0 {
                0
            } else {
                unsafe { read::<u8>(form + FORM_TYPE) }
            };
            if kind != SHOUT_TYPE {
                log(&format!("{}: form {id:08X} is type {kind:#x}, not a shout", s.id));
                return 0;
            }
            form
        })
        .collect::<Vec<_>>();
    log(&format!(
        "resolved {}/{} shouts",
        forms.iter().filter(|&&f| f != 0).count(),
        forms.len()
    ));
    let _ = SHOUTS.set(forms);
}

pub fn label_spoken_words() {
    let Some(forms) = SHOUTS.get() else {
        return;
    };
    let make: extern "C" fn(usize, *const std::ffi::c_char) -> usize = unsafe { function(rt().make_string) };
    let mut labelled = 0;
    for (&shout, spec) in forms.iter().zip(&Table::embedded().shouts) {
        if shout == 0 {
            continue;
        }
        for (i, spoken) in spec.words.iter().enumerate() {
            let form = word(shout, i);
            if form == 0 || unsafe { read::<u8>(form + FORM_TYPE) } != WORD_TYPE {
                log(&format!("{}: word {i} is {}, not a word of power", spec.id, describe(form)));
                continue;
            }
            let current = unsafe { read::<*const std::ffi::c_char>(form + WORD_TRANSLATION) };
            if current.is_null() {
                continue;
            }
            let translation = unsafe { std::ffi::CStr::from_ptr(current) }.to_bytes();
            let prefix = format!("{spoken} (");
            if translation.is_empty() || translation.get(..prefix.len()).is_some_and(|t| t.eq_ignore_ascii_case(prefix.as_bytes())) {
                continue;
            }
            let label = std::ffi::CString::new(table::spoken_label(spoken, translation)).unwrap();
            make(form + WORD_TRANSLATION, label.as_ptr());
            labelled += 1;
        }
    }
    log(&format!("labelled {labelled} words with their spoken form"));
}

unsafe fn list(entries: usize, count: usize, light: bool) -> impl Iterator<Item = (usize, bool)> {
    (0..count).map(move |i| (unsafe { read::<usize>(entries + i * 8) }, light))
}

unsafe fn collection_files(collection: usize) -> Vec<(usize, bool)> {
    let array = |at: usize, light| unsafe { list(read::<usize>(at), read::<u32>(at + 0x10) as usize, light) };
    array(collection, false).chain(array(collection + 0x18, true)).collect()
}

fn vr_esl_collection() -> Option<usize> {
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    unsafe {
        let module = GetModuleHandleW(windows::core::w!("skyrimvresl.dll")).ok()?;
        let export = GetProcAddress(module, windows::core::s!("GetCompiledFileCollectionExtern"))?;
        let get: extern "C" fn() -> usize = std::mem::transmute(export);
        Some(get())
    }
}

fn loaded_files(handler: usize) -> Vec<(usize, bool)> {
    unsafe {
        match rt().files {
            Files::Collection(offset) => collection_files(handler + offset),
            Files::Inline { count, entries } => match vr_esl_collection() {
                Some(collection) => collection_files(collection),
                None => list(handler + entries, (read::<u32>(handler + count) as usize).min(INLINE_FILES), false).collect(),
            },
        }
    }
}

fn file_form_id(name: &str, local: u32) -> Option<u32> {
    let handler = unsafe { read::<usize>(base() + rt().data_handler) };
    if handler == 0 {
        return None;
    }
    loaded_files(handler).into_iter().find_map(|(file, light)| unsafe {
        let found = std::ffi::CStr::from_ptr((file + FILE_NAME) as *const std::ffi::c_char);
        if !found.to_bytes().eq_ignore_ascii_case(name.as_bytes()) {
            return None;
        }
        Some(if light {
            0xFE00_0000 | (read::<u16>(file + FILE_LIGHT_INDEX) as u32) << 12 | local & 0xFFF
        } else {
            (read::<u8>(file + FILE_INDEX) as u32) << 24 | local & 0xFF_FFFF
        })
    })
}

pub fn resolve_voice_power() -> bool {
    let Some(id) = file_form_id(esp::FILE, esp::VOICE_POWER) else {
        log(&format!("{} is not loaded", esp::FILE));
        return false;
    };
    let lookup: extern "C" fn(u32) -> usize = unsafe { function(rt().lookup_form_by_id) };
    let form = lookup(id);
    if form == 0 || unsafe { read::<u8>(form + FORM_TYPE) } != esp::SPELL_TYPE {
        log(&format!("voice power {id:08X} is {}, not a spell", describe(form)));
        return false;
    }
    let slot = lookup(esp::VOICE_EQUIP);
    if slot == 0 {
        log(&format!("voice equip slot {:08X} is missing", esp::VOICE_EQUIP));
        return false;
    }
    VOICE_POWER.store(form, Ordering::Release);
    VOICE_SLOT.store(slot, Ordering::Release);
    log(&format!("voice power {} slot {}", describe(form), describe(slot)));
    true
}

pub fn voice_power_selected() -> bool {
    let player = unsafe { read::<usize>(base() + rt().player) };
    let voice = VOICE_POWER.load(Ordering::Acquire);
    voice != 0
        && player != 0
        && (unsafe { read::<usize>(player + rt().equipped_shout) } == voice
            || PENDING.load(Ordering::Acquire) && PREVIOUS.load(Ordering::Acquire) == voice)
}

fn reequip_voice_power(player: usize) -> bool {
    let voice = VOICE_POWER.load(Ordering::Acquire);
    let manager = unsafe { read::<usize>(base() + rt().equip_manager) };
    if voice == 0
        || manager == 0
        || PREVIOUS.load(Ordering::Acquire) != voice
        || unsafe { read::<usize>(player + rt().equipped_shout) } == voice
    {
        return false;
    }
    let equip: extern "C" fn(usize, usize, usize, usize) = unsafe { function(rt().equip_spell) };
    equip(manager, player, voice, VOICE_SLOT.load(Ordering::Acquire));
    unsafe { read::<usize>(player + rt().equipped_shout) == voice }
}

fn word(shout: usize, index: usize) -> usize {
    unsafe { read::<usize>(shout + SHOUT_WORDS + index * WORD_STRIDE) }
}

fn variation_spell(shout: usize, index: usize) -> usize {
    unsafe { read::<usize>(shout + SHOUT_WORDS + index * WORD_STRIDE + 8) }
}

fn unlocked_words(shout: usize) -> u8 {
    (0..3)
        .take_while(|&i| {
            word(shout, i) != 0 && unsafe { read::<u32>(word(shout, i) + FORM_FLAGS) } & WORD_UNLOCKED != 0
        })
        .count() as u8
}

fn voice(player: usize) -> (u32, f32) {
    let state: extern "C" fn(usize) -> u32 = unsafe { function(rt().voice_state) };
    let recovery: extern "C" fn(usize) -> f32 = unsafe { function(rt().voice_recovery) };
    (state(player), recovery(player))
}

fn speech() -> usize {
    unsafe { read::<usize>(base() + rt().speech) }
}

fn clear_pending_shout() {
    let speech = speech();
    if speech != 0 {
        unsafe {
            write::<usize>(speech + SPEECH_SHOUT, 0);
            write::<usize>(speech + SPEECH_PREVIOUS, 0);
        }
    }
    PENDING.store(false, Ordering::Release);
}

pub fn reset() {
    if PENDING.load(Ordering::Acquire) {
        clear_pending_shout();
    }
}

fn describe(form: usize) -> String {
    if form == 0 {
        "none".into()
    } else {
        unsafe {
            format!(
                "{:08X}/{:#x}",
                read::<u32>(form + FORM_ID),
                read::<u8>(form + FORM_TYPE)
            )
        }
    }
}

pub fn settle() -> Option<String> {
    let player = unsafe { read::<usize>(base() + rt().player) };
    if player == 0 {
        clear_pending_shout();
        return Some("settled no_player".into());
    }
    let (state, recovery) = voice(player);
    if state != 0 {
        return None;
    }
    let mut equipped = unsafe { read::<usize>(player + rt().equipped_shout) };
    let restored = equipped == PREVIOUS.load(Ordering::Acquire);
    let reequipped = !restored && reequip_voice_power(player);
    if reequipped {
        equipped = unsafe { read::<usize>(player + rt().equipped_shout) };
    }
    clear_pending_shout();
    Some(format!(
        "settled confirmed={} recovery_s={recovery:.2} restored={restored} reequipped={reequipped} equipped_after={}",
        recovery > 0.0,
        describe(equipped)
    ))
}

fn blocked(player: usize) -> Option<&'static str> {
    unsafe {
        if read::<usize>(player + PARENT_CELL) == 0 || read::<usize>(player + LOADED_STATE) == 0 {
            return Some("not_in_world");
        }
        let ui = read::<usize>(base() + rt().ui);
        if ui == 0 {
            return Some("controls_unavailable");
        }
        if read::<u32>(ui + PAUSING_MENUS) != 0 {
            return Some("menu_open");
        }
        let controls = read::<usize>(base() + rt().control_map);
        let menu = read::<usize>(base() + rt().menu_controls);
        if controls == 0 || menu == 0 {
            return Some("controls_unavailable");
        }
        if read::<u32>(controls + rt().enabled_controls) & KINECT_SHOUT_CONTROLS != KINECT_SHOUT_CONTROLS {
            return Some("controls_disabled");
        }
        if read::<u8>(menu + rt().beast_form) != 0 {
            return Some("beast_form");
        }
    }
    None
}

pub fn execute(command: &Command) -> String {
    let player = unsafe { read::<usize>(base() + rt().player) };
    let Some(&shout) = SHOUTS.get().and_then(|s| s.get(command.shout)) else {
        return "rejected unresolved".into();
    };
    if shout == 0 || player == 0 {
        return format!("rejected unavailable shout={shout:#x} player={player:#x}");
    }
    if let Some(reason) = blocked(player) {
        return format!("rejected {reason}");
    }
    let tier = command.tier as usize;
    let (state, recovery) = voice(player);
    let unlocked = unlocked_words(shout);
    if state != 0 || PENDING.load(Ordering::Acquire) {
        return format!("rejected busy voice_state={state}");
    }
    if recovery > 0.0 {
        return format!("rejected recovering recovery_s={recovery:.2}");
    }
    if variation_spell(shout, tier - 1) == 0 || (unlocked as usize) < tier {
        return format!("rejected locked unlocked_words={unlocked}");
    }
    let speech = speech();
    let manager = unsafe { read::<usize>(base() + rt().equip_manager) };
    if speech == 0 || manager == 0 {
        return format!("rejected missing speech={speech:#x} manager={manager:#x}");
    }
    let equip: extern "C" fn(usize, usize, usize) = unsafe { function(rt().equip_shout) };
    let start: extern "C" fn(usize, usize, i32, usize) -> u8 = unsafe { function(rt().start_shout) };
    let previous = unsafe { read::<usize>(player + rt().equipped_shout) };
    PREVIOUS.store(previous, Ordering::Release);
    unsafe {
        write::<usize>(speech + SPEECH_SHOUT, shout);
        write::<usize>(speech + SPEECH_PREVIOUS, previous);
    }
    equip(manager, player, shout);
    let equipped = unsafe { read::<usize>(player + rt().equipped_shout) } == shout;
    let fired = equipped && start(player, shout, tier as i32 - 1, 0) != 0;
    let fired_us = clock_us();
    let reequipped = !fired && reequip_voice_power(player);
    if fired {
        PENDING.store(true, Ordering::Release);
    } else {
        clear_pending_shout();
    }
    let (state, _) = voice(player);
    let origin = match &command.origin {
        Origin::Hold { chunks } => format!("hold_chunks={chunks}"),
        #[cfg(feature = "test-hook")]
        Origin::Test(name) => format!("test={name}"),
    };
    format!(
        "{} equipped={equipped} previous={} tier={tier} voice_state={state} reequipped={reequipped} latency_ms={:.1} {origin}",
        if fired { "fired" } else { "rejected start_failed" },
        describe(previous),
        fired_us.saturating_sub(command.issued_us) as f64 / 1000.0
    )
}
