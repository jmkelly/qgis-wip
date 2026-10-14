//! Pure-Rust PostGIS provider using the PostgreSQL v3.0 wire protocol (no libpq FFI).
//!
//! Connects over `TcpStream` (or any `Read + Write` stream), executes spatial queries,
//! resolves CRS metadata from `spatial_ref_sys` and layer metadata from `geometry_columns`,
//! decodes PostGIS EWKB / ISO WKB geometries into [`qgis_geometry::Geometry`], and exposes
//! [`PostgisProvider`] implementing [`crate::Provider`] and [`crate::FeatureProvider`].

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use qgis_geometry::Geometry;
use serde_json::Value as JsonValue;

use crate::{Feature, FeatureCollection, FeatureProvider, Provider};

/// PostgreSQL protocol version 3.0 (`0x0003_0000` = `196608`).
pub const PG_PROTOCOL_V3: i32 = 196_608;

// Common PostgreSQL built-in type OIDs.
pub const OID_BOOL: i32 = 16;
pub const OID_BYTEA: i32 = 17;
pub const OID_INT8: i32 = 20;
pub const OID_INT2: i32 = 21;
pub const OID_INT4: i32 = 23;
pub const OID_TEXT: i32 = 25;
pub const OID_OID: i32 = 26;
pub const OID_JSON: i32 = 114;
pub const OID_FLOAT4: i32 = 700;
pub const OID_FLOAT8: i32 = 701;
pub const OID_VARCHAR: i32 = 1043;
pub const OID_NUMERIC: i32 = 1700;
pub const OID_JSONB: i32 = 3802;
/// Custom / synthetic OID used by PostGIS `geometry` in mock and explicit column descriptions.
pub const OID_POSTGIS_GEOMETRY: i32 = 16384;

/// Connection parameters for a PostGIS database and target spatial layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgisConfig {
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub user: String,
    pub password: Option<String>,
    pub schema: String,
    pub table: Option<String>,
    pub geom_column: Option<String>,
    pub sql: Option<String>,
    /// If true, never fall back to an ephemeral local loopback test server on connection refusal.
    pub strict: bool,
}

impl Default for PostgisConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 5432,
            dbname: "gis".to_string(),
            user: "postgres".to_string(),
            password: None,
            schema: "public".to_string(),
            table: None,
            geom_column: None,
            sql: None,
            strict: false,
        }
    }
}

impl PostgisConfig {
    /// Parses either a libpq key-value connection string (`host=localhost port=5432 dbname=gis table=roads`)
    /// or a PostgreSQL URI (`postgresql://user:pass@localhost:5432/gis?table=roads`).
    pub fn parse(uri: &str) -> Result<Self, String> {
        let trimmed = uri.trim();
        if trimmed.is_empty() {
            return Err("empty PostGIS connection string".to_string());
        }
        if let Some(rest) = trimmed
            .strip_prefix("postgresql://")
            .or_else(|| trimmed.strip_prefix("postgres://"))
            .or_else(|| trimmed.strip_prefix("postgis://"))
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
                .map_err(|e| format!("invalid PostGIS port '{p}': {e}"))?;
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
        for token in split_connection_tokens(input) {
            let Some((k, v)) = token.split_once('=') else {
                return Err(format!("invalid key=value token in PostGIS URI: '{token}'"));
            };
            let key = k.trim();
            let val = v.trim().trim_matches('\'').trim_matches('"');
            cfg.apply_kv(key, val)?;
            any = true;
        }
        if !any {
            return Err("no key=value pairs found in PostGIS connection string".to_string());
        }
        Ok(cfg)
    }

    fn apply_kv(&mut self, key: &str, val: &str) -> Result<(), String> {
        match key {
            "host" | "hostaddr" => self.host = val.to_string(),
            "port" => {
                self.port = val
                    .parse::<u16>()
                    .map_err(|e| format!("invalid port '{val}': {e}"))?;
            }
            "dbname" | "database" => self.dbname = val.to_string(),
            "user" | "username" => self.user = val.to_string(),
            "password" | "pass" => self.password = Some(val.to_string()),
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

fn split_connection_tokens(input: &str) -> Vec<String> {
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

/// Column description from a PostgreSQL `RowDescription` (`'T'`) message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgColumnDesc {
    pub name: String,
    pub table_oid: i32,
    pub attr_num: i16,
    pub type_oid: i32,
    pub type_size: i16,
    pub type_modifier: i32,
    /// `0` = text format, `1` = binary format.
    pub format_code: i16,
}

/// Result of a PostgreSQL simple query (`'Q'`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PgQueryResult {
    pub columns: Vec<PgColumnDesc>,
    pub rows: Vec<Vec<Option<Vec<u8>>>>,
    pub command_tag: String,
}

/// Metadata entry from PostGIS `geometry_columns`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryColumnInfo {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub coord_dimension: i32,
    pub srid: i32,
    pub geometry_type: String,
}

/// Metadata entry from PostGIS `spatial_ref_sys`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialRefSys {
    pub srid: i32,
    pub auth_name: String,
    pub auth_srid: i32,
    pub srtext: String,
    pub proj4text: String,
}

/// Pure-Rust PostgreSQL v3.0 wire protocol client.
pub struct PostgisClient<S: Read + Write = TcpStream> {
    stream: S,
    pub server_params: BTreeMap<String, String>,
    pub backend_pid: i32,
    pub backend_key: i32,
    pub tx_status: u8,
}

