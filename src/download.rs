use anyhow::Result;
use convert_case::{Case, Casing};
use scraper::{ElementRef, Html, Selector};

fn parse_table(table: ElementRef) -> Result<String> {
    let mut csv = String::new();

    lazy_static! {
        static ref ROW_SELECTOR: Selector = Selector::parse("tr>*").unwrap();
    };

    let elements = table.select(&ROW_SELECTOR);

    let (headings, body): (Vec<ElementRef>, Vec<ElementRef>) =
        elements.partition(|x| x.value().name() == "th");
    let mut items_in_line = 0;

    for item in &headings {
        let text = item.text().collect::<String>();
        if !csv.is_empty() {
            csv.push_str(", ");
        }

        csv.push_str(&text.to_case(Case::Title));
    }

    csv.push('\n');

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

        if items_in_line == headings.len() {
            line.push('\n');
            items_in_line = 0;
        }

        csv.push_str(&line);
    }

    Ok(csv)
}

pub fn merge_tables(page: &str, indexes: (usize, usize)) -> Result<String> {
    let document = Html::parse_document(page);

    lazy_static! {
        static ref TABLE_SELECTOR: Selector = Selector::parse("table").unwrap();
    }
    let eeprom_table = document.select(&TABLE_SELECTOR).nth(indexes.0).unwrap();
    let ram_table = document.select(&TABLE_SELECTOR).nth(indexes.1).unwrap();

    let mut eeprom = parse_table(eeprom_table)?;
    let ram = parse_table(ram_table)?;

    // Make sure the headings are equal before combining
    assert_eq!(eeprom.lines().next(), ram.lines().next());
    eeprom.push_str(&ram.lines().skip(1).collect::<Vec<_>>().join("\n"));

    Ok(eeprom)
}
