//! Attribute table editors: table views, feature forms.

use std::collections::BTreeMap;

/// A single cell value in an attribute table.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl std::fmt::Display for CellValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellValue::Null => write!(f, ""),
            CellValue::Bool(b) => write!(f, "{}", b),
            CellValue::Int(v) => write!(f, "{}", v),
            CellValue::Float(v) => write!(f, "{}", v),
            CellValue::Text(s) => write!(f, "{}", s),
        }
    }
}

/// Column descriptor.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub editable: bool,
}

/// A row in the attribute table.
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub feature_id: i64,
    pub values: Vec<CellValue>,
}

/// In-memory attribute table.
#[derive(Debug, Clone)]
pub struct AttributeTable {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub filter: Option<String>,
    pub sort_column: Option<usize>,
    pub sort_ascending: bool,
}

impl AttributeTable {
    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            filter: None,
            sort_column: None,
            sort_ascending: true,
        }
    }

    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    pub fn visible_rows(&self) -> Vec<&Row> {
        let mut rows: Vec<&Row> = self.rows.iter().collect();
        if let Some(col) = self.sort_column {
            rows.sort_by(|a, b| {
                let va = a.values.get(col);
                let vb = b.values.get(col);
                let ord = cmp_cell(va, vb);
                if self.sort_ascending { ord } else { ord.reverse() }
            });
        }
        rows
    }

    pub fn set_cell(&mut self, row_idx: usize, col_idx: usize, value: CellValue) -> bool {
        if row_idx >= self.rows.len() || col_idx >= self.columns.len() {
            return false;
        }
        if !self.columns[col_idx].editable {
            return false;
        }
        if let Some(row) = self.rows.get_mut(row_idx) {
            if col_idx < row.values.len() {
                row.values[col_idx] = value;
                return true;
            }
        }
        false
    }
}

fn cmp_cell(a: Option<&CellValue>, b: Option<&CellValue>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => match (x, y) {
            (CellValue::Int(a), CellValue::Int(b)) => a.cmp(b),
            (CellValue::Float(a), CellValue::Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (CellValue::Text(a), CellValue::Text(b)) => a.cmp(b),
            (CellValue::Bool(a), CellValue::Bool(b)) => a.cmp(b),
            _ => Ordering::Equal,
        },
    }
}

/// Feature form field.
#[derive(Debug, Clone)]
pub struct FormField {
    pub name: String,
    pub value: CellValue,
    pub read_only: bool,
}

/// Feature form.
#[derive(Debug, Clone)]
pub struct FeatureForm {
    pub feature_id: i64,
    pub fields: Vec<FormField>,
}

impl FeatureForm {
    pub fn new(feature_id: i64) -> Self {
        Self { feature_id, fields: Vec::new() }
    }
    pub fn add_field(&mut self, field: FormField) {
        self.fields.push(field);
    }
    pub fn get(&self, name: &str) -> Option<&FormField> {
        self.fields.iter().find(|f| f.name == name)
    }
    pub fn set(&mut self, name: &str, value: CellValue) -> bool {
        if let Some(f) = self.fields.iter_mut().find(|f| f.name == name) {
            if f.read_only { return false; }
            f.value = value;
            return true;
        }
        false
    }
    pub fn to_map(&self) -> BTreeMap<String, CellValue> {
        self.fields.iter().map(|f| (f.name.clone(), f.value.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_set_cell_and_sort() {
        let mut t = AttributeTable::new(vec![
            Column { name: "id".into(), editable: false },
            Column { name: "name".into(), editable: true },
        ]);
        t.add_row(Row { feature_id: 1, values: vec![CellValue::Int(1), CellValue::Text("b".into())] });
        t.add_row(Row { feature_id: 2, values: vec![CellValue::Int(2), CellValue::Text("a".into())] });
        assert!(t.set_cell(0, 1, CellValue::Text("z".into())));
        assert!(!t.set_cell(0, 0, CellValue::Int(9))); // not editable
        t.sort_column = Some(1);
        t.sort_ascending = true;
        let vr = t.visible_rows();
        assert_eq!(vr[0].feature_id, 2);
    }
    #[test]
    fn form_set() {
        let mut f = FeatureForm::new(1);
        f.add_field(FormField { name: "name".into(), value: CellValue::Text("old".into()), read_only: false });
        assert!(f.set("name", CellValue::Text("new".into())));
        assert_eq!(f.get("name").unwrap().value, CellValue::Text("new".into()));
    }
}