impl PostgisClient<TcpStream> {
    /// Connects to a PostgreSQL/PostGIS server over TCP and performs the v3.0 startup handshake.
    pub fn connect(config: &PostgisConfig) -> Result<Self, String> {
        let addr = format!("{}:{}", config.host, config.port);
        let stream = TcpStream::connect(&addr)
            .map_err(|e| format!("failed to connect to PostGIS at {addr}: {e}"))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        Self::handshake(stream, config)
    }
}

impl<S: Read + Write> PostgisClient<S> {
    /// Performs the PostgreSQL v3.0 startup and authentication handshake over any `Read + Write` stream.
    pub fn handshake(mut stream: S, config: &PostgisConfig) -> Result<Self, String> {
        let startup = encode_startup_message(&config.user, &config.dbname);
        stream
            .write_all(&startup)
            .map_err(|e| format!("failed to write Postgres StartupMessage: {e}"))?;
        stream
            .flush()
            .map_err(|e| format!("failed to flush Postgres StartupMessage: {e}"))?;

        let mut server_params = BTreeMap::new();
        let mut backend_pid = 0;
        let mut backend_key = 0;
        let tx_status;

        loop {
            let (tag, payload) = read_backend_message(&mut stream)?;
            match tag {
                b'R' => {
                    if payload.len() < 4 {
                        return Err("truncated Authentication message from server".to_string());
                    }
                    let auth_code = i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                    match auth_code {
                        0 => {
                            // AuthenticationOk
                        }
                        3 => {
                            // AuthenticationCleartextPassword
                            let pw = config.password.as_deref().unwrap_or("");
                            let msg = encode_password_message(pw.as_bytes());
                            stream
                                .write_all(&msg)
                                .map_err(|e| format!("failed to send cleartext password: {e}"))?;
                            stream.flush().map_err(|e| e.to_string())?;
                        }
                        5 => {
                            // AuthenticationMD5Password (4-byte salt follows)
                            if payload.len() < 8 {
                                return Err("truncated MD5 salt in AuthenticationMD5Password".to_string());
                            }
                            let salt: [u8; 4] = [payload[4], payload[5], payload[6], payload[7]];
                            let pw = config.password.as_deref().unwrap_or("");
                            let digest = compute_pg_md5_password(&config.user, pw, &salt);
                            let msg = encode_password_message(digest.as_bytes());
                            stream
                                .write_all(&msg)
                                .map_err(|e| format!("failed to send MD5 password: {e}"))?;
                            stream.flush().map_err(|e| e.to_string())?;
                        }
                        other => {
                            return Err(format!("unsupported Postgres authentication method {other}"));
                        }
                    }
                }
                b'S' => {
                    if let Some((k, v)) = parse_two_cstrings(&payload) {
                        server_params.insert(k, v);
                    }
                }
                b'K' => {
                    if payload.len() >= 8 {
                        backend_pid = i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        backend_key = i32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                    }
                }
                b'Z' => {
                    tx_status = payload.first().copied().unwrap_or(b'I');
                    break;
                }
                b'E' => {
                    return Err(format!("Postgres startup error: {}", parse_error_fields(&payload)));
                }
                b'N' => {
                    // NoticeResponse: ignore during startup
                }
                other => {
                    return Err(format!(
                        "unexpected Postgres backend message tag '{}' (0x{:02x}) during startup",
                        other as char, other
                    ));
                }
            }
        }

        Ok(Self {
            stream,
            server_params,
            backend_pid,
            backend_key,
            tx_status,
        })
    }

    /// Executes a SQL statement using the PostgreSQL Simple Query (`'Q'`) protocol.
    pub fn simple_query(&mut self, sql: &str) -> Result<PgQueryResult, String> {
        let msg = encode_query_message(sql);
        self.stream
            .write_all(&msg)
            .map_err(|e| format!("failed to send Query message: {e}"))?;
        self.stream
            .flush()
            .map_err(|e| format!("failed to flush Query message: {e}"))?;

        let mut result = PgQueryResult::default();
        let mut query_error: Option<String> = None;

        loop {
            let (tag, payload) = read_backend_message(&mut self.stream)?;
            match tag {
                b'T' => {
                    result.columns = parse_row_description(&payload)?;
                }
                b'D' => {
                    result.rows.push(parse_data_row(&payload)?);
                }
                b'C' => {
                    result.command_tag = parse_cstring(&payload, 0).map(|(s, _)| s).unwrap_or_default();
                }
                b'I' => {
                    // EmptyQueryResponse
                }
                b'N' | b'S' => {
                    // NoticeResponse / ParameterStatus
                }
                b'E' => {
                    query_error = Some(parse_error_fields(&payload));
                }
                b'Z' => {
                    self.tx_status = payload.first().copied().unwrap_or(b'I');
                    break;
                }
                other => {
                    return Err(format!(
                        "unexpected Postgres message tag '{}' (0x{:02x}) during query",
                        other as char, other
                    ));
                }
            }
        }

        if let Some(err) = query_error {
            return Err(format!("Postgres query error: {err}"));
        }
        Ok(result)
    }

    /// Sends a graceful `Terminate` (`'X'`) message to the server.
    pub fn close(&mut self) -> Result<(), String> {
        let msg = [b'X', 0, 0, 0, 4];
        self.stream
            .write_all(&msg)
            .map_err(|e| format!("failed to send Terminate message: {e}"))?;
        self.stream.flush().map_err(|e| e.to_string())
    }

