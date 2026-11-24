pub const FILE: &str = "thuum.esp";
pub const VOICE_POWER: u32 = 0x801;
pub const SPELL_TYPE: u8 = 0x16;

const VERSION: f32 = 1.70;
const FORM_VERSION: u16 = 44;
const LIGHT: u32 = 0x200;
const OWN: u32 = 1 << 24;
const EFFECT: u32 = 0x800;
const QUEST: u32 = 0x802;
const PLAYER_REF: u32 = 0x14;
pub const VOICE_EQUIP: u32 = 0x025BEE;
const POWER_ICON: u32 = 0x0435A5;
const LESSER_POWER: u32 = 3;
const FIRE_AND_FORGET: u32 = 1;
const SELF: u32 = 0;
const SCRIPT_ARCHETYPE: u32 = 1;
const SILENT: u32 = 2;
const NO_DURATION_MAGNITUDE_AREA: u32 = 0x200 | 0x400 | 0x800;
const HIDE_IN_UI: u32 = 0x8000;
const MANUAL_COST: u32 = 1;
const START_GAME_ENABLED: u16 = 0x11;
const START_UP_STAGE: u8 = 2;
const NONE: u32 = 0xFFFF_FFFF;

fn field(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(kind);
    out.extend_from_slice(&u16::try_from(data.len()).unwrap().to_le_bytes());
    out.extend_from_slice(data);
}

fn zstring(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn record(kind: &[u8; 4], flags: u32, form: u32, body: &[u8]) -> Vec<u8> {
    let mut out = kind.to_vec();
    out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&form.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&FORM_VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn group(kind: &[u8; 4], records: &[u8]) -> Vec<u8> {
    let mut out = b"GRUP".to_vec();
    out.extend_from_slice(&u32::try_from(24 + records.len()).unwrap().to_le_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(records);
    out
}

fn effect() -> Vec<u8> {
    let f = |x: f32| x.to_bits();
    let data = words(&[
        NO_DURATION_MAGNITUDE_AREA | HIDE_IN_UI,
        f(0.0),
        0,
        NONE,
        NONE,
        0,
        0,
        f(0.0),
        0,
        0,
        0,
        0,
        f(0.0),
        f(0.0),
        f(0.0),
        f(0.0),
        SCRIPT_ARCHETYPE,
        NONE,
        0,
        0,
        FIRE_AND_FORGET,
        SELF,
        NONE,
        0,
        0,
        0,
        f(0.0),
        0,
        f(1.0),
        0,
        0,
        0,
        0,
        0,
        0,
        SILENT,
        f(0.0),
        f(0.0),
    ]);
    assert_eq!(data.len(), 152);
    let mut body = vec![];
    field(&mut body, b"EDID", &zstring("ThuumVoiceEffect"));
    field(&mut body, b"DATA", &data);
    field(&mut body, b"DNAM", &zstring(""));
    record(b"MGEF", 0, OWN | EFFECT, &body)
}

fn power() -> Vec<u8> {
    let mut body = vec![];
    field(&mut body, b"EDID", &zstring("ThuumVoice"));
    field(&mut body, b"OBND", &[0; 12]);
    field(&mut body, b"FULL", &zstring("Thuum"));
    field(&mut body, b"MDOB", &words(&[POWER_ICON]));
    field(&mut body, b"ETYP", &words(&[VOICE_EQUIP]));
    field(&mut body, b"DESC", &zstring(""));
    field(
        &mut body,
        b"SPIT",
        &words(&[0, MANUAL_COST, LESSER_POWER, 0, FIRE_AND_FORGET, SELF, 0, 0, 0]),
    );
    field(&mut body, b"EFID", &words(&[OWN | EFFECT]));
    field(&mut body, b"EFIT", &words(&[0, 0, 0]));
    record(b"SPEL", 0, OWN | VOICE_POWER, &body)
}

fn quest() -> Vec<u8> {
    let mut body = vec![];
    field(&mut body, b"EDID", &zstring("ThuumVoiceQuest"));
    let mut dnam = START_GAME_ENABLED.to_le_bytes().to_vec();
    dnam.extend_from_slice(&[0; 10]);
    field(&mut body, b"DNAM", &dnam);
    field(&mut body, b"NEXT", &[]);
    field(&mut body, b"INDX", &[0, 0, START_UP_STAGE, 0]);
    field(&mut body, b"ANAM", &words(&[1]));
    field(&mut body, b"ALST", &words(&[0]));
    field(&mut body, b"ALID", &zstring("Player"));
    field(&mut body, b"FNAM", &words(&[0]));
    field(&mut body, b"ALFR", &words(&[PLAYER_REF]));
    field(&mut body, b"ALSP", &words(&[OWN | VOICE_POWER]));
    field(&mut body, b"VTCK", &words(&[0]));
    field(&mut body, b"ALED", &[]);
    record(b"QUST", 0, OWN | QUEST, &body)
}

pub fn build() -> Vec<u8> {
    let groups = [
        group(b"MGEF", &effect()),
        group(b"SPEL", &power()),
        group(b"QUST", &quest()),
    ]
    .concat();
    let mut header = vec![];
    let mut hedr = VERSION.to_le_bytes().to_vec();
    hedr.extend_from_slice(&words(&[6, QUEST + 1]));
    field(&mut header, b"HEDR", &hedr);
    field(&mut header, b"CNAM", &zstring("thuum"));
    field(&mut header, b"MAST", &zstring("Skyrim.esm"));
    field(&mut header, b"DATA", &[0; 8]);
    [record(b"TES4", LIGHT, 0, &header), groups].concat()
}
