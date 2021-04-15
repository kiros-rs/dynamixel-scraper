use scraper::{Html, Selector};
use serde_yaml::Value;
use std::fs;


const NAVIGATION_URL: &str =
    "https://raw.githubusercontent.com/ROBOTIS-GIT/emanual/master/_data/navigation.yml";
const BASE_URL: &str = "https://emanual.robotis.com/docs/en";

fn parse_table(data: &str, index: usize) -> String {
    let document = Html::parse_document(data);
    let mut csv = String::new();

    let table_selector = Selector::parse("table").unwrap();
    let heading_selector = Selector::parse("thead").unwrap();
    let body_selector = Selector::parse("tbody").unwrap();
    let row_selector = Selector::parse("tr").unwrap();
    let th_selector = Selector::parse("th").unwrap();
    let td_selector = Selector::parse("td").unwrap();

    let table = document.select(&table_selector).nth(index).unwrap();
    let heading = table.select(&heading_selector).next().unwrap();
    let heading_row = heading.select(&row_selector).next().unwrap();
    let headings: Vec<_> = heading_row.select(&th_selector).collect();

    for item in headings {
        let text = String::from(item.text().collect::<String>());
        if csv.len() > 0 {
            csv.push_str(", ");
        }

        csv.push_str(&text);
    }

    let body = table.select(&body_selector).next().unwrap();
    let body_rows = body.select(&row_selector);

    for row in body_rows {
        let mut line = String::new();
        for item in row.select(&td_selector) {
            let text = item
                .text()
                .collect::<String>()
                .chars()
                .filter(|x| x != &',')
                .collect::<String>();
            if line.len() > 0 {
                line.push_str(", ");
            }

            line.push_str(&text);
        }

        csv.push('\n');
        csv.push_str(&line);
    }

    csv
}

fn merge_tables(url: &str, indexes: (usize, usize)) -> String {
    let resp = reqwest::blocking::get(url).unwrap();
    let data = resp.text().unwrap();
    let mut eeprom = parse_table(&data, indexes.0);
    let ram = parse_table(&data, indexes.1);

    // Make sure the headings are equal before combining
    assert_eq!(eeprom.lines().next(), ram.lines().next());
    eeprom.push('\n');
    eeprom.push_str(&ram.lines().skip(1).collect::<Vec<_>>().join("\n"));

    eeprom
}

fn main() -> Result<(), serde_yaml::Error> {
    let yaml = reqwest::blocking::get(NAVIGATION_URL).unwrap();
    let navigation: Value = serde_yaml::from_str(&yaml.text().unwrap())?;
    let dropdown_elements = &navigation["main"][0]["children"];

    for element in dropdown_elements.as_sequence().unwrap() {
        let title: String = element["title"]
            .as_str()
            .unwrap()
            .chars()
            .filter(|x| x != &'*')
            .collect();
        let mut counter = 0;
        if title.contains("Series") {
            let children = element["children"].as_sequence().unwrap();
            for child in children {
                let url = format!("{}{}", BASE_URL, child["url"].as_str().unwrap());
                let dir = format!("tables/{}", title.split_whitespace().next().unwrap());
                fs::create_dir_all(&dir).unwrap();

                let mut url_chunks = url.split('/');
                let path = format!(
                    "{}/{}.csv",
                    &dir,
                    url_chunks.nth(url.split('/').count() - 2).unwrap()
                );
                fs::write(path, merge_tables(&url, (1, 2))).unwrap();

                counter += 1;
            }

            println!("Found {} matches for {}", counter, title);
        }
    }

    Ok(())
}
