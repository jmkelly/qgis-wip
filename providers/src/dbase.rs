//! Minimal DBF (dBase) header + field parsing.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct DbfField {
    pub name: String,
    pub field_type: char,
    pub length: u8,
    pub decimals: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbfHeader {
    pub record_count: u32,
    pub header_len: u16,
    pub record_len: u16,
    pub fields: Vec<DbfField>,
}

pub fn read_dbf_header(path: impl AsRef<Path>) -> Result<DbfHeader, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 32 { return Err("too short".into()); }
    let record_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let header_len = u16::from_le_bytes([data[8], data[9]]);
    let record_len = u16::from_le_bytes([data[10], data[11]]);
    let mut fields = Vec::new();
    let mut off = 32usize;
    while off + 32 <= data.len() && data[off] != 0x0D {
        let name_bytes = &data[off..off+11];
        let name = name_bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect::<String>().trim().to_string();
        let ft = data[off+11] as char;
        let length = data[off+16];
        let decimals = data[off+17];
        fields.push(DbfField { name, field_type: ft, length, decimals });
        off += 32;
    }
    Ok(DbfHeader { record_count, header_len, record_len, fields })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_minimal_header() {
        let mut buf = vec![0u8; 32 + 32 + 1];
        // version
        buf[0] = 0x03;
        // record count = 5
        buf[4..8].copy_from_slice(&5u32.to_le_bytes());
        // header len = 65
        buf[8..10].copy_from_slice(&65u16.to_le_bytes());
        // record len = 10
        buf[10..12].copy_from_slice(&10u16.to_le_bytes());
        // field descriptor at 32
        let name = b"NAME       ";
        buf[32..43].copy_from_slice(name);
        buf[43] = b'C';
        buf[48] = 10; // length
        buf[49] = 0;  // decimals
        // terminator
        buf[64] = 0x0D;
        let p = std::env::temp_dir().join("t.dbf");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(&buf).unwrap();
        let h = read_dbf_header(&p).unwrap();
        assert_eq!(h.record_count, 5);
        assert_eq!(h.fields.len(), 1);
        assert_eq!(h.fields[0].name, "NAME");
        assert_eq!(h.fields[0].field_type, 'C');
        assert_eq!(h.fields[0].length, 10);
    }
}
