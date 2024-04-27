use std::{env, fs::File, path::PathBuf, sync::Arc};

use chrono::Utc;
use csv::Writer;
use futures::{future, StreamExt};
use parser::{PartData, PartWithDetails};
use scraper::Html;

mod parser;

const DURATION: std::time::Duration = std::time::Duration::from_secs(1);

#[tokio::main]
async fn main() -> Result<(), String> {
    async {
        let client = reqwest::Client::new();
        let dest = create_dest_file()?;
        let mut writer = Writer::from_path(dest).map_err(|e| e.to_string())?;
        let arc = Arc::new(client);

        println!("Sending request for parts from page number 1",);

        let html = get_html(&arc, &1).await?;
        let page_count = parser::get_pages_from_html(&html).unwrap_or_else(|| 0);
        let first_page_parts = parser::get_parts_from_html(&html);
        let first_page_details = fetch_part_detail(first_page_parts, &arc).await;

        println!("Total of {} pages is about to be downloaded", page_count);

        let _ = write_result(first_page_details, &mut writer);

        let res = fetch_and_save(&arc, &2, &mut writer).await;

        Ok(res)
    }
    .await
}

#[derive(serde::Serialize)]
struct Row<'a> {
    id: &'a str,
    name: &'a str,
    price: &'a f64,
    description: &'a str,
    other_ids: &'a str,
}
fn create_dest_file() -> Result<PathBuf, String> {
    let mut path = env::current_dir().map_err(|e| e.to_string())?;
    let now = Utc::now();
    path.push(format!("parts_{}.csv", now.timestamp()));
    let ret = path.clone();
    let _ = File::create(path);
    Ok(ret)
}

fn write_result(parts: Vec<PartWithDetails>, writer: &mut Writer<File>) -> () {
    println!("Saving total of {} parts to csv", parts.len());

    parts
        .into_iter()
        .map(|part| {
            writer
                .serialize(Row {
                    id: &part.id,
                    name: &part.name,
                    price: &part.price,
                    description: &part.description,
                    other_ids: &part.other_ids.join(",").to_string(),
                })
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<()>, String>>()
        .and_then(|_| Ok(()))
        .unwrap_or_else(|e| {
            println!("Error writing parts to file. Error {}", e);
            ()
        })
}

async fn fetch_and_save(arc: &Arc<reqwest::Client>, count: &i32, writer: &mut Writer<File>) -> () {
    futures::stream::iter(2..=count.clone())
        .then(|page_num| {
            let arc_clone = Arc::clone(arc);
            async move {
                println!(
                    "Sending request for parts from page number {} after awaiting {} seconds",
                    page_num,
                    DURATION.as_secs()
                );
                tokio::time::sleep(DURATION).await;
                let html = get_html(&arc_clone, &page_num).await;

                html.map(|r| parser::get_parts_from_html(&r))
                    .unwrap_or_else(|_| Vec::new())
            }
        })
        .then(|parts| fetch_part_detail(parts, arc))
        .for_each(|parts| future::ready(write_result(parts, writer)))
        .await
}

async fn fetch_part_detail(
    parts: Vec<PartData>,
    arc: &Arc<reqwest::Client>,
) -> Vec<PartWithDetails> {
    println!("Requesting for details of {} parts", parts.len());

    futures::stream::iter(parts.into_iter())
        .fold(Vec::new(), |mut acc, part| {
            let arc_clone = arc.clone();
            async move {
                tokio::time::sleep(DURATION).await;
                let html = request(&arc_clone, (*part.href).to_string()).await;
                let details = parser::get_part_details(&html.unwrap(), &part);
                acc.push(details);
                acc
            }
        })
        .await
}

async fn get_parts_from_pages(arc: &Arc<reqwest::Client>, count: &i32) -> Vec<PartData> {
    futures::stream::iter(2..=count.clone())
        .fold(Vec::new(), |acc, page_num| {
            let arc_clone = Arc::clone(arc);
            async move {
                let duration = std::time::Duration::from_secs(1);
                println!(
                    "Sending request for parts from page number {} after awaiting {} seconds",
                    page_num,
                    duration.as_secs()
                );
                tokio::time::sleep(duration).await;
                let html = get_html(&arc_clone, &page_num).await;

                let parts = html
                    .map(|r| parser::get_parts_from_html(&r))
                    .unwrap_or_else(|_| Vec::new());

                acc.into_iter().chain(parts.into_iter()).collect()
            }
        })
        .await
}

async fn get_html(client: &Arc<reqwest::Client>, page_num: &i32) -> Result<Html, String> {
    request(client, make_url(page_num)).await
}

async fn request(client: &Arc<reqwest::Client>, url: String) -> Result<Html, String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map(|html| Html::parse_document(&html))
        .map_err(|e| e.to_string())
}

fn make_url(page_num: &i32) -> String {
    [
        "https://www.jeepchryslerparts.pl/sklep/wszystkie-marki/",
        &page_num.to_string(),
        "/wszystkie-marki.html",
    ]
    .join("")
}
