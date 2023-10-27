use std::{collections::HashMap, error::Error, fs::File};

use uuid::Uuid;

extern crate csv;

#[derive(Debug, Clone)]
enum Suite {
    BLACK,
    WHITE,
}

impl Suite {
    fn from_str(value: &str) -> Option<Suite> {
        match value {
            "Prompt" => Some(Suite::BLACK),
            "Response" => Some(Suite::WHITE),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Edition {
    id: Option<Uuid>,
    temp_id: Uuid,
    country_code: String,
    version: String,
}

#[derive(Debug, Clone)]
struct SetColumns {
    suite: usize,
    text: usize,
    special: usize,
    editions: HashMap<Uuid, usize>,
}

#[derive(Debug, Clone)]
struct Card {
    id: Option<Uuid>,
    temp_id: Uuid,
    suite: Suite,
    text: String,
    special: String,
    editions: Vec<Uuid>,
}
impl Card {
    fn new(suite: Suite, text: String, special: String) -> Self {
        let temp_id = Uuid::new_v4();
        Card {
            id: None,
            temp_id,
            suite,
            text,
            special,
            editions: Vec::new(),
        }
    }
}
#[derive(Debug, Clone)]
struct CardEdition {
    card_id: Uuid,
    edition_id: Uuid,
}
#[derive(Debug, Clone)]
struct Set {
    id: Option<u64>,
    temp_id: Uuid,
    name: String,
    cards: Vec<Card>,
    editions: Vec<Edition>,
}

impl Set {
    fn new(name: String) -> Self {
        let temp_id = Uuid::new_v4();
        Set {
            id: None,
            temp_id,
            name,
            cards: Vec::new(),
            editions: Vec::new(),
        }
    }
}

fn parse_set_columns(record: &csv::StringRecord) -> (Vec<SetColumns>) {
    let mut suite_index = None;
    let mut text_index = None;
    let mut special_index = None;

    let mut result = Vec::new();

    for (index, field) in record.iter().enumerate() {
        match field {
            "Set" => suite_index = Some(index),
            "Special" => special_index = Some(index),
            _ if field.len() > 3
                && suite_index.is_some()
                && index > suite_index.unwrap()
                && special_index.is_none() =>
            {
                text_index = Some(index);
            }
            _ => {}
        }

        if suite_index.is_some() && text_index.is_some() && special_index.is_some() {
            result.push(SetColumns {
                suite: suite_index.unwrap(),
                text: text_index.unwrap(),
                special: special_index.unwrap(),
                editions: HashMap::new(),
            });
            suite_index = None;
            text_index = None;
            special_index = None;
        }
    }

    result
}

fn parse_cards(
    record: &csv::StringRecord,
    sets: &mut HashMap<Uuid, Set>,
    cols: &mut HashMap<Uuid, SetColumns>,
) {
    if record.is_empty() {
        return;
    }

    for (set_id, col) in cols.iter() {
        let suite = record.get(col.suite).unwrap_or("");
        let text = record.get(col.text).unwrap_or("");
        let special = record.get(col.special).unwrap_or("");
        let mut editions: Vec<Uuid> = Vec::new();
        for (id, idx) in col.editions.iter() {
            if record.get(*idx).is_none() {
                continue;
            }
            editions.push(*id);
        }
        if let Some(suite) = Suite::from_str(suite) {
            let mut card = Card::new(suite, text.to_string(), special.to_string());
            card.editions = editions;
            if let Some(set) = sets.get_mut(&set_id) {
                set.cards.push(card);
            }
        }
    }
}

fn parse_csv_file(file_path: &str) -> Result<Vec<Set>, Box<dyn Error>> {
    let file = File::open(file_path)?;
    let mut rdr = csv::Reader::from_reader(file);

    let mut parsing: HashMap<Uuid, Set> = HashMap::new();
    let mut mapping: HashMap<Uuid, SetColumns> = HashMap::new();

    let mut sets: Vec<Set> = Vec::new();

    for result in rdr.records() {
        let record = result?;
        parse_cards(&record, &mut parsing, &mut mapping);

        let new_set_columns = parse_set_columns(&record);

        let mut finished = Vec::new();

        for set_column in &new_set_columns {
            let matching_set_column = mapping.iter().find(|(_, column)| {
                column.suite == set_column.suite
                    && column.text == set_column.text
                    && column.special == set_column.special
            });

            match matching_set_column {
                Some((key, _)) => {
                    if let Some(set) = parsing.get(key) {
                        sets.push(set.clone());
                    }
                    finished.push(key.clone());
                }
                None => {}
            }
        }

        for id in finished {
            let _ = parsing.remove(&id);
            let _ = mapping.remove(&id);
        }

        for set_column in new_set_columns {
            let s = Set::new(record[set_column.text as usize].to_string());
            let id = s.temp_id;
            parsing.insert(id, s);
            mapping.insert(id, set_column);
        }
    }
    sets.extend(parsing.values().cloned());

    Ok(sets)
}

fn main() -> Result<(), Box<dyn Error>> {
    let file_path = "./data/Cards Against Humanity - CAH Main Deck.csv";

    let sets = parse_csv_file(file_path)?;
    print!("found {} sets", sets.len());
    for set in sets {
        println!("{}", set.name);
        for card in &set.cards[0..10] {
            println!("Card: {}", card.text);
        }
    }

    Ok(())
}
