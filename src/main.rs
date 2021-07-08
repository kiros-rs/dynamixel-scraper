mod create_lib;

pub mod analysis;
pub mod download;
pub mod serialize;

#[macro_use]
extern crate lazy_static;

use anyhow::Result;
use clap::{App, Arg, ArgGroup};
use download::merge_tables;
use futures_util::stream::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use serde_yaml::Value;
use serialize::{parse_servo, serialize_servo, ControlTableData};
use std::fs;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio_stream as stream;

static TICK_RATE: u64 = 50;

#[derive(Clone, Debug)]
pub struct Actuator {
    series: String,
    raw_name: String,
    name: String,
    data: Vec<ControlTableData>,
}

impl Actuator {
    pub fn new(url: String, name: String, text: String) -> Result<Actuator> {
        // Example URL: https://emanual.robotis.com/docs/en/dxl/ax/ax-12a/
        // Raw name: ax-12a
        // Series: ax
        let mut url_parts = url.split('/');
        let raw_name = url_parts.nth_back(1).unwrap();
        let series = url_parts.next_back().unwrap();

        Ok(Actuator {
            series: series.to_string(),
            raw_name: raw_name.to_string(),
            name,
            data: parse_servo(merge_tables(&text, (1, 2))?)?,
        })
    }

    pub fn write_object(&mut self) -> Result<()> {
        fs::create_dir_all(format!("objects/{}", &self.series))?;
        let path = format!("objects/{}/{}.ron", &self.series, &self.raw_name);
        fs::write(path, serialize_servo(&self.data)?)?;

        Ok(())
    }
}

#[derive(Debug)]
struct ActuatorIndex {
    pub url: String,
    pub name: String,
}

fn configure_spinner(spinner: &ProgressBar) {
    let style = ProgressStyle::default_spinner()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
        .template("{spinner:.green} [{elapsed_precise}] {msg:.cyan.bold}");
    spinner.set_style(style);
    spinner.enable_steady_tick(TICK_RATE);
}

fn configure_dxl_spinner(spinner: &ProgressBar) {
    let style = ProgressStyle::default_spinner()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
        .template("{prefix:.magenta.bold} {msg:.green}");
    spinner.set_style(style);
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("Dynamixel Control Table Scraper")
                        .version("0.1")
                        .author("Angus Finch <developer.finchie@gmail.com>")
                        .about("Scrapes the Robotis E-Manual for Dynamixel control tables")
                        .arg(Arg::with_name("lib")
                            .long("lib")
                            .takes_value(false)
                            .help("If the control table should be output as a Rust library"))
                        .arg(Arg::with_name("ron")
                            .long("ron")
                            .takes_value(false)
                            .help("If the control table should be output in RON"))
                        .group(ArgGroup::with_name("format")
                            .multiple(true)
                            .args(&["lib", "ron"]))
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

    let nav_download = ProgressBar::new_spinner().with_message("Fetching navigation index");
    configure_spinner(&nav_download);
    let yaml = reqwest::get(matches.value_of("navigation_url").unwrap()).await?;
    nav_download.finish();

    let yaml_parse = ProgressBar::new_spinner().with_message("Parsing YAML");
    configure_spinner(&yaml_parse);
    let navigation: Value = serde_yaml::from_str(&yaml.text().await?)?;
    let dropdown_elements = &navigation["main"][0]["children"];

    let dxls: Vec<&str> = match matches.is_present("dynamixel") {
        true => matches.values_of("dynamixel").unwrap().collect(),
        false => vec![],
    };
    let series: Vec<&str> = match matches.is_present("series") {
        true => matches.values_of("series").unwrap().collect(),
        false => vec![],
    };

    let mut indexes: Vec<ActuatorIndex> = Vec::new();

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
                let dxl = ActuatorIndex { url, name };

                if matches.is_present("servo_choice") {
                    if dxls.contains(&dxl.url.split('/').nth_back(1).unwrap()) {
                        indexes.push(dxl);
                        continue;
                    }

                    if series.contains(&title.split(' ').next().unwrap()) {
                        indexes.push(dxl)
                    }
                } else {
                    indexes.push(dxl);
                }
            }
        }
    }

    yaml_parse.finish();

    let counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let total = Arc::new(indexes.len());
    let fetch_progress =
        ProgressBar::new_spinner().with_message("Downloading & extracting Dynamixels");
    configure_spinner(&fetch_progress);
    fetch_progress.disable_steady_tick();

    // Thanks to http://patshaughnessy.net/2020/1/20/downloading-100000-files-using-async-rust
    let fetches = stream::iter(indexes)
        .map(|dxl| {
            let spinner = Arc::new(ProgressBar::new_spinner().with_message(dxl.name.clone()));
            configure_dxl_spinner(&spinner);

            counter.store(counter.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
            spinner.set_prefix(format!("{:?}/{}", counter, total));

            tokio::spawn(async move {
                let req = reqwest::get(&dxl.url).await.unwrap();
                let text = req.text().await.unwrap();
                let actuator = Actuator::new(dxl.url, dxl.name, text).unwrap();
                spinner.finish_and_clear();

                actuator
            })
        })
        .buffer_unordered(20)
        .collect::<Vec<_>>()
        .await;

    fetch_progress.tick();
    fetch_progress.finish();

    let data_write = ProgressBar::new_spinner().with_message("Writing data");
    configure_spinner(&data_write);
    let actuators: Vec<Actuator> = fetches.into_iter().map(|dxl| dxl.unwrap()).collect();
    if matches.is_present("format") {
        if matches.is_present("lib") {
            create_lib::create_lib(&actuators)?;
        }

        if matches.is_present("ron") {
            for mut dxl in actuators {
                dxl.write_object()?;
            }
        }
    } else {
        create_lib::create_lib(&actuators)?;
    }

    data_write.finish();

    Ok(())
}
