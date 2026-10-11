#![doc = include_str!("../README.md")]
//! External table parsing (Excel/CSV) for rcad
//!
//! This crate provides functionality to read and parse external table files
//! (.xlsx, .csv) for use in parameterizing CAD models.
//!
//! It uses the `calamine` crate for Excel file parsing and built-in CSV support.

use anyhow::{Context, Result};
use calamine::{Data, Reader, Xlsx, open_workbook};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Represents a cell value from a table
#[derive(Debug, Clone)]
pub enum CellValue {
    String(String),
    Float(f64),
    Int(i64),
    Bool(bool),
    DateTime(String),
    Empty,
}

/// Represents a row in a table
#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<CellValue>,
}

/// Represents a table with headers and data
#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<TableRow>,
}

impl Table {
    /// Get a cell value by column name and row index
    pub fn get(&self, column: &str, row: usize) -> Option<&CellValue> {
        let col_index = self.headers.iter().position(|h| h == column)?;
        self.rows.get(row)?.cells.get(col_index)
    }

    /// Get a string value by column name and row index
    pub fn get_string(&self, column: &str, row: usize) -> Option<String> {
        self.get(column, row).cloned().and_then(|val| match val {
            CellValue::String(s) => Some(s),
            CellValue::Float(f) => Some(f.to_string()),
            CellValue::Int(i) => Some(i.to_string()),
            CellValue::Bool(b) => Some(b.to_string()),
            CellValue::DateTime(dt) => Some(dt),
            CellValue::Empty => Some(String::new()),
        })
    }

    /// Get a float value by column name and row index
    pub fn get_float(&self, column: &str, row: usize) -> Option<f64> {
        self.get(column, row).cloned().and_then(|val| match val {
            CellValue::Float(f) => Some(f),
            CellValue::Int(i) => Some(i as f64),
            CellValue::String(s) => s.parse().ok(),
            _ => None,
        })
    }
}

/// Reads an Excel file and returns a Table
pub fn read_excel<P: AsRef<Path>>(path: P, sheet_name: Option<&str>) -> Result<Table> {
    let mut workbook: Xlsx<_> = open_workbook(path)?;
    let sheet_name = match sheet_name {
        Some(name) => name.to_string(),
        None => {
            let sheet_names = workbook.sheet_names();
            sheet_names
                .get(0)
                .map(|s| s.as_str().to_string())
                .unwrap_or_else(|| "Sheet1".to_string())
        }
    };

    let range = workbook
        .worksheet_range(&sheet_name)
        .with_context(|| format!("Cannot find worksheet '{}'", sheet_name))?;

    let mut headers = Vec::new();
    let mut rows = Vec::new();

    // Process rows
    for (row_idx, row) in range.rows().enumerate() {
        if row_idx == 0 {
            // First row is headers
            for cell in row {
                headers.push(match cell {
                    Data::String(s) => s.clone(),
                    Data::Float(f) => f.to_string(),
                    Data::Int(i) => i.to_string(),
                    Data::Bool(b) => b.to_string(),
                    Data::DateTime(dt) => dt.to_string(),
                    Data::DateTimeIso(s) => s.clone(),
                    Data::DurationIso(s) => s.clone(),
                    Data::Error(_) => String::new(),
                    Data::Empty => String::new(),
                });
            }
        } else {
            // Data rows
            let mut cells = Vec::new();
            for cell in row {
                let cell_value = match cell {
                    Data::String(s) => CellValue::String(s.clone()),
                    Data::Float(f) => CellValue::Float(*f),
                    Data::Int(i) => CellValue::Int(*i),
                    Data::Bool(b) => CellValue::Bool(*b),
                    Data::DateTime(dt) => CellValue::DateTime(dt.to_string()),
                    Data::DateTimeIso(s) => CellValue::DateTime(s.clone()),
                    Data::DurationIso(s) => CellValue::DateTime(s.clone()),
                    Data::Error(_) => CellValue::Empty,
                    Data::Empty => CellValue::Empty,
                };
                cells.push(cell_value);
            }
            rows.push(TableRow { cells });
        }
    }

    Ok(Table {
        name: sheet_name,
        headers,
        rows,
    })
}

