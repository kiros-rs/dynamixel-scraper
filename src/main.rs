mod analysis;
mod download;
mod serialize;

use analysis::display_analysis;
use anyhow::Result;
use clap::{App, Arg, SubCommand};
use download::merge_tables;
use futures_util::stream::StreamExt;
use serde_yaml::Value;
use serialize::serialize_servo;
use std::fs;
use tokio_stream as stream;

const NAVIGATION_URL: &str =
    "https://raw.githubusercontent.com/ROBOTIS-GIT/emanual/master/_data/navigation.yml";
const BASE_URL: &str = "https://emanual.robotis.com/docs/en";

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

    fn write_table(&self, text: &str) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = format!("{}/{}.csv", self.dir, self.raw_name);
        let contents = merge_tables(text, (1, 2))?;
        // display_analysis(&contents);
        // fs::write(path, contents)?;
        println!("Servo: {}", self.name);
        serialize_servo(&contents);

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("Dynamixel Control Table Scraper")
                        .version("0.1")
                        .author("Angus Finch <developer.finchie@gmail.com>")
                        .about("Scrapes the Robotis E-Manual for Dynamixel control tables")
                        .arg(Arg::with_name("dynamixel")
                            .short("d")
                            .long("dxl")
                            .value_name("SERVO")
                            .help("Specifies which files to download. If no files are specified, all will be downloaded")
                            .takes_value(true)
                            .multiple(true)).get_matches();

    let yaml = reqwest::get(NAVIGATION_URL).await?;
    let navigation: Value = serde_yaml::from_str(&yaml.text().await?)?;
    let dropdown_elements = &navigation["main"][0]["children"];
    let dxls: Option<Vec<&str>> = match matches.is_present("dynamixel") {
        true => Some(matches.values_of("dynamixel").unwrap().collect()),
        false => None,
    };

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

                if let Some(ref params) = dxls {
                    if params.contains(&&*dxl.raw_name) {
                        actuators.push(dxl);
                    }
                } else {
                    actuators.push(dxl);
                }
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
