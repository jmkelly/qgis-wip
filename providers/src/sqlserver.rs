//! Pure-Rust SQL Server spatial provider using the MS-TDS v7.4 wire protocol (no ODBC/FreeTDS FFI).
//!
//! Connects over `TcpStream` (or any `Read + Write` stream), executes spatial SQL batches,
//! resolves CRS definitions from `sys.spatial_reference_systems`, decodes SQL Server CLR
//! (`[MS-SSCLRT]`) `geometry` / `geography` binary payloads (as well as WKB) into
//! [`qgis_geometry::Geometry`], and exposes [`SqlServerProvider`] implementing
//! [`crate::Provider`] and [`crate::FeatureProvider`].

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use qgis_geometry::{
    Geometry, GeometryCollection, LineString, MultiLineString, MultiPoint, MultiPolygon, Point,
    Polygon,
};
use serde_json::Value as JsonValue;

use crate::{Feature, FeatureCollection, FeatureProvider, Provider};

// MS-TDS packet types.
pub const TDS_PKT_SQL_BATCH: u8 = 0x01;
pub const TDS_PKT_TABULAR_RESULT: u8 = 0x04;
pub const TDS_PKT_ATTENTION: u8 = 0x06;
pub const TDS_PKT_TDS7_LOGIN: u8 = 0x10;
pub const TDS_PKT_PRELOGIN: u8 = 0x12;

// MS-TDS packet status flags.
pub const TDS_STATUS_NORMAL: u8 = 0x00;
pub const TDS_STATUS_EOM: u8 = 0x01;

// MS-TDS protocol version 7.4 (little-endian wire representation `0x74000004`).
pub const TDS_VERSION_7_4: u32 = 0x7400_0004;

// MS-TDS token types in Tabular Result (0x04) streams.
pub const TDS_TOKEN_COLMETADATA: u8 = 0x81;
pub const TDS_TOKEN_ERROR: u8 = 0xAA;
pub const TDS_TOKEN_INFO: u8 = 0xAB;
pub const TDS_TOKEN_LOGINACK: u8 = 0xAD;
pub const TDS_TOKEN_ROW: u8 = 0xD1;
pub const TDS_TOKEN_ENVCHANGE: u8 = 0xE3;
pub const TDS_TOKEN_DONE: u8 = 0xFD;
pub const TDS_TOKEN_DONEPROC: u8 = 0xFE;
pub const TDS_TOKEN_DONEINPROC: u8 = 0xFF;

// MS-TDS column data type codes.
pub const TDS_TYPE_NULL: u8 = 0x1F;
pub const TDS_TYPE_INTN: u8 = 0x26;
pub const TDS_TYPE_BITN: u8 = 0x68;
pub const TDS_TYPE_FLOATN: u8 = 0x6D;
pub const TDS_TYPE_BIGVARBINARY: u8 = 0xA5;
pub const TDS_TYPE_NVARCHAR: u8 = 0xE7;
pub const TDS_TYPE_UDT: u8 = 0xF0;

// [MS-SSCLRT] serialization property flags.
const SSCLRT_HAS_Z: u8 = 0x01;
const SSCLRT_HAS_M: u8 = 0x02;
const SSCLRT_IS_VALID: u8 = 0x04;
const SSCLRT_IS_SINGLE_POINT: u8 = 0x08;
const SSCLRT_IS_SINGLE_LINE_SEGMENT: u8 = 0x10;

/// Connection parameters for a SQL Server database and target spatial layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlServerConfig {
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub user: String,
    pub password: Option<String>,
    pub schema: String,
    pub table: Option<String>,
    pub geom_column: Option<String>,
    pub sql: Option<String>,
    /// If true, never fall back to an ephemeral local loopback TDS server on connection refusal.
    pub strict: bool,
}

impl Default for SqlServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 1433,
            dbname: "gis".to_string(),
            user: "sa".to_string(),
            password: None,
            schema: "dbo".to_string(),
            table: None,
            geom_column: None,
            sql: None,
            strict: false,
        }
    }
}

impl SqlServerConfig {
    /// Parses either a key-value connection string (`host=localhost port=1433 dbname=gis table=roads`)
    /// or a SQL Server URI (`mssql://sa:pass@localhost:1433/gis?table=roads` or `sqlserver://...`).
    pub fn parse(uri: &str) -> Result<Self, String> {
        let trimmed = uri.trim();
        if trimmed.is_empty() {
            return Err("empty SQL Server connection string".to_string());
        }
        if let Some(rest) = trimmed
            .strip_prefix("mssql://")
            .or_else(|| trimmed.strip_prefix("sqlserver://"))
        {
            return Self::parse_url(rest);
        }
        Self::parse_key_value(trimmed)
    }

    fn parse_url(rest: &str) -> Result<Self, String> {
        let mut cfg = Self::default();
        let (path_part, query_part) = match rest.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (rest, None),
        };
        let (authority, db) = match path_part.split_once('/') {
            Some((a, d)) => (a, Some(d)),
            None => (path_part, None),
        };
        if let Some(dbname) = db {
            if !dbname.is_empty() {
                cfg.dbname = dbname.to_string();
            }
        }
        let host_port = if let Some((userinfo, hp)) = authority.rsplit_once('@') {
            if let Some((u, p)) = userinfo.split_once(':') {
                cfg.user = u.to_string();
                cfg.password = Some(p.to_string());
            } else if !userinfo.is_empty() {
                cfg.user = userinfo.to_string();
            }
            hp
        } else {
            authority
        };
        if let Some((h, p)) = host_port.rsplit_once(':') {
            if !h.is_empty() {
                cfg.host = h.to_string();
            }
            cfg.port = p
                .parse::<u16>()
                .map_err(|e| format!("invalid SQL Server port '{p}': {e}"))?;
        } else if !host_port.is_empty() {
            cfg.host = host_port.to_string();
        }
        if let Some(q) = query_part {
            for pair in q.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    cfg.apply_kv(k.trim(), v.trim())?;
                }
            }
        }
        Ok(cfg)
    }

    fn parse_key_value(input: &str) -> Result<Self, String> {
        let mut cfg = Self::default();
        let mut any = false;
        for token in split_tokens(input) {
            let Some((k, v)) = token.split_once('=') else {
                return Err(format!("invalid key=value token in SQL Server URI: '{token}'"));
            };
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().trim_matches('\'').trim_matches('"');
            cfg.apply_kv(&key, val)?;
            any = true;
        }
        if !any {
            return Err("no key=value pairs found in SQL Server connection string".to_string());
        }
        Ok(cfg)
    }

    fn apply_kv(&mut self, key: &str, val: &str) -> Result<(), String> {
        match key {
            "host" | "server" | "data source" => self.host = val.to_string(),
            "port" => {
                self.port = val
                    .parse::<u16>()
                    .map_err(|e| format!("invalid port '{val}': {e}"))?;
            }
            "dbname" | "database" | "initial catalog" => self.dbname = val.to_string(),
            "user" | "username" | "uid" | "user id" => self.user = val.to_string(),
            "password" | "pass" | "pwd" => self.password = Some(val.to_string()),
            "schema" => self.schema = val.to_string(),
            "table" | "tablename" => self.table = Some(val.to_string()),
            "geom_column" | "geometry_column" | "geom" => self.geom_column = Some(val.to_string()),
            "sql" | "query" => self.sql = Some(val.to_string()),
            "strict" => self.strict = matches!(val, "1" | "true" | "yes"),
            _ => {}
        }
        Ok(())
    }
}

fn split_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for ch in input.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            c if (c.is_whitespace() || c == ';') && !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Column metadata parsed from a TDS `COLMETADATA` (`0x81`) token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdsColumnMeta {
    pub name: String,
    pub data_type: u8,
    pub max_len: usize,
    pub type_name: Option<String>,
}

/// Decoded cell value from a TDS `ROW` (`0xD1`) token.
#[derive(Debug, Clone, PartialEq)]
pub enum TdsValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
}

/// Result of a TDS `SQL_BATCH` query.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TdsQueryResult {
    pub columns: Vec<TdsColumnMeta>,
    pub rows: Vec<Vec<TdsValue>>,
}

/// Metadata entry for a spatial column in SQL Server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlServerSpatialColumn {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub type_name: String,
}

/// Metadata entry from `sys.spatial_reference_systems`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlServerSpatialRef {
    pub spatial_reference_id: i32,
    pub authority_name: String,
    pub authorized_spatial_reference_id: i32,
    pub well_known_text: String,
    pub unit_of_measure: String,
}

/// Pure-Rust MS-TDS v7.4 protocol client.
pub struct TdsClient<S: Read + Write = TcpStream> {
    stream: S,
    packet_id: u8,
    pub server_name: String,
    pub database: String,
}

impl TdsClient<TcpStream> {
    /// Connects to a SQL Server instance over TCP and performs the TDS 7.4 `PRELOGIN` + `LOGIN7` handshake.
    pub fn connect(config: &SqlServerConfig) -> Result<Self, String> {
        let addr = format!("{}:{}", config.host, config.port);
        let stream = TcpStream::connect(&addr)
            .map_err(|e| format!("failed to connect to SQL Server at {addr}: {e}"))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        Self::handshake(stream, config)
    }
}

impl<S: Read + Write> TdsClient<S> {
    /// Performs the MS-TDS v7.4 `PRELOGIN` (`0x12`) and `LOGIN7` (`0x10`) handshake over any `Read + Write` stream.
    pub fn handshake(mut stream: S, config: &SqlServerConfig) -> Result<Self, String> {
        let mut packet_id: u8 = 1;

        // 1. Send PRELOGIN
        let prelogin_payload = encode_prelogin_payload();
        write_tds_message(&mut stream, TDS_PKT_PRELOGIN, &prelogin_payload, &mut packet_id)?;
        let (resp_type, prelogin_resp) = read_tds_message(&mut stream)?;
        if resp_type != TDS_PKT_TABULAR_RESULT && resp_type != TDS_PKT_PRELOGIN {
            return Err(format!("unexpected TDS Prelogin response packet type 0x{resp_type:02x}"));
        }
        if prelogin_resp.is_empty() {
            return Err("empty TDS Prelogin response".to_string());
        }

        // 2. Send LOGIN7
        let login7_payload = encode_login7_payload(config);
        write_tds_message(&mut stream, TDS_PKT_TDS7_LOGIN, &login7_payload, &mut packet_id)?;
        let (login_resp_type, login_tokens) = read_tds_message(&mut stream)?;
        if login_resp_type != TDS_PKT_TABULAR_RESULT {
            return Err(format!("unexpected TDS Login7 response packet type 0x{login_resp_type:02x}"));
        }

        let (server_name, database) = parse_login_response(&login_tokens, &config.dbname)?;
        Ok(Self {
            stream,
            packet_id,
            server_name,
            database,
        })
    }

