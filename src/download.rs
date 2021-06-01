use convert_case::{Case, Casing};
use scraper::{ElementRef, Html, Selector};
use anyhow::Result;

fn parse_table(table: ElementRef) -> Result<String> {
    let mut csv = String::new();

    let heading_selector = Selector::parse("thead>tr>th").unwrap();
    let body_selector = Selector::parse("tbody>tr>td").unwrap();

    let headings: Vec<_> = table.select(&heading_selector).collect();
    let mut num_headings = 0;
    let mut items_in_line = 0;

    for item in headings {
        let text = String::from(item.text().collect::<String>());
        if csv.len() > 0 {
            csv.push_str(", ");
        }

        csv.push_str(&text.to_case(Case::Title));
        num_headings += 1;
    }

    csv.push('\n');
    let body = table.select(&body_selector);

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

        if items_in_line == num_headings {
            line.push('\n');
            items_in_line = 0;
        }

        csv.push_str(&line);
    }

    Ok(csv)
}

pub fn merge_tables(page: &str, indexes: (usize, usize)) -> Result<String> {
    let document = Html::parse_document(page);

    let table_selector = Selector::parse("table").unwrap();
    let eeprom_table = document.select(&table_selector).nth(indexes.0).unwrap();
    let ram_table = document.select(&table_selector).nth(indexes.1).unwrap();

    let mut eeprom = parse_table(eeprom_table)?;
    let ram = parse_table(ram_table)?;

    // Make sure the headings are equal before combining
    assert_eq!(eeprom.lines().next(), ram.lines().next());
    eeprom.push_str(&ram.lines().skip(1).collect::<Vec<_>>().join("\n"));

    Ok(eeprom)
}
