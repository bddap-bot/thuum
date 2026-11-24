use std::collections::HashMap;
use thuum::esp::{SPELL_TYPE, VOICE_POWER, build};

type Fields = Vec<([u8; 4], Vec<u8>)>;

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

fn fields(body: &[u8]) -> Fields {
    let mut out = vec![];
    let mut i = 0;
    while i < body.len() {
        let len = u16::from_le_bytes([body[i + 4], body[i + 5]]) as usize;
        out.push((body[i..i + 4].try_into().unwrap(), body[i + 6..i + 6 + len].to_vec()));
        i += 6 + len;
    }
    assert_eq!(i, body.len());
    out
}

fn get<'a>(f: &'a Fields, kind: &[u8; 4]) -> Vec<&'a [u8]> {
    f.iter().filter(|(k, _)| k == kind).map(|(_, v)| v.as_slice()).collect()
}

struct Plugin {
    flags: u32,
    header: Fields,
    groups: Vec<[u8; 4]>,
    records: HashMap<u32, ([u8; 4], Fields)>,
}

fn parse(data: &[u8]) -> Plugin {
    assert_eq!(&data[..4], b"TES4");
    let size = u32_at(data, 4) as usize;
    let mut plugin = Plugin {
        flags: u32_at(data, 8),
        header: fields(&data[24..24 + size]),
        groups: vec![],
        records: HashMap::new(),
    };
    let mut i = 24 + size;
    while i < data.len() {
        assert_eq!(&data[i..i + 4], b"GRUP");
        let end = i + u32_at(data, i + 4) as usize;
        let label: [u8; 4] = data[i + 8..i + 12].try_into().unwrap();
        plugin.groups.push(label);
        let mut j = i + 24;
        while j < end {
            let kind: [u8; 4] = data[j..j + 4].try_into().unwrap();
            assert_eq!(kind, label);
            let size = u32_at(data, j + 4) as usize;
            assert_eq!(u16::from_le_bytes([data[j + 20], data[j + 21]]), 44);
            let previous = plugin
                .records
                .insert(u32_at(data, j + 12), (kind, fields(&data[j + 24..j + 24 + size])));
            assert!(previous.is_none());
            j += 24 + size;
        }
        assert_eq!(j, end);
        i = end;
    }
    plugin
}

#[test]
fn voice_power_plugin() {
    let p = parse(&build());
    assert_eq!(p.flags & 0x200, 0x200, "light");
    assert_eq!(p.flags & 0x80, 0, "not localized");
    assert_eq!(get(&p.header, b"MAST"), [b"Skyrim.esm\0"]);
    let hedr = get(&p.header, b"HEDR")[0];
    assert_eq!(f32::from_le_bytes(hedr[..4].try_into().unwrap()), 1.70);
    assert_eq!(u32_at(hedr, 4) as usize, p.records.len() + p.groups.len());
    let next = u32_at(hedr, 8);
    for &id in p.records.keys() {
        assert_eq!(id >> 24, 1);
        assert!((0x800..next).contains(&(id & 0xFF_FFFF)), "light form id {id:08X}");
    }
    assert!(next <= 0x1000);
    assert_eq!(p.groups, [*b"MGEF", *b"SPEL", *b"QUST"]);
    let own = |kind: &[u8; 4]| -> (u32, &Fields) {
        let (id, (_, f)) = p.records.iter().find(|(_, (k, _))| k == kind).unwrap();
        (*id, f)
    };

    let (effect, mgef) = own(b"MGEF");
    let data = get(mgef, b"DATA")[0];
    assert_eq!(data.len(), 152);
    assert_eq!(u32_at(data, 16 * 4), 1, "script archetype, no script");
    assert_eq!(u32_at(data, 0) & 0x8000, 0x8000, "hidden");

    let (power, spel) = own(b"SPEL");
    assert_eq!(power, 1 << 24 | VOICE_POWER);
    assert_eq!(SPELL_TYPE, 0x16);
    assert_eq!(get(spel, b"FULL"), [b"Thuum\0"]);
    assert_eq!(get(spel, b"ETYP"), [0x025BEEu32.to_le_bytes()]);
    let spit = get(spel, b"SPIT")[0];
    assert_eq!(spit.len(), 36);
    assert_eq!(u32_at(spit, 8), 3, "lesser power");
    assert_eq!(get(spel, b"EFID"), [effect.to_le_bytes()]);
    assert_eq!(get(spel, b"EFIT")[0].len(), 12);

    let (_, qust) = own(b"QUST");
    assert_eq!(get(qust, b"DNAM")[0].len(), 12);
    assert_eq!(get(qust, b"DNAM")[0][0] & 1, 1, "start game enabled");
    assert_eq!(get(qust, b"ALFR"), [0x14u32.to_le_bytes()]);
    assert_eq!(get(qust, b"ALSP"), [power.to_le_bytes()]);
    let order: Vec<&[u8; 4]> = qust.iter().map(|(k, _)| k).skip_while(|k| *k != b"ALST").collect();
    assert_eq!(order.first(), Some(&b"ALST"));
    assert_eq!(order.last(), Some(&b"ALED"));
}