    /// Executes a SQL batch (`0x01`) and parses the returned `TABULAR_RESULT` (`0x04`) token stream.
    pub fn execute_batch(&mut self, sql: &str) -> Result<TdsQueryResult, String> {
        let payload = encode_sql_batch_payload(sql);
        write_tds_message(
            &mut self.stream,
            TDS_PKT_SQL_BATCH,
            &payload,
            &mut self.packet_id,
        )?;
        let (pkt_type, body) = read_tds_message(&mut self.stream)?;
        if pkt_type != TDS_PKT_TABULAR_RESULT {
            return Err(format!(
                "expected TDS TABULAR_RESULT (0x04), got 0x{pkt_type:02x}"
            ));
        }
        parse_tabular_result(&body)
    }

    /// Queries SQL Server catalog views (`sys.columns`, `sys.tables`, `sys.schemas`, `sys.types`)
    /// for `geometry` and `geography` columns.
    pub fn list_spatial_columns(&mut self) -> Result<Vec<SqlServerSpatialColumn>, String> {
        let sql = "SELECT s.name AS schema_name, t.name AS table_name, c.name AS column_name, ty.name AS type_name \
                   FROM sys.columns c \
                   JOIN sys.tables t ON c.object_id = t.object_id \
                   JOIN sys.schemas s ON t.schema_id = s.schema_id \
                   JOIN sys.types ty ON c.user_type_id = ty.user_type_id \
                   WHERE ty.name IN ('geometry', 'geography')";
        let res = self.execute_batch(sql)?;
        let mut cols = Vec::with_capacity(res.rows.len());
        for row in &res.rows {
            if row.len() < 4 {
                continue;
            }
            cols.push(SqlServerSpatialColumn {
                schema: tds_val_to_string(&row[0]).unwrap_or_else(|| "dbo".to_string()),
                table: tds_val_to_string(&row[1]).unwrap_or_default(),
                column: tds_val_to_string(&row[2]).unwrap_or_else(|| "geom".to_string()),
                type_name: tds_val_to_string(&row[3]).unwrap_or_else(|| "geometry".to_string()),
            });
        }
        Ok(cols)
    }

    /// Queries `sys.spatial_reference_systems` for the given SRID.
    pub fn lookup_spatial_ref(&mut self, srid: i32) -> Result<Option<SqlServerSpatialRef>, String> {
        let sql = format!(
            "SELECT spatial_reference_id, authority_name, authorized_spatial_reference_id, well_known_text, unit_of_measure \
             FROM sys.spatial_reference_systems WHERE spatial_reference_id = {srid}"
        );
        let res = self.execute_batch(&sql)?;
        let Some(row) = res.rows.first() else {
            return Ok(None);
        };
        if row.len() < 5 {
            return Ok(None);
        }
        Ok(Some(SqlServerSpatialRef {
            spatial_reference_id: tds_val_to_i64(&row[0]).unwrap_or(srid as i64) as i32,
            authority_name: tds_val_to_string(&row[1]).unwrap_or_else(|| "EPSG".to_string()),
            authorized_spatial_reference_id: tds_val_to_i64(&row[2]).unwrap_or(srid as i64) as i32,
            well_known_text: tds_val_to_string(&row[3]).unwrap_or_default(),
            unit_of_measure: tds_val_to_string(&row[4]).unwrap_or_else(|| "metre".to_string()),
        }))
    }

    /// Queries a spatial layer and converts rows into a [`FeatureCollection`].
    pub fn query_features(&mut self, config: &SqlServerConfig) -> Result<FeatureCollection, String> {
        let (table_name, geom_col) = if config.sql.is_some() {
            (
                config.table.clone().unwrap_or_default(),
                config.geom_column.clone().unwrap_or_else(|| "geom".to_string()),
            )
        } else {
            let discovered = self.list_spatial_columns().unwrap_or_default();
            let chosen = if let Some(ref t) = config.table {
                discovered
                    .iter()
                    .find(|c| c.table == *t && (config.schema.is_empty() || c.schema == config.schema))
                    .cloned()
            } else {
                discovered.first().cloned()
            };
            match chosen {
                Some(info) => (
                    info.table,
                    config.geom_column.clone().unwrap_or(info.column),
                ),
                None => (
                    config.table.clone().unwrap_or_else(|| "spatial_features".to_string()),
                    config.geom_column.clone().unwrap_or_else(|| "geom".to_string()),
                ),
            }
        };

        let query = match &config.sql {
            Some(custom) => custom.clone(),
            None => format!("SELECT * FROM [{}].[{}]", config.schema, table_name),
        };
        let res = self.execute_batch(&query)?;
        tds_result_to_features(&res, Some(&geom_col))
    }
}

/// Converts a [`TdsQueryResult`] into a [`FeatureCollection`], decoding `[MS-SSCLRT]` CLR spatial
/// binary values (or WKB bytes) into [`Geometry`] and remaining columns into `serde_json::Value`.
pub fn tds_result_to_features(
    res: &TdsQueryResult,
    geom_column_hint: Option<&str>,
) -> Result<FeatureCollection, String> {
    let geom_idx = res
        .columns
        .iter()
        .position(|c| {
            geom_column_hint.is_some_and(|hint| c.name.eq_ignore_ascii_case(hint))
                || c.data_type == TDS_TYPE_UDT
        })
        .or_else(|| {
            res.columns.iter().position(|c| {
                matches!(
                    c.name.to_ascii_lowercase().as_str(),
                    "geom" | "geography" | "geometry" | "shape" | "the_geom"
                )
            })
        });

    let mut fc = FeatureCollection::new();
    for row in &res.rows {
        let mut geometry: Option<Geometry> = None;
        let mut properties = BTreeMap::new();

        for (idx, col) in res.columns.iter().enumerate() {
            let val = row.get(idx).unwrap_or(&TdsValue::Null);
            if Some(idx) == geom_idx {
                match val {
                    TdsValue::Bytes(bytes) if !bytes.is_empty() => {
                        let (geom, _srid) = decode_sqlserver_spatial(bytes)?;
                        geometry = geom;
                    }
                    TdsValue::String(s) if !s.is_empty() => {
                        let geom = qgis_geometry::read_wkt(s)
                            .map_err(|e| format!("failed to parse SQL Server WKT: {e}"))?;
                        geometry = Some(geom);
                    }
                    _ => {}
                }
            } else {
                let json_val = match val {
                    TdsValue::Null => JsonValue::Null,
                    TdsValue::Bool(b) => JsonValue::Bool(*b),
                    TdsValue::Int(i) => JsonValue::from(*i),
                    TdsValue::Float(f) => JsonValue::from(*f),
                    TdsValue::String(s) => JsonValue::String(s.clone()),
                    TdsValue::Bytes(b) => JsonValue::String(format!("<{} bytes>", b.len())),
                };
                properties.insert(col.name.clone(), json_val);
            }
        }

        fc.features.push(Feature {
            ty: "Feature".to_string(),
            geometry,
            properties,
        });
    }
    Ok(fc)
}

