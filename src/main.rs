use actix_multipart::form::{
    tempfile::{TempFile, TempFileConfig},
    MultipartForm,
};
use serde::Deserialize;
use serde::Serialize;
use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
};

use mongodb::{bson::doc, Client, Collection, Database};

use actix_web::{
    get,
    web::{self, Redirect},
    App, Error as ActixError, HttpResponse, HttpServer, Responder,
};
use uuid::Uuid;

extern crate csv;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Suite {
    PROMPT,
    RESPONSE,
}

impl Suite {
    fn from_str(value: &str) -> Option<Suite> {
        match value {
            "Prompt" => Some(Suite::PROMPT),
            "Response" => Some(Suite::RESPONSE),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Edition {
    uuid: Uuid,
    set_uuid: Uuid,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Card {
    uuid: Uuid,
    suite: Suite,
    text: String,
    special: String,
    editions: Vec<Uuid>,
}
impl Card {
    fn new(suite: Suite, text: String, special: String) -> Self {
        Card {
            uuid: Uuid::new_v4(),
            suite,
            text,
            special,
            editions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Set {
    pub uuid: Uuid,
    pub name: String,
    #[serde(skip)]
    pub cards: Vec<Card>,
    #[serde(skip)]
    pub editions: Vec<Edition>,
}

impl Set {
    fn new(name: String) -> Self {
        Set {
            uuid: Uuid::new_v4(),
            name,
            cards: Vec::new(),
            editions: Vec::new(),
        }
    }
}

fn parse_set_editions(record: &csv::StringRecord) -> HashMap<Uuid, HashMap<usize, String>> {
    let mut result: HashMap<Uuid, HashMap<usize, String>> = HashMap::new();

    let mut current_set_uuid: Option<Uuid> = None;
    let mut current_set_index: Option<usize> = None;

    for (index, field) in record.iter().enumerate() {
        if field == "Edition" {
            current_set_uuid = Some(Uuid::new_v4());
            current_set_index = Some(index);
        } else if !field.is_empty() {
            if let (Some(set_uuid), Some(index)) = (current_set_uuid, current_set_index) {
                result
                    .entry(set_uuid)
                    .or_insert_with(HashMap::new)
                    .insert(index, field.to_string());
            }
        }
    }
    for (uuid, editions) in &result {
        println!("UUID: {:?}", uuid);
        for (index, edition) in editions {
            println!("Index: {}, Edition: {:?}", index, edition);
        }
    }
    return result;
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
        let _= parse_set_editions(&record);

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
            let id = s.uuid;
            parsing.insert(id, s);
            mapping.insert(id, set_column);
        }
    }
    sets.extend(parsing.values().cloned());

    Ok(sets)
}

#[derive(Debug, MultipartForm)]
struct UploadForm {
    #[multipart(rename = "file")]
    files: Vec<TempFile>,
}

async fn save_set(set: &Set) -> Result<(), mongodb::error::Error> {
    let uri = "mongodb://admin:mypassword@localhost:27017";
    let client = Client::with_uri_str(uri).await?;
    let database = client.database("controversy");
    let sets_collection: Collection<Set> = database.collection("sets");
    match sets_collection.insert_one(set, None).await {
        Ok(_) => {
            println!("Successfully added set {:?}", set.name);
            return Ok(());
        }
        Err(err) => {
            // Handle other errors if necessary
            eprintln!("Error inserting set: {}", err);

            return Ok(());
        }
    }
}
async fn save_cards(cards: &Vec<Card>) -> Result<(), mongodb::error::Error> {
    let uri = "mongodb://admin:mypassword@localhost:27017";
    let client = Client::with_uri_str(uri).await?;
    let database = client.database("controversy");
    let card_collection: Collection<Card> = database.collection("cards");
    card_collection.insert_many(cards, None).await?;
    Ok(())
}

async fn add_set(set: &Set) -> Result<(), mongodb::error::Error> {
    save_set(set).await?;
    save_cards(&set.cards).await?;
    Ok(())
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
                println!("Failed to delete the file: {:?}", err);
            }
        }
        println!("found {} sets", sets.len());
        for set in sets {
            // let _ = add_set(&set).await;
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