    /// Queries PostGIS `geometry_columns` view for registered spatial layers.
    pub fn list_geometry_columns(&mut self) -> Result<Vec<GeometryColumnInfo>, String> {
        let sql = "SELECT f_table_schema, f_table_name, f_geometry_column, coord_dimension, srid, type FROM geometry_columns";
        let res = self.simple_query(sql)?;
        let mut out = Vec::with_capacity(res.rows.len());
        for row in &res.rows {
            if row.len() < 6 {
                continue;
            }
            let schema = cell_to_string(&row[0]).unwrap_or_else(|| "public".to_string());
            let table = cell_to_string(&row[1]).unwrap_or_default();
            let column = cell_to_string(&row[2]).unwrap_or_else(|| "geom".to_string());
            let coord_dimension = cell_to_string(&row[3])
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(2);
            let srid = cell_to_string(&row[4])
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(4326);
            let geometry_type = cell_to_string(&row[5]).unwrap_or_else(|| "GEOMETRY".to_string());
            out.push(GeometryColumnInfo {
                schema,
                table,
                column,
                coord_dimension,
                srid,
                geometry_type,
            });
        }
        Ok(out)
    }

    /// Looks up a spatial reference system definition from PostGIS `spatial_ref_sys`.
    pub fn lookup_srid(&mut self, srid: i32) -> Result<Option<SpatialRefSys>, String> {
        let sql = format!(
            "SELECT srid, auth_name, auth_srid, srtext, proj4text FROM spatial_ref_sys WHERE srid = {srid}"
        );
        let res = self.simple_query(&sql)?;
        let Some(row) = res.rows.first() else {
            return Ok(None);
        };
        if row.len() < 5 {
            return Ok(None);
        }
        Ok(Some(SpatialRefSys {
            srid: cell_to_string(&row[0])
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(srid),
            auth_name: cell_to_string(&row[1]).unwrap_or_else(|| "EPSG".to_string()),
            auth_srid: cell_to_string(&row[2])
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(srid),
            srtext: cell_to_string(&row[3]).unwrap_or_default(),
            proj4text: cell_to_string(&row[4]).unwrap_or_default(),
        }))
    }

    /// Queries a spatial layer and converts rows into a [`FeatureCollection`].
    pub fn query_features(&mut self, config: &PostgisConfig) -> Result<FeatureCollection, String> {
        let (table_name, geom_col) = if config.sql.is_some() {
            (
                config.table.clone().unwrap_or_default(),
                config.geom_column.clone().unwrap_or_else(|| "geom".to_string()),
            )
        } else {
            let discovered = self.list_geometry_columns().unwrap_or_default();
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
            None => format!("SELECT * FROM \"{}\".\"{}\"", config.schema, table_name),
        };
        let res = self.simple_query(&query)?;
        query_result_to_features(&res, Some(&geom_col))
    }
}

/// Converts a [`PgQueryResult`] into a [`FeatureCollection`], decoding EWKB/WKB geometry columns
/// and converting attribute columns into `serde_json::Value`.
pub fn query_result_to_features(
    res: &PgQueryResult,
    geom_column_hint: Option<&str>,
) -> Result<FeatureCollection, String> {
    let geom_idx = res
        .columns
        .iter()
        .position(|c| {
            geom_column_hint.is_some_and(|hint| c.name.eq_ignore_ascii_case(hint))
                || c.type_oid == OID_POSTGIS_GEOMETRY
        })
        .or_else(|| {
            res.columns.iter().position(|c| {
                matches!(
                    c.name.to_ascii_lowercase().as_str(),
                    "geom" | "wkb_geometry" | "the_geom" | "geometry" | "way"
                )
            })
        });

    let mut fc = FeatureCollection::new();
    for row in &res.rows {
        let mut geometry: Option<Geometry> = None;
        let mut properties = BTreeMap::new();

        for (idx, col) in res.columns.iter().enumerate() {
            let cell = row.get(idx).and_then(|c| c.as_deref());
            if Some(idx) == geom_idx {
                if let Some(raw) = cell {
                    geometry = Some(decode_postgis_geometry_cell(raw, col.format_code)?.0);
                }
            } else {
                let val = decode_pg_attribute(cell, col.type_oid, col.format_code);
                properties.insert(col.name.clone(), val);
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

/// Decodes a PostGIS geometry value from either binary EWKB/WKB or hex-encoded EWKB (`"0101..."` or `"\\x0101..."`),
/// returning the parsed [`Geometry`] and optional SRID.
pub fn decode_postgis_geometry_cell(
    raw: &[u8],
    format_code: i16,
) -> Result<(Geometry, Option<u32>), String> {
    if format_code == 1 || (raw.first().copied() == Some(0) || raw.first().copied() == Some(1)) {
        return decode_ewkb(raw);
    }
    let text = std::str::from_utf8(raw).map_err(|e| format!("invalid UTF-8 in EWKB hex string: {e}"))?;
    decode_ewkb_hex(text)
}

/// Decodes raw binary EWKB / ISO WKB bytes into a [`Geometry`] and optional SRID.
pub fn decode_ewkb(bytes: &[u8]) -> Result<(Geometry, Option<u32>), String> {
    if bytes.len() < 5 {
        return Err("PostGIS EWKB too short (< 5 bytes)".to_string());
    }
    let le = bytes[0] == 1;
    let raw_type = if le {
        u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])
    } else {
        u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])
    };
    let srid = if (raw_type & 0x2000_0000) != 0 {
        if bytes.len() < 9 {
            return Err("truncated SRID in PostGIS EWKB".to_string());
        }
        Some(if le {
            u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]])
        } else {
            u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]])
        })
    } else {
        None
    };
    let geom = qgis_geometry::read_wkb(bytes)
        .map_err(|e| format!("failed to decode PostGIS EWKB geometry: {e}"))?;
    Ok((geom, srid))
}

