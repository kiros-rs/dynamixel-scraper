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
    pub initial_value: Option<RangeValue>,
    pub range: Option<(RangeValue, RangeValue)>,
    pub units: Option<String>,
    // pub modbus: Option<ModbusAddress>, // Need to understand this better before implementation
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RangeValue {
    Integer(i32),
    Address { name: String, negative: bool },
}

impl RangeValue {
    pub fn new(text: &str) -> Result<RangeValue> {
        lazy_static! {
            // Regex to capture the address-based range values (eg "AccelerationLimit40")
            static ref ADDRESS_RE: Regex = Regex::new(r"^-?([a-zA-Z]+)[0-9]*$").unwrap();
            // Regex to capture the integer-based range values (eg 0)
            static ref INTEGER_RE: Regex = Regex::new(r"^-?[0-9]+$").unwrap();
        }

        let filtered_text = text.chars().filter(|c| *c != ',').collect::<String>();

        let address_matches = ADDRESS_RE.captures(&filtered_text);
        let integer_matches = INTEGER_RE.captures(&filtered_text);

        // Make sure only one regex matches
        assert!(address_matches.is_none() || integer_matches.is_none());
        assert!(address_matches.is_some() || integer_matches.is_some());

        if address_matches.is_some() {
            if let Some(captures) = address_matches {
                let mut captured_text = captures.get(0).unwrap().as_str().to_string();
                // Some ranges can be negative, eg -PWMLimit ~ PWMLimit
                let negative = captured_text.starts_with('-');
                // Filter out any extra chars (should be just numbers) "PWMLimit36" -> PWMLimit
                // This is done so that the names can be used with the DataName enum in the library (plus it looks better)
                captured_text = captured_text
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .collect();

                return Ok(RangeValue::Address {
                    name: captured_text,
                    negative,
                });
            }
        } else if let Some(captures) = integer_matches {
            let num = captures.get(0).unwrap().as_str();
            return Ok(RangeValue::Integer(num.parse::<i32>()?));
        };

        panic!("This should definitely not be possible");
    }
}

pub fn parse_servo(servo: Vec<Vec<String>>) -> Result<Vec<ControlTableData>> {
    let mut lines: Vec<Vec<Option<&str>>> = Vec::new();
    let bad_chars: Vec<char> = vec!['.', '-', ' ', '…', '~', '\u{a0}'];

    let mut lowest_address: Option<u32> = None;
    let mut highest_address: Option<u32> = None;
    // Regex to capture the data names that need extra processing
    lazy_static! {
        static ref INDERECT_RE: Regex =
            Regex::new(r"Indirect (?:Address|Data) (?:N|[0-9]*)").unwrap();
    }

    for line in servo.iter().skip(1) {
        let mut line_to_add: Vec<Option<&str>> = vec![];
        for col in line {
            if col.chars().all(|c| bad_chars.contains(&c)) {
                line_to_add.push(None);
            } else {
                line_to_add.push(Some(col));
            }
        }

        if !line_to_add.iter().all(|o| o.is_none()) {
            if let Some(captures) =
                INDERECT_RE.captures(&line.clone().into_iter().collect::<String>())
            {
                let current_match = captures.get(captures.len() - 1).unwrap().as_str();
                if !current_match.chars().all(char::is_numeric) {
                    continue;
                }

                let current_value = current_match.parse::<u32>()?;

                if lowest_address.unwrap_or(u32::MAX) > current_value {
                    lowest_address = Some(current_value);
                }

                if highest_address.unwrap_or(u32::MIN) < current_value {
                    highest_address = Some(current_value);
                }
            } else {
                lines.push(line_to_add);
            }
        }
    }

    let mut indexes: HashMap<&str, usize> = HashMap::new();
    for (idx, heading) in servo[0].iter().enumerate() {
        indexes.insert(heading, idx);
    }

    let mut data: Vec<ControlTableData> = Vec::new();
    for line in lines {
        let range: Option<(RangeValue, RangeValue)> =
            if let Some(text) = try_find(&indexes, &line, "Range") {
                if text.matches('~').count() == 1 {
                    assert_eq!(text.matches('~').count(), 1);
                    let mut text_parts = text.split('~').map(|s| {
                        s.chars()
                            .filter(|c| c.is_alphanumeric() || *c == '-')
                            .collect::<String>()
                    });

                    let min = RangeValue::new(&text_parts.next().unwrap())?;
                    let max = RangeValue::new(&text_parts.next().unwrap())?;

                    Some((min, max))
                } else {
                    // Need to fix these edge cases
                    None
                }
            } else if let Some(min_text) = try_find(&indexes, &line, "Min") {
                if let Some(max_text) = try_find(&indexes, &line, "Max") {
                    let min = RangeValue::new(&min_text)?;
                    let max = RangeValue::new(&max_text)?;

                    Some((min, max))
                } else {
                    None
                }
            } else {
                None
            };

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
                "RW" => AccessLevel::ReadWrite,
                "R/RW" => AccessLevel::ReadWrite, // Needs further research
                e => panic!("Unknown level: {}", e),
            },
            initial_value: match try_find(&indexes, &line, "Initial Value") {
                Some(val) => Some(RangeValue::new(
                    &val.chars().filter(|c| *c != ' ').collect::<String>(),
                )?),
                None => None,
            },
            range,
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
