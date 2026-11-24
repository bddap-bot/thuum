use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Shout {
    pub id: String,
    pub file: String,
    pub local: u32,
    pub words: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub id: String,
    pub shout: usize,
    pub tier: u8,
    pub words: Vec<String>,
}

pub struct Table {
    pub shouts: Vec<Shout>,
    pub phrases: Vec<Phrase>,
}

impl Table {
    pub fn embedded() -> Table {
        Table::parse(
            include_str!("../../data/shouts.json"),
            include_str!("../../data/game-forms.json"),
        )
    }

    pub fn parse(shouts: &str, forms: &str) -> Table {
        let shouts: Value = serde_json::from_str(shouts).unwrap();
        let forms: Value = serde_json::from_str(forms).unwrap();
        assert_eq!(shouts["schema"], 1);
        assert_eq!(forms["schema"], 1);
        let forms = forms["shouts"].as_object().unwrap();
        let mut table = Table {
            shouts: vec![],
            phrases: vec![],
        };
        for shout in shouts["shouts"].as_array().unwrap() {
            let id = shout["id"].as_str().unwrap().to_owned();
            let form = &forms[&id];
            let local = u32::from_str_radix(form["id"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
            let index = table.shouts.len();
            let words: Vec<String> = shout["words"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(words.len(), 3, "{id}: a shout has three words");
            assert!(words.iter().all(|w| !w.is_empty()), "{id}: a word is empty");
            for tier in 1..=3u8 {
                table.phrases.push(Phrase {
                    id: format!("{id}:{tier}"),
                    shout: index,
                    tier,
                    words: words[..tier as usize].to_vec(),
                });
            }
            assert!(local < 1 << 24, "{id}: local form id out of range");
            table.shouts.push(Shout {
                id,
                file: form["file"].as_str().unwrap().to_owned(),
                local,
                words,
            });
        }
        assert_eq!(
            table.shouts.len(),
            forms.len(),
            "game forms must cover exactly the shout table"
        );
        table
    }

    pub fn phrase(&self, id: &str) -> Option<usize> {
        self.phrases.iter().position(|p| p.id == id)
    }

    pub fn targets(&self) -> Vec<Vec<usize>> {
        let mut vocabulary: Vec<&str> = self
            .phrases
            .iter()
            .flat_map(|p| p.words.iter().map(String::as_str))
            .collect();
        vocabulary.sort_unstable();
        vocabulary.dedup();
        self.phrases
            .iter()
            .map(|p| {
                p.words
                    .iter()
                    .map(|w| vocabulary.binary_search(&w.as_str()).unwrap() + 1)
                    .collect()
            })
            .collect()
    }
}

pub fn spoken_label(word: &str, translation: &[u8]) -> Vec<u8> {
    let mut label = word.as_bytes().to_vec();
    label[0] = label[0].to_ascii_uppercase();
    label.extend_from_slice(b" (");
    label.extend_from_slice(translation);
    label.push(b')');
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_forms_name_their_plugin_file_and_local_id() {
        let table = Table::embedded();
        let form = |name: &str| {
            let s = table.shouts.iter().find(|s| s.id == name).unwrap();
            (s.file.as_str(), s.local)
        };
        assert_eq!(form("unrelenting_force"), ("Skyrim.esm", 0x01_3E07));
        assert_eq!(form("call_durnehviir"), ("Dawnguard.esm", 0x00_30D2));
        assert_eq!(form("dragon_aspect"), ("Dragonborn.esm", 0x01_DF92));
    }

    #[test]
    fn targets_index_the_sorted_vocabulary_and_extend_by_prefix() {
        let table = Table::embedded();
        let targets = table.targets();
        let mut tokens: Vec<usize> = targets.iter().flatten().copied().collect();
        tokens.sort_unstable();
        tokens.dedup();
        assert_eq!(tokens, (1..=81).collect::<Vec<_>>());
        let uf = |tier: u8| &targets[table.phrase(&format!("unrelenting_force:{tier}")).unwrap()];
        assert_eq!(uf(3).len(), 3);
        assert_eq!(&uf(3)[..2], uf(2).as_slice());
    }

    #[test]
    fn labels_lead_with_the_spoken_word_and_keep_the_translation_bytes() {
        assert_eq!(spoken_label("fus", b"Force"), b"Fus (Force)");
        assert_eq!(spoken_label("iiz", b"Eis \xdf"), b"Iiz (Eis \xdf)");
    }
}