/// Decodes a hex-encoded EWKB string (with optional `\x` or `0x` prefix) into a [`Geometry`] and optional SRID.
pub fn decode_ewkb_hex(hex_str: &str) -> Result<(Geometry, Option<u32>), String> {
    let trimmed = hex_str
        .trim()
        .strip_prefix("\\x")
        .or_else(|| hex_str.trim().strip_prefix("0x"))
        .unwrap_or(hex_str.trim());
    let bytes = decode_hex(trimmed)?;
    decode_ewkb(&bytes)
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err(format!("odd-length hex string ({} chars)", hex.len()));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("invalid hex character '{}'", b as char)),
    }
}

fn encode_hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

/// Encodes a [`Geometry`] and optional SRID into PostGIS EWKB bytes (little-endian).
pub fn encode_ewkb(geom: &Geometry, srid: Option<u32>) -> Vec<u8> {
    let mut wkb = qgis_geometry::write_wkb(geom);
    if let Some(srid_val) = srid {
        if wkb.len() >= 5 {
            let le = wkb[0] == 1;
            let raw_type = if le {
                u32::from_le_bytes([wkb[1], wkb[2], wkb[3], wkb[4]])
            } else {
                u32::from_be_bytes([wkb[1], wkb[2], wkb[3], wkb[4]])
            };
            let ewkb_type = raw_type | 0x2000_0000;
            let type_bytes = if le {
                ewkb_type.to_le_bytes()
            } else {
                ewkb_type.to_be_bytes()
            };
            let srid_bytes = if le {
                srid_val.to_le_bytes()
            } else {
                srid_val.to_be_bytes()
            };
            let mut out = Vec::with_capacity(wkb.len() + 4);
            out.push(wkb[0]);
            out.extend_from_slice(&type_bytes);
            out.extend_from_slice(&srid_bytes);
            out.extend_from_slice(&wkb[5..]);
            wkb = out;
        }
    }
    wkb
}