/// Reads a CSV file and returns a Table
pub fn read_csv<P: AsRef<Path>>(path: P, has_headers: bool) -> Result<Table> {
    let path_ref = path.as_ref();
    let file = File::open(path_ref)?;
    let reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(reader);

    let mut headers = Vec::new();
    let mut rows = Vec::new();

    if has_headers {
        // Read the header
        let header_result = rdr.headers()?;
        for header in header_result.iter() {
            headers.push(header.to_string());
        }
        // Process the remaining records as data
        for result in rdr.records() {
            let record = result?;
            let mut cells = Vec::new();
            for field in record.iter() {
                // Try to parse as number first
                if let Ok(float_val) = field.parse::<f64>() {
                    if float_val.fract() == 0.0 {
                        cells.push(CellValue::Int(float_val as i64));
                    } else {
                        cells.push(CellValue::Float(float_val));
                    }
                } else if let Ok(int_val) = field.parse::<i64>() {
                    cells.push(CellValue::Int(int_val));
                } else if let Ok(bool_val) = field.parse::<bool>() {
                    cells.push(CellValue::Bool(bool_val));
                } else {
                    cells.push(CellValue::String(field.to_string()));
                }
            }
            rows.push(TableRow { cells });
        }
    } else {
        // No headers: we need to read all records and generate headers
        let mut records = Vec::new();
        for result in rdr.records() {
            records.push(result?);
        }
        if let Some(first_record) = records.first() {
            // Generate headers based on the number of columns in the first record
            for i in 0..first_record.len() {
                headers.push(format!("Column{}", i + 1));
            }
            // Process all records as data
            for record in records {
                let mut cells = Vec::new();
                for field in record.iter() {
                    // Try to parse as number first
                    if let Ok(float_val) = field.parse::<f64>() {
                        if float_val.fract() == 0.0 {
                            cells.push(CellValue::Int(float_val as i64));
                        } else {
                            cells.push(CellValue::Float(float_val));
                        }
                    } else if let Ok(int_val) = field.parse::<i64>() {
                        cells.push(CellValue::Int(int_val));
                    } else if let Ok(bool_val) = field.parse::<bool>() {
                        cells.push(CellValue::Bool(bool_val));
                    } else {
                        cells.push(CellValue::String(field.to_string()));
                    }
                }
                rows.push(TableRow { cells });
            }
        }
        // If there are no records, then headers and rows remain empty
    }

    Ok(Table {
        name: path_ref
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("table")
            .to_string(),
        headers,
        rows,
    })
}

/// Configuration for external table loading
#[derive(Debug, Clone, Deserialize)]
pub struct ExternalTableConfig {
    /// Path to the table file (relative to project root)
    pub path: String,
    /// Sheet name (for Excel files) or nil for first sheet
    pub sheet_name: Option<String>,
    /// Whether the file has headers (for CSV)
    pub has_headers: bool,
    /// Whether to cache the table in memory
    pub cache: bool,
}

/// Manager for external tables
#[derive(Default)]
pub struct ExternalTableManager {
    tables: HashMap<String, Table>,
    config: Option<ExternalTableConfig>,
}

impl ExternalTableManager {
    /// Create a new manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from rcad.toml
    pub fn load_config(&mut self) -> Result<()> {
        // In a real implementation, this would read from rcad.toml
        // For now, we'll use a default configuration
        self.config = Some(ExternalTableConfig {
            path: "tables/default.xlsx".to_string(),
            sheet_name: None,
            has_headers: true,
            cache: true,
        });
        Ok(())
    }

    /// Load a table based on configuration
    pub fn load_table(&mut self, name: &str) -> Result<&Table> {
        // Use entry API to avoid double borrow
        let config = self
            .config
            .as_ref()
            .expect("Configuration not loaded. Call load_config() first.");

        let table = if config.path.ends_with(".xlsx") || config.path.ends_with(".xls") {
            read_excel(&config.path, config.sheet_name.as_deref())?
        } else if config.path.ends_with(".csv") {
            read_csv(&config.path, config.has_headers)?
        } else {
            anyhow::bail!("Unsupported file format: {}", config.path);
        };

        // Insert or get the table
        Ok(self.tables.entry(name.to_string()).or_insert(table))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_table_creation() -> Result<()> {
        // Create a simple CSV for testing
        let mut file = File::create("test.csv")?;
        writeln!(file, "name,value,active")?;
        writeln!(file, "part1,10.5,true")?;
        writeln!(file, "part2,20.0,false")?;

        let table = read_csv("test.csv", true)?;
        assert_eq!(table.name, "test");
        assert_eq!(table.headers, vec!["name", "value", "active"]);
        assert_eq!(table.rows.len(), 2);

        // Check first row
        let name = table.get_string("name", 0).ok_or_else(|| anyhow::anyhow!("missing name"))?;
        assert_eq!(name, "part1");
        let value = table.get_float("value", 0).ok_or_else(|| anyhow::anyhow!("missing value"))?;
        assert_eq!(value, 10.5);
        let active = table.get_string("active", 0).ok_or_else(|| anyhow::anyhow!("missing active"))?;
        assert_eq!(active, "true");

        Ok(())
    }
}