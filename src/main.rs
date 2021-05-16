use convert_case::{Case, Casing};
use futures_util::stream::StreamExt;
use scraper::{ElementRef, Html, Selector};
use serde_yaml::Value;
use std::fs;
use tokio_stream as stream;
use anyhow::Result;

const NAVIGATION_URL: &str =
    "https://raw.githubusercontent.com/ROBOTIS-GIT/emanual/master/_data/navigation.yml";
const BASE_URL: &str = "https://emanual.robotis.com/docs/en";

fn parse_table(table: ElementRef) -> Result<String> {
    let mut csv = String::new();

    let heading_selector = Selector::parse("thead>tr>th").unwrap();
    let body_selector = Selector::parse("tbody>tr>td").unwrap();

    let headings: Vec<_> = table.select(&heading_selector).collect();
    let mut num_headings = 0;
    let mut items_in_line = 0;

    for item in headings {
        let text = String::from(item.text().collect::<String>());
        if csv.len() > 0 {
            csv.push_str(", ");
        }

        csv.push_str(&text.to_case(Case::Title));
        num_headings += 1;
    }

    csv.push('\n');
    let body = table.select(&body_selector);

    for element in body {
        let mut line = String::new();
        let text = element
            .text()
            .collect::<String>()
            .chars()
            .filter(|x| x != &',')
            .collect::<String>();

        if text.is_empty() {
            continue;
        }

        if items_in_line > 0 {
            line.push_str(", ");
        }

        line.push_str(&text);
        items_in_line += 1;

        if items_in_line == num_headings {
            line.push('\n');
            items_in_line = 0;
        }

        csv.push_str(&line);
    }

    Ok(csv)
}

fn merge_tables(page: &str, indexes: (usize, usize)) -> Result<String> {
    let document = Html::parse_document(page);

    let table_selector = Selector::parse("table").unwrap();
    let eeprom_table = document.select(&table_selector).nth(indexes.0).unwrap();
    let ram_table = document.select(&table_selector).nth(indexes.1).unwrap();

    let mut eeprom = parse_table(eeprom_table)?;
    let ram = parse_table(ram_table)?;

    // Make sure the headings are equal before combining
    assert_eq!(eeprom.lines().next(), ram.lines().next());
    eeprom.push_str(&ram.lines().skip(1).collect::<Vec<_>>().join("\n"));

    Ok(eeprom)
}

#[derive(Clone, Debug)]
struct Actuator {
    url: String,
    dir: String,
    raw_name: String,
    name: String,
}

impl Actuator {
    fn new(url: String, name: String, series: String) -> Result<Actuator> {
        let raw_name = url.split('/').nth_back(1).unwrap();

        Ok(Actuator {
            url: url.clone(),
            dir: format!("tables/{}", series.split_whitespace().next().unwrap()),
            raw_name: raw_name.to_string(),
            name,
        })
    }

    fn write_table(&self, text: &str) -> Result<()>{
        fs::create_dir_all(&self.dir)?;
        let path = format!("{}/{}.csv", self.dir, self.raw_name);
        fs::write(path, merge_tables(text, (1, 2))?)?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let yaml = reqwest::get(NAVIGATION_URL).await?;
    let navigation: Value = serde_yaml::from_str(&yaml.text().await?)?;
    let dropdown_elements = &navigation["main"][0]["children"];

    let mut actuators: Vec<Actuator> = Vec::new();

    for element in dropdown_elements.as_sequence().unwrap() {
        let title: String = element["title"]
            .as_str()
            .unwrap()
            .chars()
            .filter(|x| x != &'*')
            .collect();
        if title.contains("Series") {
            let children = element["children"].as_sequence().unwrap();
            for child in children {
                let url = format!("{}{}", BASE_URL, child["url"].as_str().unwrap());
                let name = child["title"].as_str().unwrap().to_string();
                let dxl = Actuator::new(url, name, title.clone())?;
                actuators.push(dxl);
            }
        }
    }

    let client = reqwest::Client::new();
    let mut stream = stream::iter(actuators.clone())
        .map(|dxl| client.get(&dxl.url).send())
        .buffer_unordered(20);

    while let Some(Ok(response)) = stream.next().await {
        let index = actuators
            .iter()
            .position(|x| x.url == response.url().as_str())
            .unwrap();
        let text = response.text().await?;
        actuators[index].write_table(&text)?;
    }

    Ok(())
}