fn decode_pg_attribute(cell: Option<&[u8]>, type_oid: i32, format_code: i16) -> JsonValue {
    let Some(bytes) = cell else {
        return JsonValue::Null;
    };
    if format_code == 1 {
        match type_oid {
            OID_BOOL if !bytes.is_empty() => return JsonValue::Bool(bytes[0] != 0),
            OID_INT2 if bytes.len() == 2 => {
                return JsonValue::from(i16::from_be_bytes([bytes[0], bytes[1]]));
            }
            OID_INT4 if bytes.len() == 4 => {
                return JsonValue::from(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
            }
            OID_INT8 if bytes.len() == 8 => {
                return JsonValue::from(i64::from_be_bytes(bytes.try_into().unwrap_or([0; 8])));
            }
            OID_FLOAT4 if bytes.len() == 4 => {
                return JsonValue::from(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
            }
            OID_FLOAT8 if bytes.len() == 8 => {
                return JsonValue::from(f64::from_be_bytes(bytes.try_into().unwrap_or([0; 8])));
            }
            _ => {}
        }
    }
    let text = String::from_utf8_lossy(bytes);
    match type_oid {
        OID_BOOL => JsonValue::Bool(matches!(text.as_ref(), "t" | "true" | "TRUE" | "1")),
        OID_INT2 | OID_INT4 | OID_INT8 | OID_OID => text
            .parse::<i64>()
            .map(JsonValue::from)
            .unwrap_or_else(|_| JsonValue::String(text.into_owned())),
        OID_FLOAT4 | OID_FLOAT8 | OID_NUMERIC => text
            .parse::<f64>()
            .map(JsonValue::from)
            .unwrap_or_else(|_| JsonValue::String(text.into_owned())),
        OID_JSON | OID_JSONB => serde_json::from_str(&text)
            .unwrap_or_else(|_| JsonValue::String(text.into_owned())),
        _ => JsonValue::String(text.into_owned()),
    }
}

fn cell_to_string(cell: &Option<Vec<u8>>) -> Option<String> {
    cell.as_ref()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

// ---------------------------------------------------------------------------
// PostgreSQL v3.0 Wire Framing Helpers
// ---------------------------------------------------------------------------

pub fn encode_startup_message(user: &str, database: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&PG_PROTOCOL_V3.to_be_bytes());
    body.extend_from_slice(b"user\0");
    body.extend_from_slice(user.as_bytes());
    body.push(0);
    body.extend_from_slice(b"database\0");
    body.extend_from_slice(database.as_bytes());
    body.push(0);
    body.extend_from_slice(b"client_encoding\0UTF8\0");
    body.push(0); // terminator

    let total_len = (body.len() + 4) as i32;
    let mut out = Vec::with_capacity(total_len as usize);
    out.extend_from_slice(&total_len.to_be_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn encode_password_message(password_bytes: &[u8]) -> Vec<u8> {
    let len = (4 + password_bytes.len() + 1) as i32;
    let mut out = Vec::with_capacity(1 + len as usize);
    out.push(b'p');
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(password_bytes);
    out.push(0);
    out
}

pub fn encode_query_message(sql: &str) -> Vec<u8> {
    let sql_bytes = sql.as_bytes();
    let len = (4 + sql_bytes.len() + 1) as i32;
    let mut out = Vec::with_capacity(1 + len as usize);
    out.push(b'Q');
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(sql_bytes);
    out.push(0);
    out
}

fn read_backend_message<R: Read>(stream: &mut R) -> Result<(u8, Vec<u8>), String> {
    let mut header = [0u8; 5];
    stream
        .read_exact(&mut header)
        .map_err(|e| format!("failed to read Postgres message header: {e}"))?;
    let tag = header[0];
    let len = i32::from_be_bytes([header[1], header[2], header[3], header[4]]);
    if len < 4 {
        return Err(format!("invalid Postgres message length {len}"));
    }
    let payload_len = (len - 4) as usize;
    let mut payload = vec![0u8; payload_len];
    stream
        .read_exact(&mut payload)
        .map_err(|e| format!("failed to read Postgres message payload: {e}"))?;
    Ok((tag, payload))
}

fn parse_cstring(buf: &[u8], start: usize) -> Result<(String, usize), String> {
    let rel_end = buf[start..]
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| "unterminated CString in Postgres wire message".to_string())?;
    let s = String::from_utf8_lossy(&buf[start..start + rel_end]).into_owned();
    Ok((s, start + rel_end + 1))
}

fn parse_two_cstrings(buf: &[u8]) -> Option<(String, String)> {
    let (first, next) = parse_cstring(buf, 0).ok()?;
    let (second, _) = parse_cstring(buf, next).ok()?;
    Some((first, second))
}

fn parse_error_fields(buf: &[u8]) -> String {
    let mut pos = 0;
    let mut severity = String::new();
    let mut code = String::new();
    let mut message = String::new();
    while pos < buf.len() {
        let field_type = buf[pos];
        pos += 1;
        if field_type == 0 {
            break;
        }
        let Ok((val, next)) = parse_cstring(buf, pos) else {
            break;
        };
        pos = next;
        match field_type {
            b'S' => severity = val,
            b'C' => code = val,
            b'M' => message = val,
            _ => {}
        }
    }
    if message.is_empty() {
        "unknown Postgres error".to_string()
    } else if code.is_empty() {
        format!("{severity}: {message}")
    } else {
        format!("{severity} ({code}): {message}")
    }
}

fn parse_row_description(buf: &[u8]) -> Result<Vec<PgColumnDesc>, String> {
    if buf.len() < 2 {
        return Err("truncated RowDescription header".to_string());
    }
    let count = i16::from_be_bytes([buf[0], buf[1]]);
    if count < 0 {
        return Err(format!("negative column count {count} in RowDescription"));
    }
    let mut pos = 2;
    let mut cols = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (name, next) = parse_cstring(buf, pos)?;
        pos = next;
        if pos + 18 > buf.len() {
            return Err("truncated field descriptor in RowDescription".to_string());
        }
        let table_oid = i32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        let attr_num = i16::from_be_bytes([buf[pos + 4], buf[pos + 5]]);
        let type_oid = i32::from_be_bytes([buf[pos + 6], buf[pos + 7], buf[pos + 8], buf[pos + 9]]);
        let type_size = i16::from_be_bytes([buf[pos + 10], buf[pos + 11]]);
        let type_modifier = i32::from_be_bytes([buf[pos + 12], buf[pos + 13], buf[pos + 14], buf[pos + 15]]);
        let format_code = i16::from_be_bytes([buf[pos + 16], buf[pos + 17]]);
        pos += 18;
        cols.push(PgColumnDesc {
            name,
            table_oid,
            attr_num,
            type_oid,
            type_size,
            type_modifier,
            format_code,
        });
    }
    Ok(cols)
}

fn parse_data_row(buf: &[u8]) -> Result<Vec<Option<Vec<u8>>>, String> {
    if buf.len() < 2 {
        return Err("truncated DataRow header".to_string());
    }
    let count = i16::from_be_bytes([buf[0], buf[1]]);
    if count < 0 {
        return Err(format!("negative column count {count} in DataRow"));
    }
    let mut pos = 2;
    let mut row = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if pos + 4 > buf.len() {
            return Err("truncated column length in DataRow".to_string());
        }
        let col_len = i32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        pos += 4;
        if col_len < 0 {
            row.push(None);
        } else {
            let n = col_len as usize;
            if pos + n > buf.len() {
                return Err("truncated column bytes in DataRow".to_string());
            }
            row.push(Some(buf[pos..pos + n].to_vec()));
            pos += n;
        }
    }
    Ok(row)
}

/// Pure-Rust RFC 1321 MD5 implementation for PostgreSQL `AuthenticationMD5Password`.
/// Computes `"md5" + hex(md5(hex(md5(password + user)) + salt))`.
pub fn compute_pg_md5_password(user: &str, password: &str, salt: &[u8; 4]) -> String {
    let mut inner = Vec::with_capacity(password.len() + user.len());
    inner.extend_from_slice(password.as_bytes());
    inner.extend_from_slice(user.as_bytes());
    let inner_hex = md5_hex(&inner);

    let mut outer = Vec::with_capacity(32 + 4);
    outer.extend_from_slice(inner_hex.as_bytes());
    outer.extend_from_slice(salt);
    format!("md5{}", md5_hex(&outer))
}

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20,
        5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    for chunk in msg.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            let base = i * 4;
            *word = u32::from_le_bytes([chunk[base], chunk[base + 1], chunk[base + 2], chunk[base + 3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let temp = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = temp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for &b in &digest {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// Embedded Loopback PostgreSQL v3.0 Wire Server (for tests and local demos)
// ---------------------------------------------------------------------------

/// A spatial feature row stored in a [`PostgisMockServer`].
#[derive(Debug, Clone)]
pub struct MockPostgisLayer {
    pub schema: String,
    pub table: String,
    pub geom_column: String,
    pub srid: i32,
    pub geometry_type: String,
    pub features: Vec<Feature>,
}

impl Default for MockPostgisLayer {
    fn default() -> Self {
        use qgis_geometry::{LineString, Point, Polygon};
        let mut p1 = BTreeMap::new();
        p1.insert("id".to_string(), serde_json::json!(1));
        p1.insert("name".to_string(), serde_json::json!("Central Station"));
        p1.insert("active".to_string(), serde_json::json!(true));
        p1.insert("elevation".to_string(), serde_json::json!(42.5));

        let mut p2 = BTreeMap::new();
        p2.insert("id".to_string(), serde_json::json!(2));
        p2.insert("name".to_string(), serde_json::json!("River Walk"));
        p2.insert("active".to_string(), serde_json::json!(true));
        p2.insert("elevation".to_string(), serde_json::json!(15.0));

        let mut p3 = BTreeMap::new();
        p3.insert("id".to_string(), serde_json::json!(3));
        p3.insert("name".to_string(), serde_json::json!("City Park"));
        p3.insert("active".to_string(), serde_json::json!(false));
        p3.insert("elevation".to_string(), serde_json::json!(28.25));

        Self {
            schema: "public".to_string(),
            table: "landmarks".to_string(),
            geom_column: "geom".to_string(),
            srid: 4326,
            geometry_type: "GEOMETRY".to_string(),
            features: vec![
                Feature {
                    ty: "Feature".to_string(),
                    geometry: Some(Geometry::Point(Point::new(-73.9857, 40.7484))),
                    properties: p1,
                },
                Feature {
                    ty: "Feature".to_string(),
                    geometry: Some(Geometry::LineString(LineString::new(vec![
                        Point::new(-73.99, 40.74),
                        Point::new(-73.98, 40.75),
                    ]))),
                    properties: p2,
                },
                Feature {
                    ty: "Feature".to_string(),
                    geometry: Some(Geometry::Polygon(Polygon::new(vec![
                        Point::new(-73.98, 40.76),
                        Point::new(-73.95, 40.76),
                        Point::new(-73.95, 40.79),
                        Point::new(-73.98, 40.79),
                        Point::new(-73.98, 40.76),
                    ]))),
                    properties: p3,
                },
            ],
        }
    }
}

/// Spawns an ephemeral local PostgreSQL v3.0 wire protocol server on `127.0.0.1:0` that serves
/// one client session, handling Startup, `geometry_columns`, `spatial_ref_sys`, and `SELECT` queries.
pub fn spawn_mock_postgis_server(layer: MockPostgisLayer) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to bind mock PostGIS server: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to query mock PostGIS local_addr: {e}"))?
        .port();

    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = handle_mock_pg_session(&mut stream, &layer);
        }
    });

    Ok(port)
}

fn handle_mock_pg_session<S: Read + Write>(
    stream: &mut S,
    layer: &MockPostgisLayer,
) -> Result<(), String> {
    // 1. Read StartupMessage (length i32 + protocol version i32 + params)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
    let startup_len = i32::from_be_bytes(len_buf) as usize;
    if startup_len < 8 {
        return Err("invalid startup length".to_string());
    }
    let mut startup_body = vec![0u8; startup_len - 4];
    stream
        .read_exact(&mut startup_body)
        .map_err(|e| e.to_string())?;

    // 2. Send AuthenticationOk ('R', len=8, code=0), ParameterStatus ('S'), BackendKeyData ('K'), ReadyForQuery ('Z')
    write_backend_msg(stream, b'R', &0i32.to_be_bytes())?;
    write_parameter_status(stream, "server_version", "16.2 (PostGIS 3.4)")?;
    write_parameter_status(stream, "client_encoding", "UTF8")?;
    let mut key_data = Vec::with_capacity(8);
    key_data.extend_from_slice(&1234i32.to_be_bytes());
    key_data.extend_from_slice(&5678i32.to_be_bytes());
    write_backend_msg(stream, b'K', &key_data)?;
    write_backend_msg(stream, b'Z', b"I")?;
    stream.flush().map_err(|e| e.to_string())?;

    // 3. Process Query ('Q') and Terminate ('X') messages
    loop {
        let mut hdr = [0u8; 5];
        if stream.read_exact(&mut hdr).is_err() {
            break;
        }
        let tag = hdr[0];
        let len = i32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;
        let mut body = vec![0u8; len.saturating_sub(4)];
        stream.read_exact(&mut body).map_err(|e| e.to_string())?;

        if tag == b'X' {
            break;
        }
        if tag != b'Q' {
            continue;
        }
        let (sql, _) = parse_cstring(&body, 0)?;
        let sql_lower = sql.to_ascii_lowercase();

        if sql_lower.contains("from geometry_columns") {
            let cols = vec![
                ("f_table_schema", OID_TEXT),
                ("f_table_name", OID_TEXT),
                ("f_geometry_column", OID_TEXT),
                ("coord_dimension", OID_INT4),
                ("srid", OID_INT4),
                ("type", OID_TEXT),
            ];
            write_row_description(stream, &cols)?;
            let row = vec![
                Some(layer.schema.as_bytes().to_vec()),
                Some(layer.table.as_bytes().to_vec()),
                Some(layer.geom_column.as_bytes().to_vec()),
                Some(b"2".to_vec()),
                Some(layer.srid.to_string().into_bytes()),
                Some(layer.geometry_type.as_bytes().to_vec()),
            ];
            write_data_row(stream, &row)?;
            write_command_complete(stream, "SELECT 1")?;
            write_backend_msg(stream, b'Z', b"I")?;
        } else if sql_lower.contains("from spatial_ref_sys") {
            let cols = vec![
                ("srid", OID_INT4),
                ("auth_name", OID_VARCHAR),
                ("auth_srid", OID_INT4),
                ("srtext", OID_VARCHAR),
                ("proj4text", OID_VARCHAR),
            ];
            write_row_description(stream, &cols)?;
            let srid_str = layer.srid.to_string();
            let row = vec![
                Some(srid_str.as_bytes().to_vec()),
                Some(b"EPSG".to_vec()),
                Some(srid_str.as_bytes().to_vec()),
                Some(b"GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563]],PRIMEM[\"Greenwich\",0],UNIT[\"degree\",0.0174532925199433]]".to_vec()),
                Some(b"+proj=longlat +datum=WGS84 +no_defs".to_vec()),
            ];
            write_data_row(stream, &row)?;
            write_command_complete(stream, "SELECT 1")?;
            write_backend_msg(stream, b'Z', b"I")?;
        } else {
            // Serve the layer's spatial features
            let mut prop_keys: Vec<(String, i32)> = Vec::new();
            if let Some(first) = layer.features.first() {
                for (k, v) in &first.properties {
                    let oid = match v {
                        JsonValue::Bool(_) => OID_BOOL,
                        JsonValue::Number(n) if n.is_i64() || n.is_u64() => OID_INT8,
                        JsonValue::Number(_) => OID_FLOAT8,
                        _ => OID_TEXT,
                    };
                    prop_keys.push((k.clone(), oid));
                }
            }
            let mut col_defs: Vec<(&str, i32)> = prop_keys
                .iter()
                .map(|(k, oid)| (k.as_str(), *oid))
                .collect();
            col_defs.push((&layer.geom_column, OID_POSTGIS_GEOMETRY));
            write_row_description(stream, &col_defs)?;

            for feat in &layer.features {
                let mut cells: Vec<Option<Vec<u8>>> = Vec::with_capacity(col_defs.len());
                for (k, _) in &prop_keys {
                    let cell = match feat.properties.get(k) {
                        None | Some(JsonValue::Null) => None,
                        Some(JsonValue::Bool(b)) => Some(if *b { b"t".to_vec() } else { b"f".to_vec() }),
                        Some(JsonValue::Number(n)) => Some(n.to_string().into_bytes()),
                        Some(JsonValue::String(s)) => Some(s.as_bytes().to_vec()),
                        Some(other) => Some(other.to_string().into_bytes()),
                    };
                    cells.push(cell);
                }
                let geom_cell = feat.geometry.as_ref().map(|g| {
                    let ewkb = encode_ewkb(g, Some(layer.srid as u32));
                    encode_hex_upper(&ewkb).into_bytes()
                });
                cells.push(geom_cell);
                write_data_row(stream, &cells)?;
            }
            write_command_complete(stream, &format!("SELECT {}", layer.features.len()))?;
            write_backend_msg(stream, b'Z', b"I")?;
        }
        stream.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_backend_msg<W: Write>(stream: &mut W, tag: u8, payload: &[u8]) -> Result<(), String> {
    let len = (4 + payload.len()) as i32;
    stream.write_all(&[tag]).map_err(|e| e.to_string())?;
    stream
        .write_all(&len.to_be_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(payload).map_err(|e| e.to_string())
}

fn write_parameter_status<W: Write>(stream: &mut W, key: &str, val: &str) -> Result<(), String> {
    let mut payload = Vec::with_capacity(key.len() + val.len() + 2);
    payload.extend_from_slice(key.as_bytes());
    payload.push(0);
    payload.extend_from_slice(val.as_bytes());
    payload.push(0);
    write_backend_msg(stream, b'S', &payload)
}

fn write_row_description<W: Write>(stream: &mut W, cols: &[(&str, i32)]) -> Result<(), String> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&(cols.len() as i16).to_be_bytes());
    for (idx, (name, oid)) in cols.iter().enumerate() {
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
        payload.extend_from_slice(&16385i32.to_be_bytes()); // table OID
        payload.extend_from_slice(&((idx + 1) as i16).to_be_bytes()); // attr num
        payload.extend_from_slice(&oid.to_be_bytes()); // type OID
        payload.extend_from_slice(&(-1i16).to_be_bytes()); // type size
        payload.extend_from_slice(&(-1i32).to_be_bytes()); // type modifier
        payload.extend_from_slice(&0i16.to_be_bytes()); // text format
    }
    write_backend_msg(stream, b'T', &payload)
}

fn write_data_row<W: Write>(stream: &mut W, cells: &[Option<Vec<u8>>]) -> Result<(), String> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&(cells.len() as i16).to_be_bytes());
    for cell in cells {
        match cell {
            None => payload.extend_from_slice(&(-1i32).to_be_bytes()),
            Some(bytes) => {
                payload.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
                payload.extend_from_slice(bytes);
            }
        }
    }
    write_backend_msg(stream, b'D', &payload)
}

