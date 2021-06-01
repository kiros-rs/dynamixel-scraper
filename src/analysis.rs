use prettytable::{Table, Row, Cell};
use std::collections::HashMap;

pub struct FileAnalysis {

}

pub struct GroupAnalysis {
    pub files_analysed: HashMap<String, FileAnalysis>,
    pub recurring_cols: Vec<String>,
    pub unique_cols: Vec<String>,
}

fn analyse_file(contents: &str) -> Vec<Vec<&str>> {
    vec![vec![]]
}

pub fn display_analysis(contents: &str) {
    let mut rows: Vec<Vec<&str>> = Vec::new();

    for row in contents.split('\n') {
        let mut current_row: Vec<&str> = Vec::new();
        for col in row.split(", ") {
            current_row.push(col);
        }

        rows.push(current_row);
    }

    let mut table = Table::new();

    for row in rows {
        table.add_row(Row::new(row.iter().map(|x| Cell::new(x)).collect()));
    }

    table.printstd();
}