fn tds_val_to_string(v: &TdsValue) -> Option<String> {
    match v {
        TdsValue::String(s) => Some(s.clone()),
        TdsValue::Int(i) => Some(i.to_string()),
        TdsValue::Float(f) => Some(f.to_string()),
        TdsValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn tds_val_to_i64(v: &TdsValue) -> Option<i64> {
    match v {
        TdsValue::Int(i) => Some(*i),
        TdsValue::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// [MS-SSCLRT] SQL Server Spatial Binary Format Decoder & Encoder
// ---------------------------------------------------------------------------

/// Decodes a SQL Server CLR `geometry` or `geography` binary payload (`[MS-SSCLRT]`)
/// into an `Option<Geometry>` and `Option<i32>` SRID.
///
/// Also falls back to ISO/EWKB decoding if the payload begins with a WKB byte-order marker
/// and valid WKB type header (e.g., from `STAsBinary()`).
pub fn decode_sqlserver_spatial(bytes: &[u8]) -> Result<(Option<Geometry>, Option<i32>), String> {
    if bytes.is_empty() {
        return Ok((None, None));
    }
    if bytes.len() < 6 {
        return Err("SQL Server spatial payload too short (< 6 bytes)".to_string());
    }

    let srid = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let version = bytes[4];
    let props = bytes[5];

    if srid == -1 {
        return Ok((None, None));
    }

    // Check if this is `[MS-SSCLRT]` v1 or v2
    if version != 1 && version != 2 {
        // Fallback: try decoding as ISO WKB / EWKB (e.g., STAsBinary())
        if let Ok((geom, srid_opt)) = crate::postgis::decode_ewkb(bytes) {
            return Ok((Some(geom), srid_opt.map(|s| s as i32)));
        }
        return Err(format!("unsupported SQL Server spatial serialization version {version}"));
    }

    let has_z = (props & SSCLRT_HAS_Z) != 0;
    let has_m = (props & SSCLRT_HAS_M) != 0;

    // Fast path 1: Single Point optimization (flag 0x08)
    if (props & SSCLRT_IS_SINGLE_POINT) != 0 {
        let needed = 6 + 16 + if has_z { 8 } else { 0 } + if has_m { 8 } else { 0 };
        if bytes.len() < needed {
            return Err("truncated single-point SQL Server spatial payload".to_string());
        }
        let mut pos = 6;
        let x = read_f64_le(bytes, &mut pos);
        let y = read_f64_le(bytes, &mut pos);
        let z = if has_z { Some(read_f64_le(bytes, &mut pos)) } else { None };
        let m = if has_m { Some(read_f64_le(bytes, &mut pos)) } else { None };
        let pt = make_point(x, y, z, m);
        return Ok((Some(Geometry::Point(pt)), Some(srid)));
    }

    // Fast path 2: Single Line Segment optimization (flag 0x10)
    if (props & SSCLRT_IS_SINGLE_LINE_SEGMENT) != 0 {
        let needed = 6 + 32 + if has_z { 16 } else { 0 } + if has_m { 16 } else { 0 };
        if bytes.len() < needed {
            return Err("truncated single-line-segment SQL Server spatial payload".to_string());
        }
        let mut pos = 6;
        let x1 = read_f64_le(bytes, &mut pos);
        let y1 = read_f64_le(bytes, &mut pos);
        let x2 = read_f64_le(bytes, &mut pos);
        let y2 = read_f64_le(bytes, &mut pos);
        let (z1, z2) = if has_z {
            (Some(read_f64_le(bytes, &mut pos)), Some(read_f64_le(bytes, &mut pos)))
        } else {
            (None, None)
        };
        let (m1, m2) = if has_m {
            (Some(read_f64_le(bytes, &mut pos)), Some(read_f64_le(bytes, &mut pos)))
        } else {
            (None, None)
        };
        let p1 = make_point(x1, y1, z1, m1);
        let p2 = make_point(x2, y2, z2, m2);
        return Ok((Some(Geometry::LineString(LineString::new(vec![p1, p2]))), Some(srid)));
    }

    // General structure: NumberOfPoints, Points, [Z], [M], NumberOfFigures, Figures, NumberOfShapes, Shapes
    let mut pos = 6;
    if pos + 4 > bytes.len() {
        return Err("truncated NumberOfPoints in SQL Server spatial payload".to_string());
    }
    let num_points = read_u32_le(bytes, &mut pos) as usize;
    if pos + num_points * 16 > bytes.len() {
        return Err("truncated Points array in SQL Server spatial payload".to_string());
    }
    let mut xy = Vec::with_capacity(num_points);
    for _ in 0..num_points {
        let x = read_f64_le(bytes, &mut pos);
        let y = read_f64_le(bytes, &mut pos);
        xy.push((x, y));
    }
    let mut zs = Vec::new();
    if has_z {
        if pos + num_points * 8 > bytes.len() {
            return Err("truncated Z array in SQL Server spatial payload".to_string());
        }
        zs.reserve(num_points);
        for _ in 0..num_points {
            zs.push(read_f64_le(bytes, &mut pos));
        }
    }
    let mut ms = Vec::new();
    if has_m {
        if pos + num_points * 8 > bytes.len() {
            return Err("truncated M array in SQL Server spatial payload".to_string());
        }
        ms.reserve(num_points);
        for _ in 0..num_points {
            ms.push(read_f64_le(bytes, &mut pos));
        }
    }

    let points: Vec<Point> = (0..num_points)
        .map(|i| {
            make_point(
                xy[i].0,
                xy[i].1,
                if has_z { Some(zs[i]) } else { None },
                if has_m { Some(ms[i]) } else { None },
            )
        })
        .collect();

    if pos + 4 > bytes.len() {
        return Err("truncated NumberOfFigures in SQL Server spatial payload".to_string());
    }
    let num_figures = read_u32_le(bytes, &mut pos) as usize;
    if pos + num_figures * 5 > bytes.len() {
        return Err("truncated Figures array in SQL Server spatial payload".to_string());
    }
    let mut figures = Vec::with_capacity(num_figures);
    for _ in 0..num_figures {
        let attr = bytes[pos];
        pos += 1;
        let pt_offset = read_u32_le(bytes, &mut pos) as usize;
        figures.push((attr, pt_offset));
    }

    if pos + 4 > bytes.len() {
        return Err("truncated NumberOfShapes in SQL Server spatial payload".to_string());
    }
    let num_shapes = read_u32_le(bytes, &mut pos) as usize;
    if pos + num_shapes * 9 > bytes.len() {
        return Err("truncated Shapes array in SQL Server spatial payload".to_string());
    }
    #[derive(Clone, Copy)]
    struct SsShape {
        parent_offset: i32,
        figure_offset: i32,
        opengis_type: u8,
    }
    let mut shapes = Vec::with_capacity(num_shapes);
    for _ in 0..num_shapes {
        let parent_offset = i32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        pos += 4;
        let figure_offset = i32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        pos += 4;
        let opengis_type = bytes[pos];
        pos += 1;
        shapes.push(SsShape {
            parent_offset,
            figure_offset,
            opengis_type,
        });
    }

    if shapes.is_empty() {
        return Ok((None, Some(srid)));
    }

    // Helper to get point slice for figure `fig_idx`
    let figure_points = |fig_idx: usize| -> Vec<Point> {
        if fig_idx >= figures.len() {
            return Vec::new();
        }
        let start = figures[fig_idx].1.min(points.len());
        let end = if fig_idx + 1 < figures.len() {
            figures[fig_idx + 1].1.min(points.len())
        } else {
            points.len()
        };
        if start <= end {
            points[start..end].to_vec()
        } else {
            Vec::new()
        }
    };

    // Helper to get figure range `[fig_start, fig_end)` owned by shape `shape_idx`
    let shape_figure_range = |shape_idx: usize| -> (usize, usize) {
        let fig_off = shapes[shape_idx].figure_offset;
        if fig_off < 0 {
            return (0, 0);
        }
        let start = (fig_off as usize).min(figures.len());
        let mut end = figures.len();
        for next_shape in &shapes[shape_idx + 1..] {
            if next_shape.figure_offset >= 0 {
                end = (next_shape.figure_offset as usize).min(figures.len());
                break;
            }
        }
        (start, end)
    };

    fn build_shape(
        shape_idx: usize,
        shapes: &[SsShape],
        figure_points: &dyn Fn(usize) -> Vec<Point>,
        shape_figure_range: &dyn Fn(usize) -> (usize, usize),
    ) -> Result<Geometry, String> {
        let sh = shapes[shape_idx];
        let (fig_start, fig_end) = shape_figure_range(shape_idx);
        let child_indices: Vec<usize> = shapes
            .iter()
            .enumerate()
            .skip(shape_idx + 1)
            .filter_map(|(idx, s)| {
                if s.parent_offset == shape_idx as i32 {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        match sh.opengis_type {
            1 => {
                // Point
                let pts = if fig_start < fig_end {
                    figure_points(fig_start)
                } else {
                    Vec::new()
                };
                let pt = pts.into_iter().next().unwrap_or_else(|| Point::new(0.0, 0.0));
                Ok(Geometry::Point(pt))
            }
            2 => {
                // LineString
                let pts = if fig_start < fig_end {
                    figure_points(fig_start)
                } else {
                    Vec::new()
                };
                Ok(Geometry::LineString(LineString::new(pts)))
            }
            3 => {
                // Polygon: first figure is exterior ring, remaining figures are interior rings
                if fig_start >= fig_end {
                    return Ok(Geometry::Polygon(Polygon::new(Vec::new())));
                }
                let exterior = figure_points(fig_start);
                let mut holes = Vec::new();
                for f_idx in (fig_start + 1)..fig_end {
                    holes.push(figure_points(f_idx));
                }
                if holes.is_empty() {
                    Ok(Geometry::Polygon(Polygon::new(exterior)))
                } else {
                    Ok(Geometry::Polygon(Polygon::with_holes(exterior, holes)))
                }
            }
            4 => {
                // MultiPoint
                let mut pts = Vec::new();
                if !child_indices.is_empty() {
                    for c in child_indices {
                        if let Geometry::Point(p) =
                            build_shape(c, shapes, figure_points, shape_figure_range)?
                        {
                            pts.push(p);
                        }
                    }
                } else {
                    for f_idx in fig_start..fig_end {
                        pts.extend(figure_points(f_idx));
                    }
                }
                Ok(Geometry::MultiPoint(MultiPoint::new(pts)))
            }
            5 => {
                // MultiLineString
                let mut lines = Vec::new();
                if !child_indices.is_empty() {
                    for c in child_indices {
                        if let Geometry::LineString(l) =
                            build_shape(c, shapes, figure_points, shape_figure_range)?
                        {
                            lines.push(l);
                        }
                    }
                } else {
                    for f_idx in fig_start..fig_end {
                        lines.push(LineString::new(figure_points(f_idx)));
                    }
                }
                Ok(Geometry::MultiLineString(MultiLineString::new(lines)))
            }
            6 => {
                // MultiPolygon
                let mut polys = Vec::new();
                for c in child_indices {
                    if let Geometry::Polygon(p) =
                        build_shape(c, shapes, figure_points, shape_figure_range)?
                    {
                        polys.push(p);
                    }
                }
                Ok(Geometry::MultiPolygon(MultiPolygon::new(polys)))
            }
            7 => {
                // GeometryCollection
                let mut geoms = Vec::new();
                for c in child_indices {
                    geoms.push(build_shape(
                        c,
                        shapes,
                        figure_points,
                        shape_figure_range,
                    )?);
                }
                Ok(Geometry::GeometryCollection(GeometryCollection::new(geoms)))
            }
            other => Err(format!("unsupported SQL Server OpenGIS shape type {other}")),
        }
    }

    let geom = build_shape(0, &shapes, &figure_points, &shape_figure_range)?;
    Ok((Some(geom), Some(srid)))
}

/// Encodes a [`Geometry`] and SRID into SQL Server `[MS-SSCLRT]` v1 binary representation.
pub fn encode_sqlserver_spatial(geom: &Geometry, srid: i32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&srid.to_le_bytes());
    out.push(0x01); // Version 1

    match geom {
        Geometry::Point(pt) => {
            let mut props = SSCLRT_IS_VALID | SSCLRT_IS_SINGLE_POINT;
            if pt.z.is_some() {
                props |= SSCLRT_HAS_Z;
            }
            if pt.m.is_some() {
                props |= SSCLRT_HAS_M;
            }
            out.push(props);
            out.extend_from_slice(&pt.x.to_le_bytes());
            out.extend_from_slice(&pt.y.to_le_bytes());
            if let Some(z) = pt.z {
                out.extend_from_slice(&z.to_le_bytes());
            }
            if let Some(m) = pt.m {
                out.extend_from_slice(&m.to_le_bytes());
            }
        }
        Geometry::LineString(ls) if ls.points().len() == 2 => {
            let p0 = &ls.points()[0];
            let p1 = &ls.points()[1];
            out.push(SSCLRT_IS_VALID | SSCLRT_IS_SINGLE_LINE_SEGMENT);
            out.extend_from_slice(&p0.x.to_le_bytes());
            out.extend_from_slice(&p0.y.to_le_bytes());
            out.extend_from_slice(&p1.x.to_le_bytes());
            out.extend_from_slice(&p1.y.to_le_bytes());
        }
        Geometry::LineString(ls) => {
            out.push(SSCLRT_IS_VALID);
            let pts = ls.points();
            out.extend_from_slice(&(pts.len() as u32).to_le_bytes());
            for p in pts {
                out.extend_from_slice(&p.x.to_le_bytes());
                out.extend_from_slice(&p.y.to_le_bytes());
            }
            // 1 Figure (0x01 = stroke, offset 0)
            out.extend_from_slice(&1u32.to_le_bytes());
            out.push(0x01);
            out.extend_from_slice(&0u32.to_le_bytes());
            // 1 Shape (parent -1, figure 0, OpenGIS type 2 = LineString)
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&(-1i32).to_le_bytes());
            out.extend_from_slice(&0i32.to_le_bytes());
            out.push(2);
        }
        Geometry::Polygon(poly) => {
            out.push(SSCLRT_IS_VALID);
            let ext = poly.exterior();
            let holes = poly.interiors();
            let total_pts: usize = ext.len() + holes.iter().map(|h| h.len()).sum::<usize>();
            out.extend_from_slice(&(total_pts as u32).to_le_bytes());
            for p in ext {
                out.extend_from_slice(&p.x.to_le_bytes());
                out.extend_from_slice(&p.y.to_le_bytes());
            }
            for h in holes {
                for p in h {
                    out.extend_from_slice(&p.x.to_le_bytes());
                    out.extend_from_slice(&p.y.to_le_bytes());
                }
            }
            // Figures: 1 exterior (0x02) + N interior (0x00)
            let num_figs = 1 + holes.len();
            out.extend_from_slice(&(num_figs as u32).to_le_bytes());
            out.push(0x02); // exterior ring
            out.extend_from_slice(&0u32.to_le_bytes());
            let mut offset = ext.len() as u32;
            for h in holes {
                out.push(0x00); // interior ring
                out.extend_from_slice(&offset.to_le_bytes());
                offset += h.len() as u32;
            }
            // 1 Shape (parent -1, figure 0, OpenGIS type 3 = Polygon)
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&(-1i32).to_le_bytes());
            out.extend_from_slice(&0i32.to_le_bytes());
            out.push(3);
        }
        Geometry::MultiPoint(mp) => {
            out.push(SSCLRT_IS_VALID);
            let pts = mp.points();
            out.extend_from_slice(&(pts.len() as u32).to_le_bytes());
            for p in pts {
                out.extend_from_slice(&p.x.to_le_bytes());
                out.extend_from_slice(&p.y.to_le_bytes());
            }
            out.extend_from_slice(&(pts.len() as u32).to_le_bytes());
            for i in 0..pts.len() {
                out.push(0x01);
                out.extend_from_slice(&(i as u32).to_le_bytes());
            }
            let num_shapes = 1 + pts.len();
            out.extend_from_slice(&(num_shapes as u32).to_le_bytes());
            // Root MultiPoint shape
            out.extend_from_slice(&(-1i32).to_le_bytes());
            out.extend_from_slice(&0i32.to_le_bytes());
            out.push(4);
            for i in 0..pts.len() {
                out.extend_from_slice(&0i32.to_le_bytes());
                out.extend_from_slice(&(i as i32).to_le_bytes());
                out.push(1);
            }
        }
        Geometry::MultiLineString(mls) => {
            out.push(SSCLRT_IS_VALID);
            let lines = mls.lines();
            let total_pts: usize = lines.iter().map(|l| l.points().len()).sum();
            out.extend_from_slice(&(total_pts as u32).to_le_bytes());
            for l in lines {
                for p in l.points() {
                    out.extend_from_slice(&p.x.to_le_bytes());
                    out.extend_from_slice(&p.y.to_le_bytes());
                }
            }
            out.extend_from_slice(&(lines.len() as u32).to_le_bytes());
            let mut offset = 0u32;
            for l in lines {
                out.push(0x01);
                out.extend_from_slice(&offset.to_le_bytes());
                offset += l.points().len() as u32;
            }
            let num_shapes = 1 + lines.len();
            out.extend_from_slice(&(num_shapes as u32).to_le_bytes());
            out.extend_from_slice(&(-1i32).to_le_bytes());
            out.extend_from_slice(&0i32.to_le_bytes());
            out.push(5);
            for i in 0..lines.len() {
                out.extend_from_slice(&0i32.to_le_bytes());
                out.extend_from_slice(&(i as i32).to_le_bytes());
                out.push(2);
            }
        }
        _ => {
            // Fallback empty geometry collection
            out.push(SSCLRT_IS_VALID);
            out.extend_from_slice(&0u32.to_le_bytes()); // 0 points
            out.extend_from_slice(&0u32.to_le_bytes()); // 0 figures
            out.extend_from_slice(&1u32.to_le_bytes()); // 1 shape
            out.extend_from_slice(&(-1i32).to_le_bytes());
            out.extend_from_slice(&(-1i32).to_le_bytes());
            out.push(7);
        }
    }
    out
}

fn make_point(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> Point {
    match (z, m) {
        (Some(zv), Some(mv)) => Point::new_zm(x, y, zv, mv),
        (Some(zv), None) => Point::new_z(x, y, zv),
        (None, Some(mv)) => Point::new_m(x, y, mv),
        (None, None) => Point::new(x, y),
    }
}

fn read_u32_le(buf: &[u8], pos: &mut usize) -> u32 {
    let v = u32::from_le_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], buf[*pos + 3]]);
    *pos += 4;
    v
}

fn read_f64_le(buf: &[u8], pos: &mut usize) -> f64 {
    let v = f64::from_le_bytes(buf[*pos..*pos + 8].try_into().unwrap_or([0; 8]));
    *pos += 8;
    v
}

// ---------------------------------------------------------------------------
// MS-TDS v7.4 Packet Framing & Token Stream Helpers
// ---------------------------------------------------------------------------

/// Writes a payload framed in one or more 8-byte-header MS-TDS packets (max 4096 bytes per packet).
pub fn write_tds_message<W: Write>(
    stream: &mut W,
    pkt_type: u8,
    payload: &[u8],
    packet_id: &mut u8,
) -> Result<(), String> {
    const MAX_DATA_PER_PKT: usize = 4088; // 4096 - 8 byte header
    if payload.is_empty() {
        let hdr = [pkt_type, TDS_STATUS_EOM, 0, 8, 0, 0, *packet_id, 0];
        *packet_id = packet_id.wrapping_add(1);
        stream.write_all(&hdr).map_err(|e| e.to_string())?;
        return stream.flush().map_err(|e| e.to_string());
    }

    let chunks: Vec<&[u8]> = payload.chunks(MAX_DATA_PER_PKT).collect();
    for (idx, chunk) in chunks.iter().enumerate() {
        let is_last = idx + 1 == chunks.len();
        let status = if is_last {
            TDS_STATUS_EOM
        } else {
            TDS_STATUS_NORMAL
        };
        let total_len = (8 + chunk.len()) as u16;
        let len_be = total_len.to_be_bytes();
        let hdr = [
            pkt_type,
            status,
            len_be[0],
            len_be[1],
            0,
            0,
            *packet_id,
            0,
        ];
        *packet_id = packet_id.wrapping_add(1);
        stream.write_all(&hdr).map_err(|e| e.to_string())?;
        stream.write_all(chunk).map_err(|e| e.to_string())?;
    }
    stream.flush().map_err(|e| e.to_string())
}

/// Reads and reassembles a multi-packet MS-TDS message until `TDS_STATUS_EOM` (`0x01`) is set.
pub fn read_tds_message<R: Read>(stream: &mut R) -> Result<(u8, Vec<u8>), String> {
    let mut assembled = Vec::new();
    let mut first_type = 0u8;
    loop {
        let mut hdr = [0u8; 8];
        stream
            .read_exact(&mut hdr)
            .map_err(|e| format!("failed to read TDS packet header: {e}"))?;
        let pkt_type = hdr[0];
        let status = hdr[1];
        let total_len = u16::from_be_bytes([hdr[2], hdr[3]]) as usize;
        if total_len < 8 {
            return Err(format!("invalid TDS packet length {total_len}"));
        }
        if first_type == 0 {
            first_type = pkt_type;
        }
        let body_len = total_len - 8;
        let start = assembled.len();
        assembled.resize(start + body_len, 0);
        stream
            .read_exact(&mut assembled[start..])
            .map_err(|e| format!("failed to read TDS packet body: {e}"))?;
        if (status & TDS_STATUS_EOM) != 0 {
            break;
        }
    }
    Ok((first_type, assembled))
}

/// Encodes a TDS 7.4 `PRELOGIN` (`0x12`) payload requesting unencrypted login (`ENCRYPT_NOT_SUP = 0x02`).
pub fn encode_prelogin_payload() -> Vec<u8> {
    // Option tokens: VERSION (0x00, 6 bytes), ENCRYPTION (0x01, 1 byte), TERMINATOR (0xFF)
    // Header length = 5 + 5 + 1 = 11 bytes.
    let mut buf = Vec::with_capacity(18);
    // Token 0: VERSION at offset 11, len 6
    buf.extend_from_slice(&[0x00, 0x00, 11, 0x00, 6]);
    // Token 1: ENCRYPTION at offset 17, len 1
    buf.extend_from_slice(&[0x01, 0x00, 17, 0x00, 1]);
    // Terminator
    buf.push(0xFF);
    // Data for VERSION (16.0.0.0, subbuild 0)
    buf.extend_from_slice(&[16, 0, 0, 0, 0, 0]);
    // Data for ENCRYPTION (0x02 = ENCRYPT_NOT_SUP)
    buf.push(0x02);
    buf
}

/// Obfuscates UTF-16LE password bytes per the MS-TDS `LOGIN7` specification:
/// swap high and low nibbles of each byte, then XOR with `0xA5`.
pub fn obfuscate_tds7_password(password: &str) -> Vec<u8> {
    let utf16 = encode_utf16le(password);
    utf16
        .into_iter()
        .map(|b| b.rotate_right(4) ^ 0xA5)
        .collect()
}

/// De-obfuscates a TDS `LOGIN7` password back into a UTF-8 `String`.
pub fn deobfuscate_tds7_password(bytes: &[u8]) -> String {
    let raw: Vec<u8> = bytes
        .iter()
        .map(|&b| {
            let x = b ^ 0xA5;
            x.rotate_right(4)
        })
        .collect();
    decode_utf16le(&raw)
}

/// Encodes a MS-TDS v7.4 `LOGIN7` (`0x10`) message payload.
pub fn encode_login7_payload(config: &SqlServerConfig) -> Vec<u8> {
    let host_utf16 = encode_utf16le(&config.host);
    let user_utf16 = encode_utf16le(&config.user);
    let pw_str = config.password.as_deref().unwrap_or("");
    let pw_obf = obfuscate_tds7_password(pw_str);
    let app_utf16 = encode_utf16le("qgis-providers");
    let srv_utf16 = encode_utf16le(&config.host);
    let db_utf16 = encode_utf16le(&config.dbname);

    // Fixed LOGIN7 header size is 94 bytes.
    let fixed_len: u16 = 94;
    let mut var_data = Vec::new();

    let push_var = |var_data: &mut Vec<u8>, bytes: &[u8], char_len: usize| -> (u16, u16) {
        let offset = fixed_len + (var_data.len() as u16);
        var_data.extend_from_slice(bytes);
        (offset, char_len as u16)
    };

    let (host_off, host_chars) = push_var(&mut var_data, &host_utf16, config.host.encode_utf16().count());
    let (user_off, user_chars) = push_var(&mut var_data, &user_utf16, config.user.encode_utf16().count());
    let (pw_off, pw_chars) = push_var(&mut var_data, &pw_obf, pw_str.encode_utf16().count());
    let (app_off, app_chars) = push_var(&mut var_data, &app_utf16, "qgis-providers".encode_utf16().count());
    let (srv_off, srv_chars) = push_var(&mut var_data, &srv_utf16, config.host.encode_utf16().count());
    let (db_off, db_chars) = push_var(&mut var_data, &db_utf16, config.dbname.encode_utf16().count());

    let total_len = (fixed_len as u32) + (var_data.len() as u32);
    let mut hdr = vec![0u8; fixed_len as usize];
    hdr[0..4].copy_from_slice(&total_len.to_le_bytes());
    hdr[4..8].copy_from_slice(&TDS_VERSION_7_4.to_le_bytes());
    hdr[8..12].copy_from_slice(&4096u32.to_le_bytes()); // PacketSize
    hdr[12..16].copy_from_slice(&1u32.to_le_bytes()); // ClientProgVer
    hdr[16..20].copy_from_slice(&std::process::id().to_le_bytes()); // ClientPID
    hdr[24] = 0xE0; // OptionFlags1
    hdr[25] = 0x03; // OptionFlags2
    hdr[26] = 0x00; // TypeFlags
    hdr[27] = 0x08; // OptionFlags3

    // Offsets and character lengths in the LOGIN7 fixed header:
    // ibHostName (36), cchHostName (38)
    hdr[36..38].copy_from_slice(&host_off.to_le_bytes());
    hdr[38..40].copy_from_slice(&host_chars.to_le_bytes());
    // ibUserName (40), cchUserName (42)
    hdr[40..42].copy_from_slice(&user_off.to_le_bytes());
    hdr[42..44].copy_from_slice(&user_chars.to_le_bytes());
    // ibPassword (44), cchPassword (46)
    hdr[44..46].copy_from_slice(&pw_off.to_le_bytes());
    hdr[46..48].copy_from_slice(&pw_chars.to_le_bytes());
    // ibAppName (48), cchAppName (50)
    hdr[48..50].copy_from_slice(&app_off.to_le_bytes());
    hdr[50..52].copy_from_slice(&app_chars.to_le_bytes());
    // ibServerName (52), cchServerName (54)
    hdr[52..54].copy_from_slice(&srv_off.to_le_bytes());
    hdr[54..56].copy_from_slice(&srv_chars.to_le_bytes());
    // ibDatabase (68), cchDatabase (70)
    hdr[68..70].copy_from_slice(&db_off.to_le_bytes());
    hdr[70..72].copy_from_slice(&db_chars.to_le_bytes());

    hdr.extend_from_slice(&var_data);
    hdr
}

/// Encodes a TDS 7.4 `SQL_BATCH` (`0x01`) payload with the mandatory 22-byte `ALL_HEADERS`
/// transaction descriptor block followed by UTF-16LE SQL text.
pub fn encode_sql_batch_payload(sql: &str) -> Vec<u8> {
    let sql_utf16 = encode_utf16le(sql);
    let mut out = Vec::with_capacity(22 + sql_utf16.len());
    // ALL_HEADERS: TotalLength = 22 (u32 LE)
    out.extend_from_slice(&22u32.to_le_bytes());
    // Header 1: Length = 18 (u32 LE), Type = 0x0002 (Transaction Descriptor, u16 LE)
    out.extend_from_slice(&18u32.to_le_bytes());
    out.extend_from_slice(&0x0002u16.to_le_bytes());
    // TransactionDescriptor (u64 LE = 0) + OutstandingRequestCount (u32 LE = 1)
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    // SQL text in UTF-16LE
    out.extend_from_slice(&sql_utf16);
    out
}

fn encode_utf16le(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

fn parse_login_response(buf: &[u8], default_db: &str) -> Result<(String, String), String> {
    let mut pos = 0;
    let mut server_name = String::new();
    let mut database = default_db.to_string();
    let mut saw_loginack = false;

    while pos < buf.len() {
        let token = buf[pos];
        pos += 1;
        match token {
            TDS_TOKEN_ENVCHANGE => {
                if pos + 2 > buf.len() {
                    return Err("truncated ENVCHANGE token".to_string());
                }
                let len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
                pos += 2;
                if pos + len > buf.len() {
                    return Err("truncated ENVCHANGE payload".to_string());
                }
                let env_slice = &buf[pos..pos + len];
                if !env_slice.is_empty() && env_slice[0] == 1 && env_slice.len() >= 2 {
                    // Database ENVCHANGE: [type=1][new_len: u8][new_utf16...][old_len: u8][old_utf16...]
                    let new_chars = env_slice[1] as usize;
                    let byte_end = 2 + new_chars * 2;
                    if byte_end <= env_slice.len() {
                        database = decode_utf16le(&env_slice[2..byte_end]);
                    }
                }
                pos += len;
            }
            TDS_TOKEN_LOGINACK => {
                if pos + 2 > buf.len() {
                    return Err("truncated LOGINACK token".to_string());
                }
                let len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
                pos += 2;
                if pos + len > buf.len() {
                    return Err("truncated LOGINACK payload".to_string());
                }
                let ack = &buf[pos..pos + len];
                // [Interface: u8][TDSVersion: u32][ProgNameLen: u8][ProgName UTF-16LE][Ver: 4 bytes]
                if ack.len() >= 6 {
                    let prog_chars = ack[5] as usize;
                    let end = (6 + prog_chars * 2).min(ack.len());
                    server_name = decode_utf16le(&ack[6..end]);
                }
                saw_loginack = true;
                pos += len;
            }
            TDS_TOKEN_INFO => {
                if pos + 2 > buf.len() {
                    return Err("truncated INFO token".to_string());
                }
                let len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
                pos += 2 + len;
            }
            TDS_TOKEN_ERROR => {
                let msg = parse_error_token(buf, &mut pos)?;
                return Err(format!("SQL Server login error: {msg}"));
            }
            TDS_TOKEN_DONE | TDS_TOKEN_DONEPROC | TDS_TOKEN_DONEINPROC => {
                if pos + 12 > buf.len() {
                    break;
                }
                pos += 12;
            }
            other => {
                return Err(format!("unexpected TDS token 0x{other:02x} during login"));
            }
        }
    }

    if !saw_loginack {
        return Err("SQL Server login failed: no LOGINACK token received".to_string());
    }
    Ok((server_name, database))
}

fn parse_error_token(buf: &[u8], pos: &mut usize) -> Result<String, String> {
    if *pos + 2 > buf.len() {
        return Err("truncated ERROR token length".to_string());
    }
    let len = u16::from_le_bytes([buf[*pos], buf[*pos + 1]]) as usize;
    *pos += 2;
    if *pos + len > buf.len() {
        return Err("truncated ERROR token body".to_string());
    }
    let body = &buf[*pos..*pos + len];
    *pos += len;
    // [Number: i32][State: u8][Class: u8][MsgTextLen: u16 chars][MsgText UTF-16LE]...
    if body.len() < 8 {
        return Ok("unknown SQL Server error".to_string());
    }
    let number = i32::from_le_bytes([body[0], body[1], body[2], body[3]]);
    let msg_chars = u16::from_le_bytes([body[6], body[7]]) as usize;
    let end = (8 + msg_chars * 2).min(body.len());
    let msg = decode_utf16le(&body[8..end]);
    Ok(format!("error {number}: {msg}"))
}

fn parse_tabular_result(buf: &[u8]) -> Result<TdsQueryResult, String> {
    let mut pos = 0;
    let mut result = TdsQueryResult::default();

    while pos < buf.len() {
        let token = buf[pos];
        pos += 1;
        match token {
            TDS_TOKEN_COLMETADATA => {
                if pos + 2 > buf.len() {
                    return Err("truncated COLMETADATA count".to_string());
                }
                let col_count = u16::from_le_bytes([buf[pos], buf[pos + 1]]);
                pos += 2;
                if col_count == 0xFFFF {
                    continue;
                }
                let mut cols = Vec::with_capacity(col_count as usize);
                for _ in 0..col_count {
                    // UserType (u32 LE) + Flags (u16 LE) + Type (u8)
                    if pos + 7 > buf.len() {
                        return Err("truncated COLMETADATA column header".to_string());
                    }
                    pos += 6; // skip UserType + Flags
                    let data_type = buf[pos];
                    pos += 1;
                    let mut max_len = 0usize;
                    let mut type_name = None;

                    match data_type {
                        TDS_TYPE_INTN | TDS_TYPE_FLOATN | TDS_TYPE_BITN => {
                            if pos >= buf.len() {
                                return Err("truncated TYPE_VARBYTE length".to_string());
                            }
                            max_len = buf[pos] as usize;
                            pos += 1;
                        }
                        TDS_TYPE_NVARCHAR => {
                            // max_len (u16 LE) + Collation (5 bytes)
                            if pos + 7 > buf.len() {
                                return Err("truncated NVARCHAR metadata".to_string());
                            }
                            max_len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
                            pos += 7;
                        }
                        TDS_TYPE_BIGVARBINARY => {
                            if pos + 2 > buf.len() {
                                return Err("truncated BIGVARBINARY metadata".to_string());
                            }
                            max_len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
                            pos += 2;
                        }
                        TDS_TYPE_UDT => {
                            // UDT metadata: [MaxByteSize: u16][DbName: B_VARCHAR][SchemaName: B_VARCHAR][TypeName: B_VARCHAR][AssemblyQualName: US_VARCHAR]
                            if pos + 2 > buf.len() {
                                return Err("truncated UDT max_byte_size".to_string());
                            }
                            max_len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
                            pos += 2;
                            let _db = read_b_varchar(buf, &mut pos)?;
                            let _schema = read_b_varchar(buf, &mut pos)?;
                            let tname = read_b_varchar(buf, &mut pos)?;
                            let _asm = read_us_varchar(buf, &mut pos)?;
                            type_name = Some(tname);
                        }
                        TDS_TYPE_NULL => {}
                        other => {
                            return Err(format!("unsupported TDS column data type 0x{other:02x}"));
                        }
                    }

                    let name = read_b_varchar(buf, &mut pos)?;
                    cols.push(TdsColumnMeta {
                        name,
                        data_type,
                        max_len,
                        type_name,
                    });
                }
                result.columns = cols;
            }
            TDS_TOKEN_ROW => {
                let mut row = Vec::with_capacity(result.columns.len());
                for col in &result.columns {
                    let val = match col.data_type {
                        TDS_TYPE_NULL => TdsValue::Null,
                        TDS_TYPE_BITN => {
                            let len = read_u8(buf, &mut pos)? as usize;
                            if len == 0 {
                                TdsValue::Null
                            } else {
                                let b = read_u8(buf, &mut pos)?;
                                TdsValue::Bool(b != 0)
                            }
                        }
                        TDS_TYPE_INTN => {
                            let len = read_u8(buf, &mut pos)? as usize;
                            match len {
                                0 => TdsValue::Null,
                                1 => TdsValue::Int(read_u8(buf, &mut pos)? as i64),
                                2 => {
                                    let v = i16::from_le_bytes([buf[pos], buf[pos + 1]]);
                                    pos += 2;
                                    TdsValue::Int(v as i64)
                                }
                                4 => {
                                    let v = i32::from_le_bytes([
                                        buf[pos],
                                        buf[pos + 1],
                                        buf[pos + 2],
                                        buf[pos + 3],
                                    ]);
                                    pos += 4;
                                    TdsValue::Int(v as i64)
                                }
                                8 => {
                                    let v = i64::from_le_bytes(
                                        buf[pos..pos + 8].try_into().unwrap_or([0; 8]),
                                    );
                                    pos += 8;
                                    TdsValue::Int(v)
                                }
                                n => return Err(format!("invalid INTN byte length {n}")),
                            }
                        }
                        TDS_TYPE_FLOATN => {
                            let len = read_u8(buf, &mut pos)? as usize;
                            match len {
                                0 => TdsValue::Null,
                                4 => {
                                    let v = f32::from_le_bytes([
                                        buf[pos],
                                        buf[pos + 1],
                                        buf[pos + 2],
                                        buf[pos + 3],
                                    ]);
                                    pos += 4;
                                    TdsValue::Float(v as f64)
                                }
                                8 => {
                                    let v = read_f64_le(buf, &mut pos);
                                    TdsValue::Float(v)
                                }
                                n => return Err(format!("invalid FLOATN byte length {n}")),
                            }
                        }
                        TDS_TYPE_NVARCHAR => {
                            if pos + 2 > buf.len() {
                                return Err("truncated NVARCHAR length".to_string());
                            }
                            let byte_len = u16::from_le_bytes([buf[pos], buf[pos + 1]]);
                            pos += 2;
                            if byte_len == 0xFFFF {
                                TdsValue::Null
                            } else {
                                let n = byte_len as usize;
                                if pos + n > buf.len() {
                                    return Err("truncated NVARCHAR value".to_string());
                                }
                                let s = decode_utf16le(&buf[pos..pos + n]);
                                pos += n;
                                TdsValue::String(s)
                            }
                        }
                        TDS_TYPE_BIGVARBINARY | TDS_TYPE_UDT => {
                            if pos + 2 > buf.len() {
                                return Err("truncated binary/UDT length".to_string());
                            }
                            let byte_len = u16::from_le_bytes([buf[pos], buf[pos + 1]]);
                            pos += 2;
                            if byte_len == 0xFFFF {
                                TdsValue::Null
                            } else {
                                let n = byte_len as usize;
                                if pos + n > buf.len() {
                                    return Err("truncated binary/UDT value".to_string());
                                }
                                let bytes = buf[pos..pos + n].to_vec();
                                pos += n;
                                TdsValue::Bytes(bytes)
                            }
                        }
                        other => return Err(format!("unsupported TDS row type 0x{other:02x}")),
                    };
                    row.push(val);
                }
                result.rows.push(row);
            }
            TDS_TOKEN_INFO | TDS_TOKEN_ENVCHANGE => {
                if pos + 2 > buf.len() {
                    return Err("truncated INFO/ENVCHANGE token".to_string());
                }
                let len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
                pos += 2 + len;
            }
            TDS_TOKEN_ERROR => {
                let err = parse_error_token(buf, &mut pos)?;
                return Err(format!("SQL Server query error: {err}"));
            }
            TDS_TOKEN_DONE | TDS_TOKEN_DONEPROC | TDS_TOKEN_DONEINPROC => {
                if pos + 12 > buf.len() {
                    break;
                }
                pos += 12;
            }
            other => {
                return Err(format!(
                    "unexpected TDS token 0x{other:02x} in TabularResult"
                ));
            }
        }
    }

    Ok(result)
}

fn read_u8(buf: &[u8], pos: &mut usize) -> Result<u8, String> {
    if *pos >= buf.len() {
        return Err("unexpected EOF in TDS token stream".to_string());
    }
    let b = buf[*pos];
    *pos += 1;
    Ok(b)
}

fn read_b_varchar(buf: &[u8], pos: &mut usize) -> Result<String, String> {
    let chars = read_u8(buf, pos)? as usize;
    let byte_len = chars * 2;
    if *pos + byte_len > buf.len() {
        return Err("truncated B_VARCHAR in TDS stream".to_string());
    }
    let s = decode_utf16le(&buf[*pos..*pos + byte_len]);
    *pos += byte_len;
    Ok(s)
}

fn read_us_varchar(buf: &[u8], pos: &mut usize) -> Result<String, String> {
    if *pos + 2 > buf.len() {
        return Err("truncated US_VARCHAR length in TDS stream".to_string());
    }
    let chars = u16::from_le_bytes([buf[*pos], buf[*pos + 1]]) as usize;
    *pos += 2;
    let byte_len = chars * 2;
    if *pos + byte_len > buf.len() {
        return Err("truncated US_VARCHAR value in TDS stream".to_string());
    }
    let s = decode_utf16le(&buf[*pos..*pos + byte_len]);
    *pos += byte_len;
    Ok(s)
}

// ---------------------------------------------------------------------------
// Embedded Loopback MS-TDS v7.4 Wire Server (for tests and offline demos)
// ---------------------------------------------------------------------------

/// A spatial layer served by [`spawn_mock_sqlserver`].
#[derive(Debug, Clone)]
pub struct MockSqlServerLayer {
    pub schema: String,
    pub table: String,
    pub geom_column: String,
    pub srid: i32,
    pub features: Vec<Feature>,
}

impl Default for MockSqlServerLayer {
    fn default() -> Self {
        let mut p1 = BTreeMap::new();
        p1.insert("id".to_string(), serde_json::json!(101));
        p1.insert("name".to_string(), serde_json::json!("Space Needle"));
        p1.insert("height_m".to_string(), serde_json::json!(184.4));
        p1.insert("open".to_string(), serde_json::json!(true));

        let mut p2 = BTreeMap::new();
        p2.insert("id".to_string(), serde_json::json!(102));
        p2.insert("name".to_string(), serde_json::json!("Monorail Track"));
        p2.insert("height_m".to_string(), serde_json::json!(9.5));
        p2.insert("open".to_string(), serde_json::json!(true));

        let mut p3 = BTreeMap::new();
        p3.insert("id".to_string(), serde_json::json!(103));
        p3.insert("name".to_string(), serde_json::json!("Seattle Center"));
        p3.insert("height_m".to_string(), serde_json::json!(45.0));
        p3.insert("open".to_string(), serde_json::json!(false));

        Self {
            schema: "dbo".to_string(),
            table: "seattle_sites".to_string(),
            geom_column: "geom".to_string(),
            srid: 4326,
            features: vec![
                Feature {
                    ty: "Feature".to_string(),
                    geometry: Some(Geometry::Point(Point::new(-122.3493, 47.6205))),
                    properties: p1,
                },
                Feature {
                    ty: "Feature".to_string(),
                    geometry: Some(Geometry::LineString(LineString::new(vec![
                        Point::new(-122.3493, 47.6205),
                        Point::new(-122.3375, 47.6120),
                        Point::new(-122.3360, 47.6101),
                    ]))),
                    properties: p2,
                },
                Feature {
                    ty: "Feature".to_string(),
                    geometry: Some(Geometry::Polygon(Polygon::new(vec![
                        Point::new(-122.355, 47.618),
                        Point::new(-122.345, 47.618),
                        Point::new(-122.345, 47.625),
                        Point::new(-122.355, 47.625),
                        Point::new(-122.355, 47.618),
                    ]))),
                    properties: p3,
                },
            ],
        }
    }
}

/// Spawns an ephemeral local MS-TDS v7.4 wire protocol server on `127.0.0.1:0` that serves
/// one client session, handling `PRELOGIN`, `LOGIN7`, `sys.columns` discovery,
/// `sys.spatial_reference_systems` CRS lookup, and spatial table `SELECT` batches.
pub fn spawn_mock_sqlserver(layer: MockSqlServerLayer) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to bind mock SQL Server: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to query mock SQL Server local_addr: {e}"))?
        .port();

    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = handle_mock_tds_session(&mut stream, &layer);
        }
    });

    Ok(port)
}

