use anyhow::Result;
use regex::Regex;
use ron::ser::{to_string_pretty, PrettyConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn try_find(
    indexes: &HashMap<&str, usize>,
    line: &[Option<&str>],
    heading: &str,
) -> Option<String> {
    if indexes.contains_key(&heading) {
        let item = line[indexes[heading]];
        if let Some(i) = item {
            return Some(i.to_string());
        }
    }

    None
}

/// The levels of permission a user is granted in terms of an item in the
/// control table.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum AccessLevel {
    Read,
    Write,
    ReadWrite,
}

/// A representation of an item in the control table, where only information
/// is stored. When applicable, items in the control table are represented in
/// this format, along with any optional data such as range or description.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ControlTableData {
    pub address: u16,
    pub size: u8,
    pub data_name: Option<String>,
    pub description: Option<String>,
    pub access: AccessLevel,
    pub initial_value: Option<String>,
    pub range: Option<std::ops::Range<i64>>,
    pub units: Option<String>,
    // pub modbus: Option<ModbusAddress>, // Need to understand this better before implementation
}

pub fn parse_servo(servo: &str) -> Result<Vec<ControlTableData>> {
    let mut lines: Vec<Vec<Option<&str>>> = Vec::new();
    let bad_chars: Vec<char> = vec!['.', '-', ' ', '…', '~', '\u{a0}'];

    let mut lowest_address: Option<u64> = None;
    let mut highest_address: Option<u64> = None;
    let re = Regex::new("Indirect (?:Address|Data) (N|[0-9]*)")?;

    for line in servo.lines().skip(1) {
        let cols = line.split(", ");
        let mut line_to_add: Vec<Option<&str>> = vec![];
        for col in cols {
            if col.chars().all(|c| bad_chars.contains(&c)) {
                line_to_add.push(None);
            } else {
                line_to_add.push(Some(col));
            }
        }

        if !line_to_add.iter().all(|o| o.is_none()) {
            let caps = re.captures(line);

            if let Some(captures) = caps {
                let current_match = captures.get(captures.len() - 1).unwrap().as_str();
                if !current_match.chars().all(char::is_numeric) {
                    continue;
                }

                let current_value = current_match.parse::<u64>()?;

                if lowest_address.unwrap_or(u64::MAX) > current_value {
                    lowest_address = Some(current_value);
                }

                if highest_address.unwrap_or(u64::MIN) < current_value {
                    highest_address = Some(current_value);
                }
            } else {
                lines.push(line_to_add);
            }
        }
    }

    let headings: Vec<&str> = servo.lines().next().unwrap().split(", ").collect();
    let mut indexes: HashMap<&str, usize> = HashMap::new();
    for (idx, heading) in headings.iter().enumerate() {
        indexes.insert(heading, idx);
    }

    let mut data: Vec<ControlTableData> = Vec::new();
    for line in lines {
        data.push(ControlTableData {
            address: line[*indexes.get("Address").unwrap()]
                .unwrap()
                .parse::<u16>()?,
            size: line[*indexes.get("Size(byte)").unwrap()]
                .unwrap()
                .parse::<u8>()?, // NOTE: There should be a space inserted in front of applicable headings such as "Size(Byte)"
            data_name: try_find(&indexes, &line, "Data Name"),
            description: try_find(&indexes, &line, "Description"),
            access: match line[*indexes.get("Access").unwrap()].unwrap() {
                "R" => AccessLevel::Read,
                "W" => AccessLevel::Write,
                "RW" => AccessLevel::ReadWrite,
                "R/RW" => AccessLevel::ReadWrite, // Needs further research
                e => panic!("Unknown level: {}", e),
            },
            initial_value: try_find(&indexes, &line, "Initial Value"), // Needs more work
            // These will need more research before implementation
            range: None,
            units: None,
        });
    }

    Ok(data)
}

pub fn serialize_servo(servo: &[ControlTableData]) -> Result<String> {
    let pretty = PrettyConfig::new()
        .with_separate_tuple_members(true)
        .with_enumerate_arrays(true);
    let s = to_string_pretty(&servo, pretty)?;

    Ok(s)
}
