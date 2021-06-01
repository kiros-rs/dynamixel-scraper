use anyhow::Result;
use movement::dynamixel::{AccessLevel, ControlTableData};
use regex::Regex;
use ron::ser::{to_string_pretty, PrettyConfig};
use std::collections::HashMap;

fn try_find(
    indexes: &HashMap<&str, usize>,
    line: &Vec<Option<&str>>,
    heading: &str,
) -> Option<String> {
    if indexes.contains_key(&heading) {
        let item = line[indexes[heading]];
        if item.is_some() {
            return Some(item.unwrap().to_string());
        }
    }

    None
}

pub fn serialize_servo(servo: &str) -> Result<String> {
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
            let caps = re.captures(&line);

            if caps.is_none() {
                lines.push(line_to_add);
            } else {
                // println!("Captured line: {:?}", line_to_add);
                let caps = caps.unwrap();
                let current_match = caps.get(caps.len() - 1).unwrap().as_str();
                if !current_match.chars().all(char::is_numeric) {
                    // println!("Discarded {:?}", current_match);
                    continue;
                }

                let current_value = current_match.parse::<u64>()?;

                if lowest_address.unwrap_or(u64::MAX) > current_value {
                    lowest_address = Some(current_value);
                }

                if highest_address.unwrap_or(u64::MIN) < current_value {
                    highest_address = Some(current_value);
                }
            }
        }
    }

    // println!("Highest: {}", highest_address.unwrap_or(0));
    // println!("Lowest: {}", lowest_address.unwrap_or(0));

    let headings: Vec<&str> = servo.lines().next().unwrap().split(", ").collect();
    let mut indexes: HashMap<&str, usize> = HashMap::new();
    for (idx, heading) in headings.iter().enumerate() {
        indexes.insert(heading, idx);
    }

    let mut data: Vec<ControlTableData<u64>> = Vec::new();
    for line in lines {
        // println!("{:?} {}", line, *indexes.get("Address").unwrap());
        data.push(ControlTableData {
            address: line[*indexes.get("Address").unwrap()]
                .unwrap()
                .parse::<u64>()?,
            size: line[*indexes.get("Size(byte)").unwrap()]
                .unwrap()
                .parse::<u64>()?, // NOTE: There should be a space inserted in front of applicable headings such as "Size(Byte)"
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
            modbus: None,
        });
    }

    // for line in data {
    //     println!("{:?}", line);
    // }

    let pretty = PrettyConfig::new()
        .with_separate_tuple_members(true)
        .with_enumerate_arrays(true);
    let s = to_string_pretty(&data, pretty)?;
    // let s = to_string(&data)?;
    Ok(s)
}
