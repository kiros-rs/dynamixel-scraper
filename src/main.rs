mod analysis;
mod download;
mod serialize;

use anyhow::Result;
use clap::{App, Arg, ArgGroup};
use download::merge_tables;
use futures_util::stream::StreamExt;
use serde_yaml::Value;
use serialize::serialize_servo;
use std::fs;
use tokio_stream as stream;

#[derive(Clone, Debug)]
struct Actuator {
    url: String,
    series: String,
    raw_name: String,
    name: String,
    contents: Option<String>,
    pub text: String,
}

impl Actuator {
    fn new(url: String, name: String, series: String) -> Result<Actuator> {
        let raw_name = url.split('/').nth_back(1).unwrap();

        Ok(Actuator {
            url: url.clone(),
            series: series.split_whitespace().next().unwrap().to_string(),
            raw_name: raw_name.to_string(),
            name,
            contents: None,
            text: String::new(),
        })
    }

    fn get_contents(&mut self) -> Result<String> {
        if let Some(contents) = &self.contents {
            Ok(contents.to_string())
        } else {
            self.contents = Some(merge_tables(&self.text, (1, 2))?);

            Ok(self.contents.clone().unwrap())
        }
    }

    fn write_table(&mut self) -> Result<()> {
        fs::create_dir_all(format!("tables/{}", &self.series))?;
        let path = format!("tables/{}/{}.csv", &self.series, &self.raw_name);
        fs::write(path, self.get_contents()?)?;

        Ok(())
    }

    fn write_object(&mut self) -> Result<()> {
        fs::create_dir_all(format!("objects/{}", &self.series))?;
        let path = format!("objects/{}/{}.ron", &self.series, &self.raw_name);
        fs::write(path, serialize_servo(&self.get_contents()?)?)?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("Dynamixel Control Table Scraper")
                        .version("0.1")
                        .author("Angus Finch <developer.finchie@gmail.com>")
                        .about("Scrapes the Robotis E-Manual for Dynamixel control tables")
                        .arg(Arg::with_name("csv")
                            .long("csv")
                            .takes_value(false)
                            .help("If the control table should be output in CSV notation"))
                        .arg(Arg::with_name("ron")
                            .long("ron")
                            .takes_value(false)
                            .help("If the control table should be output in RON"))
                        .group(ArgGroup::with_name("format")
                            .args(&["csv", "ron"]))
                        .arg(Arg::with_name("dynamixel")
                            .short("d")
                            .long("dxl")
                            .value_name("SERVO")
                            .help("Specifies which files to download.")
                            .takes_value(true)
                            .multiple(true))
                        .arg(Arg::with_name("series")
                            .short("s")
                            .long("series")
                            .value_name("SERIES")
                            .help("Specifies which series of Dynamixel to download.")
                            .takes_value(true)
                            .multiple(true))
                        .group(ArgGroup::with_name("servo_choice")
                            .args(&["dynamixel", "series"])
                            .multiple(true))
                        .arg(Arg::with_name("navigation_url")
                            .long("navigation_url")
                            .default_value("https://raw.githubusercontent.com/ROBOTIS-GIT/emanual/master/_data/navigation.yml")
                            .help("Specify the location of the navigation URL used to locate Dynamixels"))
                        .arg(Arg::with_name("base_url")
                            .long("base_url")
                            .default_value("https://emanual.robotis.com/docs/en")
                            .help("Specify the base URL to use")).get_matches();

    let yaml = reqwest::get(matches.value_of("navigation_url").unwrap()).await?;
    let navigation: Value = serde_yaml::from_str(&yaml.text().await?)?;
    let dropdown_elements = &navigation["main"][0]["children"];

    let dxls: Option<Vec<&str>> = match matches.is_present("dynamixel") {
        true => Some(matches.values_of("dynamixel").unwrap().collect()),
        false => None,
    };
    let series: Option<Vec<&str>> = match matches.is_present("series") {
        true => Some(matches.values_of("series").unwrap().collect()),
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
                let url = format!(
                    "{}{}",
                    matches.value_of("base_url").unwrap(),
                    child["url"].as_str().unwrap()
                );
                let name = child["title"].as_str().unwrap().to_string();
                let dxl = Actuator::new(url, name, title.clone())?;

                if matches.is_present("servo_choice") {
                    if let Some(ref params) = dxls {
                        if params.contains(&&*dxl.raw_name) {
                            actuators.push(dxl);
                            continue;
                        }
                    }

                    if let Some(ref params) = series {
                        if params.contains(&title.split(' ').next().unwrap()) {
                            actuators.push(dxl)
                        }
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

        actuators[index].text = response.text().await?;

        if matches.is_present("format") {
            if matches.is_present("csv") {
                actuators[index].write_table()?;
            }

            if matches.is_present("ron") {
                actuators[index].write_object()?;
            }
        } else {
            actuators[index].write_table()?;
            actuators[index].write_object()?;
        }
    }

    Ok(())
}