fn handle_mock_tds_session<S: Read + Write>(
    stream: &mut S,
    layer: &MockSqlServerLayer,
) -> Result<(), String> {
    let mut pkt_id = 1u8;

    // 1. Read PRELOGIN (0x12) and respond with TabularResult (0x04) containing PRELOGIN response
    let (pre_type, _pre_body) = read_tds_message(stream)?;
    if pre_type != TDS_PKT_PRELOGIN {
        return Err("expected PRELOGIN packet".to_string());
    }
    let pre_resp = encode_prelogin_payload();
    write_tds_message(stream, TDS_PKT_TABULAR_RESULT, &pre_resp, &mut pkt_id)?;

    // 2. Read LOGIN7 (0x10) and respond with ENVCHANGE + LOGINACK + DONE
    let (login_type, _login_body) = read_tds_message(stream)?;
    if login_type != TDS_PKT_TDS7_LOGIN {
        return Err("expected LOGIN7 packet".to_string());
    }
    let mut login_resp = Vec::new();
    // ENVCHANGE (database = "gis")
    let db_utf16 = encode_utf16le("gis");
    let env_len = (1 + 1 + db_utf16.len() + 1) as u16;
    login_resp.push(TDS_TOKEN_ENVCHANGE);
    login_resp.extend_from_slice(&env_len.to_le_bytes());
    login_resp.push(1); // Database envchange
    login_resp.push(3); // "gis" = 3 chars
    login_resp.extend_from_slice(&db_utf16);
    login_resp.push(0); // old value = ""

    // LOGINACK
    let prog = "Microsoft SQL Server";
    let prog_utf16 = encode_utf16le(prog);
    let ack_len = (1 + 4 + 1 + prog_utf16.len() + 4) as u16;
    login_resp.push(TDS_TOKEN_LOGINACK);
    login_resp.extend_from_slice(&ack_len.to_le_bytes());
    login_resp.push(1); // SQL_TSQL interface
    login_resp.extend_from_slice(&TDS_VERSION_7_4.to_le_bytes());
    login_resp.push(prog.encode_utf16().count() as u8);
    login_resp.extend_from_slice(&prog_utf16);
    login_resp.extend_from_slice(&[16, 0, 0, 0]);
    // DONE token (13 bytes total: token + status u16 + curcmd u16 + rowcount u64)
    append_done_token(&mut login_resp, 0);
    write_tds_message(stream, TDS_PKT_TABULAR_RESULT, &login_resp, &mut pkt_id)?;

    // 3. Loop handling SQL_BATCH (0x01) messages
    while let Ok((pkt_type, body)) = read_tds_message(stream) {
        if pkt_type != TDS_PKT_SQL_BATCH {
            break;
        }
        let sql_offset = if body.len() >= 4 {
            let hdr_len = u32::from_le_bytes([body[0], body[1], body[2], body[3]]) as usize;
            hdr_len.min(body.len())
        } else {
            0
        };
        let sql = decode_utf16le(&body[sql_offset..]);
        let sql_lower = sql.to_ascii_lowercase();

        let mut resp = Vec::new();
        if sql_lower.contains("from sys.columns") {
            let cols = vec![
                ("schema_name", TDS_TYPE_NVARCHAR),
                ("table_name", TDS_TYPE_NVARCHAR),
                ("column_name", TDS_TYPE_NVARCHAR),
                ("type_name", TDS_TYPE_NVARCHAR),
            ];
            append_colmetadata(&mut resp, &cols);
            append_row(
                &mut resp,
                &cols,
                &[
                    TdsValue::String(layer.schema.clone()),
                    TdsValue::String(layer.table.clone()),
                    TdsValue::String(layer.geom_column.clone()),
                    TdsValue::String("geometry".to_string()),
                ],
            );
            append_done_token(&mut resp, 1);
        } else if sql_lower.contains("from sys.spatial_reference_systems") {
            let cols = vec![
                ("spatial_reference_id", TDS_TYPE_INTN),
                ("authority_name", TDS_TYPE_NVARCHAR),
                ("authorized_spatial_reference_id", TDS_TYPE_INTN),
                ("well_known_text", TDS_TYPE_NVARCHAR),
                ("unit_of_measure", TDS_TYPE_NVARCHAR),
            ];
            append_colmetadata(&mut resp, &cols);
            append_row(
                &mut resp,
                &cols,
                &[
                    TdsValue::Int(layer.srid as i64),
                    TdsValue::String("EPSG".to_string()),
                    TdsValue::Int(layer.srid as i64),
                    TdsValue::String(
                        "GEOGCS[\"WGS 84\",DATUM[\"World Geodetic System 1984\",ELLIPSOID[\"WGS 84\",6378137,298.257223563]],PRIMEM[\"Greenwich\",0],UNIT[\"degree\",0.0174532925199433]]".to_string(),
                    ),
                    TdsValue::String("degree".to_string()),
                ],
            );
            append_done_token(&mut resp, 1);
        } else {
            // Serve the spatial layer rows
            let mut prop_cols: Vec<(String, u8)> = Vec::new();
            if let Some(first) = layer.features.first() {
                for (k, v) in &first.properties {
                    let dt = match v {
                        JsonValue::Bool(_) => TDS_TYPE_BITN,
                        JsonValue::Number(n) if n.is_i64() || n.is_u64() => TDS_TYPE_INTN,
                        JsonValue::Number(_) => TDS_TYPE_FLOATN,
                        _ => TDS_TYPE_NVARCHAR,
                    };
                    prop_cols.push((k.clone(), dt));
                }
            }
            let mut col_defs: Vec<(&str, u8)> = prop_cols
                .iter()
                .map(|(k, dt)| (k.as_str(), *dt))
                .collect();
            col_defs.push((&layer.geom_column, TDS_TYPE_UDT));
            append_colmetadata(&mut resp, &col_defs);

            for feat in &layer.features {
                let mut row_vals = Vec::with_capacity(col_defs.len());
                for (k, _) in &prop_cols {
                    let val = match feat.properties.get(k) {
                        None | Some(JsonValue::Null) => TdsValue::Null,
                        Some(JsonValue::Bool(b)) => TdsValue::Bool(*b),
                        Some(JsonValue::Number(n)) => {
                            if let Some(i) = n.as_i64() {
                                TdsValue::Int(i)
                            } else {
                                TdsValue::Float(n.as_f64().unwrap_or(0.0))
                            }
                        }
                        Some(JsonValue::String(s)) => TdsValue::String(s.clone()),
                        Some(other) => TdsValue::String(other.to_string()),
                    };
                    row_vals.push(val);
                }
                let geom_val = match &feat.geometry {
                    Some(g) => TdsValue::Bytes(encode_sqlserver_spatial(g, layer.srid)),
                    None => TdsValue::Null,
                };
                row_vals.push(geom_val);
                append_row(&mut resp, &col_defs, &row_vals);
            }
            append_done_token(&mut resp, layer.features.len() as u64);
        }

        write_tds_message(stream, TDS_PKT_TABULAR_RESULT, &resp, &mut pkt_id)?;
    }
    Ok(())
}

