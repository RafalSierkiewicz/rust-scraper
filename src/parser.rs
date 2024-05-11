use regex::Regex;
use scraper::{selectable::Selectable, ElementRef, Html, Selector};

#[derive(Debug, PartialEq)]
pub struct PartData {
    pub price: f64,
    pub shop_id: i64,
    pub id: String,
    pub href: String,
}
#[derive(Debug, PartialEq)]
pub struct PartWithDetails {
    pub id: String,
    pub name: String,
    pub description: String,
    pub price: f64,
    pub stock: i32,
    pub other_ids: Vec<String>,
}

struct MySelectors {
    parts_selector: Selector,
    pt_num_selector: Selector,
    pt_num_a_selector: Selector,
    price_selector: Selector,
    p_selector: Selector,
    other_ids_selector: Selector,
    name_selector: Selector,
}

impl MySelectors {
    fn new() -> MySelectors {
        MySelectors {
            parts_selector: Selector::parse("div").unwrap(),
            pt_num_selector: Selector::parse(".col-lg-3.col-md-3.col-sm-9.col-xs-8").unwrap(),
            pt_num_a_selector: Selector::parse("div a").unwrap(),
            price_selector: Selector::parse(".col-lg-2.col-md-2.col-sm-3.col-xs-3").unwrap(),
            p_selector: Selector::parse("p").unwrap(),
            other_ids_selector: Selector::parse(".opistresc").unwrap(),
            name_selector: Selector::parse(
                ".col-lg-9.col-md-8.col-sm-7.col-xs-12.text-left.styl_exo",
            )
            .unwrap(),
        }
    }
}

pub fn get_pages_from_html(html: &Html) -> Option<i32> {
    let pagination_selector = Selector::parse(".pagination").unwrap();
    let page_selector = Selector::parse("li").unwrap();
    let page_num_selector = Selector::parse("a").unwrap();
    let pagination = html.select(&pagination_selector).next().unwrap();

    let last_page: Option<i32> = pagination
        .select(&page_selector)
        .into_iter()
        .map(|li| li.select(&page_num_selector).next().unwrap())
        .map(|a| a.text().into_iter().map(String::from).collect::<Vec<_>>())
        .flat_map(|el| el.into_iter().last())
        .last()
        .into_iter()
        .flat_map(|str| str.parse::<i32>().ok())
        .last();

    last_page.map(|total| println!("Total of {} found", total));

    last_page
}

pub fn get_parts_from_html(html: &Html) -> Vec<PartData> {
    let selectors = MySelectors::new();
    html.select(&selectors.parts_selector)
        .filter(|div| div.value().attr("id") == Some("prawy_bootstrap"))
        .into_iter()
        .flat_map(|parts| {
            let prices = get_prices(parts, &selectors);

            let parts_nums = get_ids(parts, &selectors);
            let digit_regex = Regex::new(r"\/(\d+)\/").unwrap();
            Iterator::zip(prices.into_iter(), parts_nums)
                .map(|(price, (href, id))| {
                    let their_id = digit_regex
                        .captures_iter(&href)
                        .map(|m| m.extract())
                        .map(|(_, [id_str])| id_str.parse::<i64>().unwrap())
                        .last()
                        .unwrap();

                    PartData {
                        price: price,
                        id: id,
                        shop_id: their_id,
                        href: href,
                    }
                })
                .collect::<Vec<PartData>>()
        })
        .collect()
}

