use actix_multipart::{
    form::{
        tempfile::{TempFile, TempFileConfig},
        MultipartForm,
    },
    Multipart,
};
use std::{collections::HashMap, error::Error, fs::{File, self}};

use actix_web::{get, web::{self, Redirect}, App, Error as ActixError, HttpResponse, HttpServer, Responder};
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

fn parse_set_columns(record: &csv::StringRecord) -> Vec<SetColumns> {
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

fn parse_field(record: &csv::StringRecord, idx: usize) -> String {
    record
        .get(idx)
        .map_or_else(|| String::from(""), |s| s.to_string())
}

fn parse_cards(
    record: &csv::StringRecord,
    mapping: HashMap<Uuid, SetColumns>,
) -> HashMap<Uuid, Vec<Card>> {
    let mut cards: HashMap<Uuid, Vec<Card>> = HashMap::new();

    for (set_id, col) in mapping.iter() {
        let mut editions: Vec<Uuid> = Vec::new();
        for (id, idx) in col.editions.iter() {
            if record.get(*idx).is_none() {
                continue;
            }
            editions.push(*id);
        }
        if let Some(suite) = Suite::from_str(&parse_field(&record, col.suite)) {
            let mut card = Card::new(
                suite,
                parse_field(&record, col.text),
                parse_field(&record, col.special),
            );
            card.editions = editions;
            cards.entry(*set_id).or_insert(Vec::new()).push(card)
        }
    }
    return cards;
}

fn parse_csv_file(file_path: &str) -> Result<Vec<Set>, Box<dyn Error>> {
    let file = File::open(file_path)?;
    let mut rdr = csv::Reader::from_reader(file);

    let mut parsing: HashMap<Uuid, Set> = HashMap::new();
    let mut mapping: HashMap<Uuid, SetColumns> = HashMap::new();

    let mut sets: Vec<Set> = Vec::new();

    for result in rdr.records() {
        let record = result?;
        let cards = parse_cards(&record, mapping.clone());
        for (set_id, cards) in cards {
            if let Some(set) = parsing.get_mut(&set_id) {
                set.cards.extend(cards)
            }
        }

        let new_set_columns = parse_set_columns(&record);

        let finished: Vec<Uuid> = new_set_columns
            .iter()
            .filter_map(|set_column| {
                mapping
                    .iter()
                    .find(|(_, column)| {
                        column.suite == set_column.suite
                            && column.text == set_column.text
                            && column.special == set_column.special
                    })
                    .map(|(key, _)| key.clone())
            })
            .collect();

        for id in finished {
            if let Some(set) = parsing.get(&id) {
                sets.push(set.clone());
            }
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

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[derive(Debug, MultipartForm)]
struct UploadForm {
    #[multipart(rename = "file")]
    files: Vec<TempFile>,
}

async fn upload_csv(
    MultipartForm(form): MultipartForm<UploadForm>,
) -> Result<impl Responder, ActixError> {
    for f in form.files {
        let path = format!("./tmp/{}", f.file_name.unwrap());
        println!("saving to {path}");
        f.file.persist(&path).unwrap();
        // Process the uploaded CSV data
        let sets = parse_csv_file(&path)?;
        match fs::remove_file(path) {
            Ok(_) => {
                println!("File deleted successfully.");
            }
            Err(err) => {
                eprintln!("Failed to delete the file: {:?}", err);
            }
        }
        print!("found {} sets", sets.len());
        for set in sets {
            println!("{}", set.name);
            for card in &set.cards[0..10] {
                println!("Card: {}", card.text);
            }
        }
    }

    Ok(Redirect::to("localhost:12001").permanent())
}

async fn index() -> HttpResponse {
    let html = r#"<html>
        <head><title>Upload Test</title></head>
        <body>
            <form target="/" method="post" enctype="multipart/form-data">
                <input type="file" multiple name="file"/>
                <button type="submit">Submit</button>
            </form>
        </body>
    </html>"#;

    HttpResponse::Ok().body(html)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("./tmp")?;

    HttpServer::new(|| {
        App::new()
            .app_data(TempFileConfig::default().directory("./tmp"))
            .service(
                web::resource("/")
                    .route(web::get().to(index))
                    .route(web::post().to(upload_csv)),
            )
    })
    .bind(("127.0.0.1", 12001))?
    .workers(2)
    .run()
    .await

    // let file_path = "./data/Cards Against Humanity - CAH Main Deck.csv";

    // Ok(())
}
