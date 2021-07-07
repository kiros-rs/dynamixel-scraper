use crate::Actuator;
use anyhow::Result;
use std::collections::BTreeMap;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::iter::FromIterator;

static CARGO_PREAMBLE: &str = "[package]
name = \"dxl-control-tables\"
version = \"0.1.0\"
edition = \"2018\"

[dependencies]
thiserror = \"1.0.26\"

[features]
";
static ERROR_DEFINITION: &str = "use thiserror::Error;

#[derive(Error, Debug)]
pub enum ControlTableError {
    #[error(\"Dynamixel model {model:?} does not support field {name:?}\")]
    NoMatchingAddress { model: Model, name: DataName },
}

";
static DERIVES: &str = "#[derive(Debug)]";
static INDENT: &str = "    ";

pub fn create_lib(servos: &[Actuator]) -> Result<()> {
    // Map of series -> model -> data names -> address
    // Should switch model and data names for improved code readability
    let mut addresses: BTreeMap<String, BTreeMap<String, BTreeMap<String, u16>>> = BTreeMap::new();

    // Keep track of all data names to convert into an enum later
    let mut data_names: Vec<String> = vec![];

    for dxl in servos {
        let series = dxl.series.to_uppercase();
        // let model = dxl.raw_name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_uppercase();
        let first_letter = dxl
            .raw_name
            .chars()
            .position(|x| x.is_alphabetic())
            .unwrap();
        let model = dxl
            .raw_name
            .chars()
            .filter(|x| x.is_alphanumeric())
            .skip(first_letter)
            .collect::<String>()
            .to_uppercase();

        let models = addresses.entry(series).or_insert_with(BTreeMap::new);

        for row in &dxl.data {
            if let Some(name) = &row.data_name {
                let pascal_name: String = name
                    .to_string()
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .collect();
                data_names.push(pascal_name.clone());

                let names = models.entry(model.clone()).or_insert_with(BTreeMap::new);
                names.insert(pascal_name, row.address);
            }
        }
    }

    data_names.sort();
    data_names.dedup();

    // Create the necessary file structure
    create_dir_all("lib/src")?;
    let mut lib = String::new();
    let mut cargo = String::new();

    cargo.push_str(CARGO_PREAMBLE);
    cargo.push_str(&format!(
        "default = [{}]",
        addresses
            .keys()
            .map(|x| format!("\"{}\"", x))
            .collect::<Vec<String>>()
            .join(", ")
    ));

    // Set up error handling
    lib.push_str(ERROR_DEFINITION);

    // DataName enum
    lib.push_str(DERIVES);
    lib.push_str("\npub enum DataName {\n    ");
    lib.push_str(&data_names.join(",\n    "));
    lib.push_str(",\n}\n\n");

    // Model enum
    lib.push_str(DERIVES);
    lib.push_str("\npub enum Model {\n");

    for (series, models) in &addresses {
        for model in models.keys() {
            lib.push_str(&format!("{}#[cfg(feature = \"{}\")]\n", INDENT, series));
            lib.push_str(&format!("{}{},\n", INDENT, model));
        }
    }
    lib.push_str("}\n");

    lib.push_str(
        "\npub const fn address(model: Model, name: DataName) -> Result<u16, ControlTableError> {",
    );
    lib.push_str(&format!("\n{}match model {{", INDENT));

    for (series, models) in &addresses {
        cargo.push_str(&format!("\n{} = []", series));
        for (model, data_names) in models {
            lib.push_str(&format!(
                "\n{}#[cfg(feature = \"{}\")]",
                INDENT.repeat(2),
                series
            ));
            lib.push_str(&format!(
                "\n{}Model::{} => match name {{",
                INDENT.repeat(2),
                model
            ));

            // Sort the addresses lowest-first
            let mut sorted_names = Vec::from_iter(data_names);
            sorted_names.sort_by(|&(_, b), &(_, a)| b.cmp(a));

            for (data_name, address) in sorted_names {
                lib.push_str(&format!(
                    "\n{}DataName::{} => Ok({}),",
                    INDENT.repeat(3),
                    data_name,
                    address
                ));
            }

            // Add error handling
            lib.push_str(&format!(
                "\n{}_ => Err(ControlTableError::NoMatchingAddress {{ model, name }}),",
                INDENT.repeat(3)
            ));
            lib.push_str(&format!("\n{}}},", INDENT.repeat(2)))
        }
    }

    lib.push_str(&format!("\n{}}}", INDENT));
    lib.push_str("\n}\n");
    cargo.push('\n');

    File::create("lib/src/lib.rs")?.write_all(lib.as_bytes())?;
    File::create("lib/Cargo.toml")?.write_all(cargo.as_bytes())?;

    Ok(())
}