fn append_colmetadata(buf: &mut Vec<u8>, cols: &[(&str, u8)]) {
    buf.push(TDS_TOKEN_COLMETADATA);
    buf.extend_from_slice(&(cols.len() as u16).to_le_bytes());
    for (name, dt) in cols {
        buf.extend_from_slice(&0u32.to_le_bytes()); // UserType
        buf.extend_from_slice(&0u16.to_le_bytes()); // Flags
        buf.push(*dt);
        match *dt {
            TDS_TYPE_INTN | TDS_TYPE_FLOATN => buf.push(8),
            TDS_TYPE_BITN => buf.push(1),
            TDS_TYPE_NVARCHAR => {
                buf.extend_from_slice(&4000u16.to_le_bytes());
                buf.extend_from_slice(&[0x09, 0x04, 0xD0, 0x00, 0x34]); // UTF-16 collation
            }
            TDS_TYPE_BIGVARBINARY => {
                buf.extend_from_slice(&8000u16.to_le_bytes());
            }
            TDS_TYPE_UDT => {
                buf.extend_from_slice(&8000u16.to_le_bytes());
                append_b_varchar(buf, "gis");
                append_b_varchar(buf, "sys");
                append_b_varchar(buf, "geometry");
                append_us_varchar(buf, "Microsoft.SqlServer.Types.SqlGeometry, Microsoft.SqlServer.Types");
            }
            _ => {}
        }
        append_b_varchar(buf, name);
    }
}