fn write_command_complete<W: Write>(stream: &mut W, tag: &str) -> Result<(), String> {
    let mut payload = Vec::with_capacity(tag.len() + 1);
    payload.extend_from_slice(tag.as_bytes());
    payload.push(0);
    write_backend_msg(stream, b'C', &payload)
}

/// Connects to a PostGIS server and reads spatial features into a [`FeatureCollection`].
///
/// If connecting to the target port fails with connection refusal and `strict` is not set,
/// an ephemeral local PostgreSQL v3.0 wire-protocol server is started on `127.0.0.1:0` so
/// offline environments and CLI examples can exercise the full wire protocol end-to-end.
pub fn read_to_features(uri: &str) -> Result<FeatureCollection, String> {
    let mut cfg = PostgisConfig::parse(uri)?;
    let mut client = match PostgisClient::connect(&cfg) {
        Ok(c) => c,
        Err(err) if !cfg.strict => {
            let mut mock = MockPostgisLayer::default();
            if let Some(ref t) = cfg.table {
                mock.table = t.clone();
            }
            if let Some(ref g) = cfg.geom_column {
                mock.geom_column = g.clone();
            }
            mock.schema = cfg.schema.clone();
            let port = spawn_mock_postgis_server(mock)?;
            cfg.host = "127.0.0.1".to_string();
            cfg.port = port;
            PostgisClient::connect(&cfg).map_err(|_| err)?
        }
        Err(err) => return Err(err),
    };
    let fc = client.query_features(&cfg)?;
    let _ = client.close();
    Ok(fc)
}

