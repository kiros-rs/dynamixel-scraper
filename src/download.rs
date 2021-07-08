use anyhow::Result;
use convert_case::{Case, Casing};
use scraper::{ElementRef, Html, Selector};

fn parse_table(table: ElementRef) -> Result<Vec<Vec<String>>> {
    lazy_static! {
        static ref ROW_SELECTOR: Selector = Selector::parse("tr>*").unwrap();
    };

    let elements = table.select(&ROW_SELECTOR);

    let (headings, body): (Vec<ElementRef>, Vec<ElementRef>) =
        elements.partition(|x| x.value().name() == "th");

    let mut parsed_table: Vec<Vec<String>> = vec![vec![]];
    for item in &headings {
        let text = item.text().collect::<String>();
        parsed_table[0].push(text.to_case(Case::Title));
    }

    for element in body {
        let text = element.text().collect::<String>();

        if text.is_empty() {
            continue;
        }

        if parsed_table.last().unwrap().len() == headings.len() {
            parsed_table.push(vec![]);
        }

        parsed_table.last_mut().unwrap().push(text);
    }

    Ok(parsed_table)
}

pub fn merge_tables(page: &str, indexes: (usize, usize)) -> Result<Vec<Vec<String>>> {
    let document = Html::parse_document(page);

    lazy_static! {
        static ref TABLE_SELECTOR: Selector = Selector::parse("table").unwrap();
    }
    let eeprom_table = document.select(&TABLE_SELECTOR).nth(indexes.0).unwrap();
    let ram_table = document.select(&TABLE_SELECTOR).nth(indexes.1).unwrap();

    let mut eeprom = parse_table(eeprom_table)?;
    let ram = parse_table(ram_table)?;

    // Make sure the headings are equal before combining
    assert_eq!(eeprom[0], ram[0]);
    eeprom.extend(ram.into_iter().skip(1));

    Ok(eeprom)
}
