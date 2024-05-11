use std::{env, fs::File, future::IntoFuture, path::PathBuf, sync::Arc};

use chrono::Utc;
use clap::{arg, command, Parser};
use csv::Writer;
use futures::{future, FutureExt, StreamExt, TryFutureExt};
use parser::{PartData, PartWithDetails};
use scraper::Html;

mod parser;

const DURATION: std::time::Duration = std::time::Duration::from_secs(1);
const RETRY_COUNT: u8 = 10;
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    limit: Option<i32>,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = Args::parse();
    async {
        let client = reqwest::Client::new();
        let dest = create_dest_file()?;
        let mut writer = Writer::from_path(dest).map_err(|e| e.to_string())?;
        let arc = Arc::new(client);

        println!("Sending request for parts from page number 1",);

        let html = get_html(&arc, &1).await?;
        let page_count = args
            .limit
            .unwrap_or_else(|| parser::get_pages_from_html(&html).unwrap_or_else(|| 0));
        let first_page_parts = parser::get_parts_from_html(&html);
        let first_page_details = fetch_part_detail(first_page_parts, &arc).await;

        println!("Total of {} pages is about to be downloaded", page_count);

        let _ = write_result(first_page_details, &mut writer);

        let res = fetch_and_save(&arc, &page_count, &mut writer).await;

        Ok(res)
    }
    .await
}

#[derive(serde::Serialize)]
struct Row<'a> {
    id: &'a str,
    name: &'a str,
    price: &'a f64,
    stock: &'a i32,
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
                    stock: &part.stock,
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
                let html = get_part_with_stock(&arc_clone, &part).await;
                let details = parser::get_part_details(&html.unwrap(), &part);
                acc.push(details);
                acc
            }
        })
        .await
}

async fn get_html(client: &Arc<reqwest::Client>, page_num: &i32) -> Result<Html, String> {
    request(client, make_url(page_num)).await
}

async fn request(client: &Arc<reqwest::Client>, url: String) -> Result<Html, String> {
    let mut counter = 0;
    loop {
        let result = client
            .get(&url)
            .send()
            .map(
                |response| match response.map(|e| e.text().map_err(|e| e.to_string())) {
                    Ok(future) => future.boxed(),
                    Err(err) => future::ready(Err(err.to_string())).boxed(),
                },
            )
            .flatten()
            .await
            .map(|html| Html::parse_document(&html))
            .map_err(|e| {
                println!("Error sending request {:?}", e);
                e.to_string()
            });

        match result {
            Ok(res) => break Ok(res),
            Err(underlying) => {
                counter += 1;

                if counter > RETRY_COUNT {
                    println!("Retry count exhausted ... Stopping execution");
                    break Err(underlying);
                } else {
                    println!(
                        "Error during sending request for details. Retry {} max {}. Sleeping 1 sec before retry ...",
                        counter, RETRY_COUNT
                    );
                    tokio::time::sleep(DURATION).await;
                }
            }
        }
    }
}

async fn get_part_with_stock(
    client: &Arc<reqwest::Client>,
    part: &PartData,
) -> Result<Html, String> {
    let mut counter = 0;

    loop {
        let form: reqwest::multipart::Form = reqwest::multipart::Form::new()
            .text("ilosc_prod", "10000")
            .text("produkt_id", part.shop_id.to_string().clone())
            .text("dodajdokoszyka", "");

        let result = client
            .post(part.href.clone())
            .multipart(form)
            .send()
            .map(
                |response| match response.map(|e| e.text().map_err(|e| e.to_string())) {
                    Ok(future) => future.boxed(),
                    Err(err) => future::ready(Err(err.to_string())).boxed(),
                },
            )
            .flatten()
            .await
            .map(|html| Html::parse_document(&html))
            .map_err(|e| {
                println!("Error sending request {:?}", e);
                e
            });

        match result {
            Ok(response) => break Ok(response),
            Err(underlying) => {
                counter += 1;
                if counter > RETRY_COUNT {
                    println!("Retry count exhausted ... Cancelling execution");
                    break Err(underlying);
                } else {
                    println!(
                        "Error during sending request for details. Retry {} max {}. Sleeping 1 sec before retry ...",
                        counter, RETRY_COUNT
                    );
                    tokio::time::sleep(DURATION).await;
                }
            }
        }
    }
}

fn make_url(page_num: &i32) -> String {
    [
        "https://www.jeepchryslerparts.pl/sklep/wszystkie-marki/",
        &page_num.to_string(),
        "/wszystkie-marki.html",
    ]
    .join("")
}
