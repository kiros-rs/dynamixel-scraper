use crate::{Actuator, ControlTableData};
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
static CONTROL_TABLE_DATA: &str =
    "/// The levels of permission a user is granted in terms of an item in the
/// control table.
#[derive(Debug)]
pub enum AccessLevel {
    Read,
    ReadWrite,
}

/// An item that represents either the min, max, or initial value of a given address
#[derive(Debug)]
pub enum RangeValue {
    Integer(i32),
    Address { name: DataName, negative: bool },
}

/// A representation of an item in the control table, where only information
/// is stored. When applicable, items in the control table are represented in
/// this format, along with any optional data such as range or description.
#[derive(Debug)]
pub struct ControlTableData {
    pub address: u16,
    pub size: u8,
    pub description: Option<&'static str>,
    pub access: AccessLevel,
    pub initial_value: Option<RangeValue>,
    pub range: Option<(RangeValue, RangeValue)>,
}

";
static DERIVES: &str = "#[derive(Clone, Copy, Debug)]";
static INDENT: &str = "    ";

/// Append RangeValue:: to any variants of the enum
fn fix_formatting(text: String) -> String {
    text.replace("Read,", "AccessLevel::Read,")
        .replace("ReadWrite,", "AccessLevel::ReadWrite,")
}

pub fn create_lib(servos: &[Actuator]) -> Result<()> {
    // Map of series -> model -> data names -> control table data
    // Should switch model and data names for improved code readability
    let mut addresses: BTreeMap<String, BTreeMap<String, BTreeMap<String, ControlTableData>>> =
        BTreeMap::new();

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
                names.insert(pascal_name, row.to_owned());
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

    // Set up ControlTableData struct
    lib.push_str(CONTROL_TABLE_DATA);

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
        "\npub const fn data(model: Model, name: DataName) -> Result<ControlTableData, ControlTableError> {",
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
            sorted_names.sort_by(|&(_, b), &(_, a)| b.address.cmp(&a.address));

            for (data_name, data) in sorted_names {
                lib.push_str(&format!(
                    "\n{}DataName::{} => Ok(ControlTableData {{",
                    INDENT.repeat(3),
                    data_name
                ));
                lib.push_str(&fix_formatting(format!(
                    "\n{}address: {},",
                    INDENT.repeat(4),
                    data.address
                )));
                lib.push_str(&fix_formatting(format!(
                    "\n{}size: {},",
                    INDENT.repeat(4),
                    data.size
                )));
                lib.push_str(&fix_formatting(format!(
                    "\n{}description: {:?},",
                    INDENT.repeat(4),
                    data.description
                )));
                lib.push_str(&fix_formatting(format!(
                    "\n{}access: {:?},",
                    INDENT.repeat(4),
                    data.access
                )));
                lib.push_str(&format!(
                    "\n{}initial_value: {},",
                    INDENT.repeat(4),
                    match &data.initial_value {
                        Some(val) => format!("Some({})", val),
                        None => "None".to_string(),
                    }
                ));
                lib.push_str(&format!(
                    "\n{}range: {},",
                    INDENT.repeat(4),
                    match &data.range {
                        Some(val) => format!("Some(({}, {}))", val.0, val.1),
                        None => "None".to_string(),
                    }
                ));
                lib.push_str(&format!("\n{}}}),", INDENT.repeat(3)))
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