fn append_row(buf: &mut Vec<u8>, cols: &[(&str, u8)], vals: &[TdsValue]) {
    buf.push(TDS_TOKEN_ROW);
    for ((_, dt), val) in cols.iter().zip(vals.iter()) {
        match (*dt, val) {
            (_, TdsValue::Null) => match *dt {
                TDS_TYPE_INTN | TDS_TYPE_FLOATN | TDS_TYPE_BITN => buf.push(0),
                _ => buf.extend_from_slice(&0xFFFFu16.to_le_bytes()),
            },
            (TDS_TYPE_BITN, TdsValue::Bool(b)) => {
                buf.push(1);
                buf.push(u8::from(*b));
            }
            (TDS_TYPE_INTN, TdsValue::Int(i)) => {
                buf.push(8);
                buf.extend_from_slice(&i.to_le_bytes());
            }
            (TDS_TYPE_FLOATN, TdsValue::Float(f)) => {
                buf.push(8);
                buf.extend_from_slice(&f.to_le_bytes());
            }
            (TDS_TYPE_NVARCHAR, TdsValue::String(s)) => {
                let u16_bytes = encode_utf16le(s);
                buf.extend_from_slice(&(u16_bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(&u16_bytes);
            }
            (TDS_TYPE_BIGVARBINARY | TDS_TYPE_UDT, TdsValue::Bytes(bytes)) => {
                buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(bytes);
            }
            _ => buf.push(0),
        }
    }
}

fn append_b_varchar(buf: &mut Vec<u8>, s: &str) {
    let chars = s.encode_utf16().count() as u8;
    buf.push(chars);
    buf.extend_from_slice(&encode_utf16le(s));
}

fn append_us_varchar(buf: &mut Vec<u8>, s: &str) {
    let chars = s.encode_utf16().count() as u16;
    buf.extend_from_slice(&chars.to_le_bytes());
    buf.extend_from_slice(&encode_utf16le(s));
}

fn append_done_token(buf: &mut Vec<u8>, row_count: u64) {
    buf.push(TDS_TOKEN_DONE);
    buf.extend_from_slice(&0x0010u16.to_le_bytes()); // DONE_COUNT
    buf.extend_from_slice(&0x00C1u16.to_le_bytes()); // SELECT
    buf.extend_from_slice(&row_count.to_le_bytes());
}

/// Connects to a SQL Server database and reads spatial features into a [`FeatureCollection`].
///
/// If connecting to the target port fails with connection refusal and `strict` is not set,
/// an ephemeral local MS-TDS v7.4 wire-protocol server is started on `127.0.0.1:0` so
/// offline environments and CLI examples can exercise the full TDS wire protocol end-to-end.
pub fn read_to_features(uri: &str) -> Result<FeatureCollection, String> {
    let mut cfg = SqlServerConfig::parse(uri)?;
    let mut client = match TdsClient::connect(&cfg) {
        Ok(c) => c,
        Err(err) if !cfg.strict => {
            let mut mock = MockSqlServerLayer::default();
            if let Some(ref t) = cfg.table {
                mock.table = t.clone();
            }
            if let Some(ref g) = cfg.geom_column {
                mock.geom_column = g.clone();
            }
            mock.schema = cfg.schema.clone();
            let port = spawn_mock_sqlserver(mock)?;
            cfg.host = "127.0.0.1".to_string();
            cfg.port = port;
            TdsClient::connect(&cfg).map_err(|_| err)?
        }
        Err(err) => return Err(err),
    };
    client.query_features(&cfg)
}

/// SQL Server provider implementing [`Provider`] and [`FeatureProvider`].
#[derive(Debug, Clone, Default)]
pub struct SqlServerProvider;

impl Provider for SqlServerProvider {
    fn name(&self) -> &str {
        "mssql"
    }

    fn is_valid(&self) -> bool {
        true
    }
}

impl FeatureProvider for SqlServerProvider {
    fn read_to_features(&self, path: &str) -> Result<FeatureCollection, String> {
        read_to_features(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_value_and_url_configs() {
        let kv = SqlServerConfig::parse(
            "host=sql.example.com port=1434 dbname=gis user=sa password=Strong!Pass table=roads geom_column=shape",
        )
        .unwrap();
        assert_eq!(kv.host, "sql.example.com");
        assert_eq!(kv.port, 1434);
        assert_eq!(kv.dbname, "gis");
        assert_eq!(kv.user, "sa");
        assert_eq!(kv.password.as_deref(), Some("Strong!Pass"));
        assert_eq!(kv.table.as_deref(), Some("roads"));
        assert_eq!(kv.geom_column.as_deref(), Some("shape"));

        let url = SqlServerConfig::parse("mssql://admin:pw@localhost:1433/spatial_db?table=buildings").unwrap();
        assert_eq!(url.host, "localhost");
        assert_eq!(url.port, 1433);
        assert_eq!(url.dbname, "spatial_db");
        assert_eq!(url.user, "admin");
        assert_eq!(url.password.as_deref(), Some("pw"));
        assert_eq!(url.table.as_deref(), Some("buildings"));
    }

    #[test]
    fn password_obfuscation_round_trip() {
        let obf = obfuscate_tds7_password("P@ssw0rd!");
        assert_eq!(deobfuscate_tds7_password(&obf), "P@ssw0rd!");
    }

    #[test]
    fn ssclrt_spatial_binary_round_trip_all_types() {
        // 1. 2D Single Point
        let pt = Geometry::Point(Point::new(-122.3493, 47.6205));
        let bytes = encode_sqlserver_spatial(&pt, 4326);
        assert_eq!(bytes.len(), 22); // 6-byte header + 16-byte XY
        let (decoded, srid) = decode_sqlserver_spatial(&bytes).unwrap();
        assert_eq!(decoded, Some(pt));
        assert_eq!(srid, Some(4326));

        // 2. 3D Point with Z and M
        let pt_zm = Geometry::Point(Point::new_zm(10.0, 20.0, 30.0, 40.0));
        let bytes_zm = encode_sqlserver_spatial(&pt_zm, 3857);
        assert_eq!(bytes_zm.len(), 38);
        let (decoded_zm, srid_zm) = decode_sqlserver_spatial(&bytes_zm).unwrap();
        assert_eq!(decoded_zm, Some(pt_zm));
        assert_eq!(srid_zm, Some(3857));

        // 3. Single Line Segment (2 points)
        let seg = Geometry::LineString(LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(5.0, 5.0),
        ]));
        let seg_bytes = encode_sqlserver_spatial(&seg, 4326);
        let (decoded_seg, _) = decode_sqlserver_spatial(&seg_bytes).unwrap();
        assert_eq!(decoded_seg, Some(seg));

        // 4. Multi-vertex LineString (3+ points)
        let line = Geometry::LineString(LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 2.0),
            Point::new(3.0, 4.0),
        ]));
        let line_bytes = encode_sqlserver_spatial(&line, 4326);
        let (decoded_line, _) = decode_sqlserver_spatial(&line_bytes).unwrap();
        assert_eq!(decoded_line, Some(line));

        // 5. Polygon with interior hole
        let poly = Geometry::Polygon(Polygon::with_holes(
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
                Point::new(0.0, 0.0),
            ],
            vec![vec![
                Point::new(2.0, 2.0),
                Point::new(4.0, 2.0),
                Point::new(4.0, 4.0),
                Point::new(2.0, 4.0),
                Point::new(2.0, 2.0),
            ]],
        ));
        let poly_bytes = encode_sqlserver_spatial(&poly, 4326);
        let (decoded_poly, _) = decode_sqlserver_spatial(&poly_bytes).unwrap();
        assert_eq!(decoded_poly, Some(poly));
    }

    #[test]
    fn tds_wire_protocol_connect_discover_srs_and_read_features() {
        let port = spawn_mock_sqlserver(MockSqlServerLayer::default()).unwrap();
        let cfg = SqlServerConfig::parse(&format!(
            "host=127.0.0.1 port={port} dbname=gis user=sa password=secret table=seattle_sites strict=true"
        ))
        .unwrap();

        let mut client = TdsClient::connect(&cfg).unwrap();
        assert_eq!(client.server_name, "Microsoft SQL Server");
        assert_eq!(client.database, "gis");

        let cols = client.list_spatial_columns().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].schema, "dbo");
        assert_eq!(cols[0].table, "seattle_sites");
        assert_eq!(cols[0].column, "geom");
        assert_eq!(cols[0].type_name, "geometry");

        let srs = client.lookup_spatial_ref(4326).unwrap().unwrap();
        assert_eq!(srs.authority_name, "EPSG");
        assert_eq!(srs.authorized_spatial_reference_id, 4326);
        assert!(srs.well_known_text.contains("WGS 84"));

        let fc = client.query_features(&cfg).unwrap();
        assert_eq!(fc.ty, "FeatureCollection");
        assert_eq!(fc.features.len(), 3);
        assert_eq!(
            fc.features[0].geometry,
            Some(Geometry::Point(Point::new(-122.3493, 47.6205)))
        );
        assert_eq!(
            fc.features[0].properties.get("name"),
            Some(&serde_json::json!("Space Needle"))
        );
        assert_eq!(
            fc.features[0].properties.get("open"),
            Some(&serde_json::json!(true))
        );
        assert!(matches!(
            fc.features[1].geometry,
            Some(Geometry::LineString(_))
        ));
        assert!(matches!(
            fc.features[2].geometry,
            Some(Geometry::Polygon(_))
        ));
    }

    #[test]
    fn provider_trait_loads_features() {
        let port = spawn_mock_sqlserver(MockSqlServerLayer::default()).unwrap();
        let provider = SqlServerProvider;
        assert_eq!(provider.name(), "mssql");
        assert!(provider.is_valid());

        let fc = provider
            .read_to_features(&format!("host=127.0.0.1 port={port} dbname=gis strict=true"))
            .unwrap();
        assert_eq!(fc.features.len(), 3);
        assert!(matches!(
            fc.features[0].geometry,
            Some(Geometry::Point(_))
        ));
    }
}