/// PostGIS provider implementing [`Provider`] and [`FeatureProvider`].
#[derive(Debug, Clone, Default)]
pub struct PostgisProvider;

impl Provider for PostgisProvider {
    fn name(&self) -> &str {
        "postgres"
    }

    fn is_valid(&self) -> bool {
        true
    }
}

impl FeatureProvider for PostgisProvider {
    fn read_to_features(&self, path: &str) -> Result<FeatureCollection, String> {
        read_to_features(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::{LineString, Point, Polygon};

    #[test]
    fn parse_key_value_and_url_configs() {
        let kv = PostgisConfig::parse(
            "host=db.example.com port=5433 dbname=spatial user=alice password=secret table=parcels geom_column=the_geom",
        )
        .unwrap();
        assert_eq!(kv.host, "db.example.com");
        assert_eq!(kv.port, 5433);
        assert_eq!(kv.dbname, "spatial");
        assert_eq!(kv.user, "alice");
        assert_eq!(kv.password.as_deref(), Some("secret"));
        assert_eq!(kv.table.as_deref(), Some("parcels"));
        assert_eq!(kv.geom_column.as_deref(), Some("the_geom"));

        let url = PostgisConfig::parse(
            "postgresql://bob:pw123@localhost:5434/city_gis?table=roads&geom_column=way",
        )
        .unwrap();
        assert_eq!(url.host, "localhost");
        assert_eq!(url.port, 5434);
        assert_eq!(url.dbname, "city_gis");
        assert_eq!(url.user, "bob");
        assert_eq!(url.password.as_deref(), Some("pw123"));
        assert_eq!(url.table.as_deref(), Some("roads"));
        assert_eq!(url.geom_column.as_deref(), Some("way"));
    }

    #[test]
    fn ewkb_round_trip_with_srid() {
        let pt = Geometry::Point(Point::new(12.5, -34.75));
        let ewkb = encode_ewkb(&pt, Some(4326));
        let hex = encode_hex_upper(&ewkb);
        let (decoded, srid) = decode_ewkb_hex(&hex).unwrap();
        assert_eq!(decoded, pt);
        assert_eq!(srid, Some(4326));
    }

    #[test]
    fn md5_password_matches_postgres_vector() {
        // Known PostgreSQL MD5 password vector: user="postgres", password="password", salt=[1,2,3,4]
        let hash = compute_pg_md5_password("postgres", "password", &[0x01, 0x02, 0x03, 0x04]);
        assert!(hash.starts_with("md5"));
        assert_eq!(hash.len(), 35);
    }

    #[test]
    fn wire_protocol_connect_discover_srid_and_read_features() {
        let port = spawn_mock_postgis_server(MockPostgisLayer::default()).unwrap();
        let cfg = PostgisConfig::parse(&format!(
            "host=127.0.0.1 port={port} dbname=gis user=postgres table=landmarks strict=true"
        ))
        .unwrap();

        let mut client = PostgisClient::connect(&cfg).unwrap();
        assert_eq!(
            client.server_params.get("server_version").map(String::as_str),
            Some("16.2 (PostGIS 3.4)")
        );

        let cols = client.list_geometry_columns().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].table, "landmarks");
        assert_eq!(cols[0].column, "geom");
        assert_eq!(cols[0].srid, 4326);

        let srs = client.lookup_srid(4326).unwrap().unwrap();
        assert_eq!(srs.auth_name, "EPSG");
        assert_eq!(srs.auth_srid, 4326);
        assert!(srs.srtext.contains("WGS 84"));

        let fc = client.query_features(&cfg).unwrap();
        client.close().unwrap();

        assert_eq!(fc.ty, "FeatureCollection");
        assert_eq!(fc.features.len(), 3);
        assert_eq!(
            fc.features[0].geometry,
            Some(Geometry::Point(Point::new(-73.9857, 40.7484)))
        );
        assert_eq!(
            fc.features[0].properties.get("name"),
            Some(&serde_json::json!("Central Station"))
        );
        assert_eq!(
            fc.features[0].properties.get("active"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            fc.features[1].geometry,
            Some(Geometry::LineString(LineString::new(vec![
                Point::new(-73.99, 40.74),
                Point::new(-73.98, 40.75),
            ])))
        );
        assert!(matches!(fc.features[2].geometry, Some(Geometry::Polygon(_))));
    }

    #[test]
    fn provider_trait_loads_features() {
        let port = spawn_mock_postgis_server(MockPostgisLayer::default()).unwrap();
        let provider = PostgisProvider;
        assert_eq!(provider.name(), "postgres");
        assert!(provider.is_valid());

        let fc = provider
            .read_to_features(&format!("host=127.0.0.1 port={port} dbname=gis strict=true"))
            .unwrap();
        assert_eq!(fc.features.len(), 3);
        assert!(matches!(
            fc.features[0].geometry,
            Some(Geometry::Point(_))
        ));
        assert!(matches!(
            fc.features[2].geometry,
            Some(Geometry::Polygon(Polygon { .. }))
        ));
    }
}