pub fn get_part_details(html: &Html, parent: &PartData) -> PartWithDetails {
    let selectors = MySelectors::new();

    let name = html
        .select(&selectors.name_selector)
        .flat_map(|e| {
            e.text()
                .into_iter()
                .map(String::from)
                .map(|s| s.replace("\n", ""))
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.trim().is_empty())
        .next()
        .unwrap_or("empty".to_string());

    let description_with_other_ids = html
        .select(&selectors.other_ids_selector)
        .flat_map(|p| {
            p.text()
                .into_iter()
                .map(String::from)
                .map(|s| s.replace("\n", ""))
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<String>>();

    let b_selector = Selector::parse("b").unwrap();
    let stock_regex = Regex::new(r"(\d+)").unwrap();
    let stock: i32 = html
        .select(&selectors.p_selector)
        .filter(|div| div.value().attr("id") == Some("myDIV"))
        .map(|e| e.select(&b_selector).next().unwrap())
        .flat_map(|a| a.text().into_iter().map(String::from).collect::<Vec<_>>())
        .flat_map(|str| {
            stock_regex
                .captures_iter(&str)
                .map(|c| c.extract())
                .map(|(_, [str])| str.to_string())
                .map(|str| str.parse::<i32>().unwrap_or(0))
                .collect::<Vec<i32>>()
        })
        .last()
        .unwrap_or(0);

    let (description_arr, ids_str) =
        description_with_other_ids.split_at(description_with_other_ids.len() - 1);

    let description = description_arr
        .into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<String>>()
        .join("\n");

    let ids = ids_str
        .into_iter()
        .flat_map(|s| {
            s.split(",")
                .map(|s| s.trim().to_string())
                .collect::<Vec<String>>()
        })
        .collect::<Vec<String>>();

    PartWithDetails {
        id: parent.id.clone(),
        name: name,
        description: description,
        price: parent.price.clone(),
        stock: stock,
        other_ids: ids,
    }
}

fn get_prices(outer: ElementRef, selectors: &MySelectors) -> Vec<f64> {
    outer
        .select(&selectors.price_selector)
        .into_iter()
        .map(|price| price.select(&selectors.p_selector).next().unwrap())
        .map(|p| p.text().into_iter().map(String::from).collect::<Vec<_>>())
        .map(|v| v.into_iter().last())
        .flatten()
        .flat_map(|str| {
            str.split(' ')
                .into_iter()
                .map(|s| String::from(s))
                .collect::<Vec<String>>()
                .first()
                .map(|e| e.clone())
        })
        .map(|price| price.parse::<f64>().unwrap())
        .collect::<Vec<f64>>()
}

fn get_ids(outer: ElementRef, selectors: &MySelectors) -> Vec<(String, String)> {
    outer
        .select(&selectors.pt_num_selector)
        .into_iter()
        .map(|part| part.select(&selectors.pt_num_a_selector).next().unwrap())
        .map(|a| {
            let id = a
                .select(&selectors.p_selector)
                .map(|p| p.text().into_iter().map(String::from).collect::<Vec<_>>())
                .map(|v| v.into_iter().last())
                .flatten()
                .collect();
            a.value().attr("href").map(String::from).zip(Some(id))
        })
        .flatten()
        .collect::<Vec<(String, String)>>()
}

fn get_their_id(outer: ElementRef, selectors: &MySelectors) -> Vec<String> {
    outer
        .select(&selectors.pt_num_selector)
        .into_iter()
        .map(|part| part.select(&selectors.pt_num_a_selector).next().unwrap())
        .map(|p| p.text().into_iter().map(String::from).collect::<Vec<_>>())
        .map(|v| v.into_iter().last())
        .flatten()
        .collect::<Vec<String>>()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn shoud_parse_html() {
        let path: String = [env!("CARGO_MANIFEST_DIR"), "resources", "test", "main.html"].join("/");
        let html_string = std::fs::read_to_string(path).expect("Error reading file");

        let html: Html = Html::parse_document(&html_string);
        let result = get_parts_from_html(&html);
        println!("{:?}", result);
        assert_eq!(result.len(), 30)
    }

    #[test]
    fn shoud_parse_inner_html() {
        let path: String = [env!("CARGO_MANIFEST_DIR"), "resources", "test", "part.html"].join("/");
        let html_string = std::fs::read_to_string(path).expect("Error reading file");

        let html: Html = Html::parse_document(&html_string);
        let parent = PartData {
            price: 1062.0,
            id: String::from("Z 600-105 DOR"),
            shop_id: 0,
            href: String::from(
                "https://www.jeepchryslerparts.pl/sklep/produkt/6493/aktywator-napedu.html",
            ),
        };
        let result = get_part_details(&html, &parent);

        assert_eq!(
            result,
            PartWithDetails{ id: String::from("Z 600-105 DOR"), name: String::from("AKTYWATOR NAPĘDU"), description: String::from("AKTYWATOR NAPĘDU 4WD/AWD\nFORD EXPEDITION 2003-2015\nFORD F-150 2004-2015\nLINCOLN MARK LT 2006-2008\nLINCOLN NAVIGATOR 2003-2015\nDORMAN"), price: 1062.0, stock: 0, other_ids: vec!["600-105".to_string(), " 7L1Z3C247A".to_string(), " TCA107".to_string()] }
        )
    }

    #[test]
    fn shoud_parse_inner_html_with_stock() {
        let path: String = [
            env!("CARGO_MANIFEST_DIR"),
            "resources",
            "test",
            "submit.html",
        ]
        .join("/");
        let html_string = std::fs::read_to_string(path).expect("Error reading file");

        let html: Html = Html::parse_document(&html_string);
        let parent = PartData {
            price: 1062.0,
            id: String::from("Z 600-105 DOR"),
            shop_id: 0,
            href: String::from(
                "https://www.jeepchryslerparts.pl/sklep/produkt/6493/aktywator-napedu.html",
            ),
        };
        let result = get_part_details(&html, &parent);
        println!("{:?}", result.other_ids);
        assert_eq!(
            result.other_ids,
            vec![
                "CA9471".to_string(),
                "5015610AA".to_string(),
                "WIX 42190".to_string(),
                "AA3037".to_string()
            ]
        )
    }

    #[test]
    fn shoud_parse_pages_html() {
        let path: String = [env!("CARGO_MANIFEST_DIR"), "resources", "test", "main.html"].join("/");
        let html_string = std::fs::read_to_string(path).expect("Error reading file");

        let html: Html = Html::parse_document(&html_string);
        let result = get_pages_from_html(&html);

        let expected: Option<i32> = Some(259);

        assert_eq!(result, expected);
    }
}
