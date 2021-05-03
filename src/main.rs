use convert_case::{Case, Casing};
use scraper::{ElementRef, Html, Selector};
use serde_yaml::Value;
use std::fs;
use threadpool::ThreadPool;

const NAVIGATION_URL: &str =
    "https://raw.githubusercontent.com/ROBOTIS-GIT/emanual/master/_data/navigation.yml";
const BASE_URL: &str = "https://emanual.robotis.com/docs/en";

fn parse_table(table: ElementRef) -> String {
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

    csv
}

fn merge_tables(url: &str, indexes: (usize, usize)) -> String {
    let resp = reqwest::blocking::get(url).unwrap();
    let document = Html::parse_document(&resp.text().unwrap());

    let table_selector = Selector::parse("table").unwrap();
    let eeprom_table = document.select(&table_selector).nth(indexes.0).unwrap();
    let ram_table = document.select(&table_selector).nth(indexes.1).unwrap();

    let mut eeprom = parse_table(eeprom_table);
    let ram = parse_table(ram_table);

    // Make sure the headings are equal before combining
    assert_eq!(eeprom.lines().next(), ram.lines().next());
    eeprom.push_str(&ram.lines().skip(1).collect::<Vec<_>>().join("\n"));

    eeprom
}

struct Actuator {
    url: String,
    dir: String,
    raw_name: String,
    name: String,
}

impl Actuator {
    fn new(url: String, name: String, series: String) -> Actuator {
        let raw_name = url.split('/').nth_back(1).unwrap();

        Actuator {
            url: url.clone(),
            dir: format!("tables/{}", series.split_whitespace().next().unwrap()),
            raw_name: raw_name.to_string(),
            name,
        }
    }

    fn write_table(&self) {
        fs::create_dir_all(&self.dir).unwrap();
        let path = format!("{}/{}.csv", self.dir, self.raw_name);
        fs::write(path, merge_tables(&self.url, (1, 2))).unwrap();
    }
}

fn main() -> Result<(), serde_yaml::Error> {
    let yaml = reqwest::blocking::get(NAVIGATION_URL).unwrap();
    let navigation: Value = serde_yaml::from_str(&yaml.text().unwrap())?;
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
                let dxl = Actuator::new(url, name, title.clone());
                actuators.push(dxl);
            }
        }
    }

    let actuator_pool = ThreadPool::new(actuators.len());
    for actuator in actuators {
        actuator_pool.execute(move || {
            actuator.write_table();
        })
    }

    actuator_pool.join();

    Ok(())
}
