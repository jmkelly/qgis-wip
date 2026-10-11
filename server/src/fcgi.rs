//! FastCGI protocol implementation.
//!
//! Handles FastCGI record parsing, name-value pair encoding/decoding,
//! and conversion between FastCGI and HTTP representations.
//!
//! Reference: https://fastcgi-archives.github.io/FastCGI_Specification.html

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::collections::HashMap;

use crate::error::{Result, ServerError};

/// FastCGI protocol version.
pub const FCGI_VERSION_1: u8 = 1;

/// FastCGI record types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    /// Begin request record.
    BeginRequest = 1,
    /// Abort request record.
    AbortRequest = 2,
    /// End request record.
    EndRequest = 3,
    /// Parameters record (name-value pairs).
    Params = 4,
    /// Standard input record (request body).
    Stdin = 5,
    /// Standard output record (response body).
    Stdout = 6,
    /// Standard error record.
    Stderr = 7,
    /// Data record.
    Data = 8,
    /// Get values record.
    GetValues = 9,
    /// Get values result record.
    GetValuesResult = 10,
    /// Unknown type record.
    UnknownType = 11,
}

impl RecordType {
    /// Converts a u8 to a RecordType.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::BeginRequest),
            2 => Some(Self::AbortRequest),
            3 => Some(Self::EndRequest),
            4 => Some(Self::Params),
            5 => Some(Self::Stdin),
            6 => Some(Self::Stdout),
            7 => Some(Self::Stderr),
            8 => Some(Self::Data),
            9 => Some(Self::GetValues),
            10 => Some(Self::GetValuesResult),
            11 => Some(Self::UnknownType),
            _ => None,
        }
    }
}

/// FastCGI roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Responder role — generates HTTP responses.
    Responder = 1,
    /// Authorizer role.
    Authorizer = 2,
    /// Filter role.
    Filter = 3,
}

impl Role {
    /// Converts a u16 to a Role.
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Responder),
            2 => Some(Self::Authorizer),
            3 => Some(Self::Filter),
            _ => None,
        }
    }
}

/// FastCGI protocol status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolStatus {
    /// Request completed normally.
    RequestComplete = 0,
    /// This app can't multiplex connections.
    CantMpxConn = 1,
    /// New request rejected; app is overloaded.
    Overloaded = 2,
    /// Role value not known.
    UnknownRole = 3,
}

impl ProtocolStatus {
    /// Converts a u8 to a ProtocolStatus.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::RequestComplete),
            1 => Some(Self::CantMpxConn),
            2 => Some(Self::Overloaded),
            3 => Some(Self::UnknownRole),
            _ => None,
        }
    }
}

/// A FastCGI record header.
#[derive(Debug, Clone, Copy)]
pub struct RecordHeader {
    /// Protocol version (always 1).
    pub version: u8,
    /// Record type.
    pub record_type: RecordType,
    /// Request ID.
    pub request_id: u16,
    /// Content length in bytes.
    pub content_length: u16,
    /// Padding length in bytes.
    pub padding_length: u8,
}

impl RecordHeader {
    /// Size of a FastCGI record header in bytes.
    pub const HEADER_SIZE: usize = 8;

    /// Parses a record header from a byte buffer.
    pub fn parse(buf: &mut BytesMut) -> Result<Self> {
        if buf.len() < Self::HEADER_SIZE {
            return Err(ServerError::InvalidRecord("buffer too small for header"));
        }

        let version = buf.get_u8();
        if version != FCGI_VERSION_1 {
            return Err(ServerError::InvalidRecord("unsupported version"));
        }

        let type_byte = buf.get_u8();
        let record_type = RecordType::from_u8(type_byte)
            .ok_or(ServerError::UnknownRecordType(type_byte))?;
        let request_id = buf.get_u16();
        let content_length = buf.get_u16();
        let padding_length = buf.get_u8();
        let _reserved = buf.get_u8();

        Ok(Self {
            version,
            record_type,
            request_id,
            content_length,
            padding_length,
        })
    }

    /// Writes the record header to a byte buffer.
    pub fn write(&self, buf: &mut BytesMut) {
        buf.put_u8(self.version);
        buf.put_u8(self.record_type as u8);
        buf.put_u16(self.request_id);
        buf.put_u16(self.content_length);
        buf.put_u8(self.padding_length);
        buf.put_u8(0); // reserved
    }
}

/// A complete FastCGI record (header + content + padding).
#[derive(Debug, Clone)]
pub struct Record {
    /// Record header.
    pub header: RecordHeader,
    /// Record content.
    pub content: Bytes,
}

impl Record {
    /// Creates a new record.
    pub fn new(record_type: RecordType, request_id: u16, content: Bytes) -> Self {
        let content_length = content.len() as u16;
        let padding_length = ((8 - (content_length % 8)) % 8) as u8;
        Self {
            header: RecordHeader {
                version: FCGI_VERSION_1,
                record_type,
                request_id,
                content_length,
                padding_length,
            },
            content,
        }
    }

    /// Parses a record from a byte buffer.
    pub fn parse(buf: &mut BytesMut) -> Result<Self> {
        let header = RecordHeader::parse(buf)?;

        let total_len = header.content_length as usize + header.padding_length as usize;
        if buf.len() < total_len {
            return Err(ServerError::InvalidRecord("buffer too small for content"));
        }

        let content = buf.split_to(header.content_length as usize).freeze();
        // Skip padding
        buf.advance(header.padding_length as usize);

        Ok(Self { header, content })
    }

    /// Writes the record to a byte buffer.
    pub fn write(&self, buf: &mut BytesMut) {
        self.header.write(buf);
        buf.put_slice(&self.content);
        // Write padding
        for _ in 0..self.header.padding_length {
            buf.put_u8(0);
        }
    }
}

/// FastCGI begin request body.
#[derive(Debug, Clone, Copy)]
pub struct BeginRequestBody {
    /// Role.
    pub role: Role,
    /// Flags.
    pub flags: u8,
}

impl BeginRequestBody {
    /// Size of the begin request body.
    pub const BODY_SIZE: usize = 8;

    /// Parses a begin request body from bytes.
    pub fn parse(content: &Bytes) -> Result<Self> {
        if content.len() < Self::BODY_SIZE {
            return Err(ServerError::InvalidRecord("begin request body too small"));
        }

        let role = Role::from_u16(u16::from_be_bytes([content[0], content[1]]))
            .ok_or(ServerError::InvalidRecord("unknown role"))?;
        let flags = content[2];

        Ok(Self { role, flags })
    }
}

/// FastCGI end request body.
#[derive(Debug, Clone, Copy)]
pub struct EndRequestBody {
    /// Application status code.
    pub app_status: u32,
    /// Protocol status.
    pub protocol_status: ProtocolStatus,
}

impl EndRequestBody {
    /// Size of the end request body.
    pub const BODY_SIZE: usize = 8;

    /// Creates a new end request body.
    pub fn new(app_status: u32, protocol_status: ProtocolStatus) -> Self {
        Self {
            app_status,
            protocol_status,
        }
    }

    /// Writes the end request body to a byte buffer.
    pub fn write(&self, buf: &mut BytesMut) {
        buf.put_u32(self.app_status);
        buf.put_u8(self.protocol_status as u8);
        buf.put_u8(0); // reserved
        buf.put_u8(0); // reserved
        buf.put_u8(0); // reserved
    }
}

/// FastCGI name-value pair map.
pub type NameValuePairs = HashMap<String, String>;

/// Parses FastCGI name-value pairs from a byte buffer.
///
/// Each name-value pair is encoded as:
/// - name length (1 byte if < 128, else 4 bytes with high bit set)
/// - value length (same encoding)
/// - name bytes
/// - value bytes
pub fn parse_name_value_pairs(mut data: Bytes) -> Result<NameValuePairs> {
    let mut pairs = HashMap::new();

    while !data.is_empty() {
        let (name_len, name_len_bytes) = read_length(&data)?;
        data.advance(name_len_bytes);

        let (value_len, value_len_bytes) = read_length(&data)?;
        data.advance(value_len_bytes);

        if data.len() < name_len + value_len {
            return Err(ServerError::InvalidNameValuePair);
        }

        let name = String::from_utf8_lossy(&data[..name_len]).to_string();
        data.advance(name_len);

        let value = String::from_utf8_lossy(&data[..value_len]).to_string();
        data.advance(value_len);

        pairs.insert(name, value);
    }

    Ok(pairs)
}

/// Reads a FastCGI length value (1 or 4 bytes).
fn read_length(data: &Bytes) -> Result<(usize, usize)> {
    if data.is_empty() {
        return Err(ServerError::InvalidNameValuePair);
    }

    let first = data[0];
    if first & 0x80 == 0 {
        // 1-byte length
        Ok((first as usize, 1))
    } else {
        // 4-byte length
        if data.len() < 4 {
            return Err(ServerError::InvalidNameValuePair);
        }
        let len = ((first as usize & 0x7f) << 24)
            | (data[1] as usize) << 16
            | (data[2] as usize) << 8
            | data[3] as usize;
        Ok((len, 4))
    }
}

/// Encodes name-value pairs into a byte buffer.
pub fn encode_name_value_pairs(pairs: &NameValuePairs) -> BytesMut {
    let mut buf = BytesMut::new();

    for (name, value) in pairs {
        write_length(&mut buf, name.len());
        write_length(&mut buf, value.len());
        buf.put_slice(name.as_bytes());
        buf.put_slice(value.as_bytes());
    }

    buf
}

/// Writes a FastCGI length value.
fn write_length(buf: &mut BytesMut, len: usize) {
    if len < 128 {
        buf.put_u8(len as u8);
    } else {
        buf.put_u8(((len >> 24) & 0x7f) as u8 | 0x80);
        buf.put_u8((len >> 16) as u8);
        buf.put_u8((len >> 8) as u8);
        buf.put_u8(len as u8);
    }
}

/// A FastCGI request, decoded from records.
#[derive(Debug, Clone)]
pub struct FcgiRequest {
    /// Request ID.
    pub request_id: u16,
    /// CGI/1.1 environment variables (from PARAMS records).
    pub params: NameValuePairs,
    /// Request body (from STDIN records).
    pub body: Bytes,
}

impl FcgiRequest {
    /// Returns the HTTP method from the REQUEST_METHOD param.
    pub fn method(&self) -> &str {
        self.params
            .get("REQUEST_METHOD")
            .map(|s| s.as_str())
            .unwrap_or("GET")
    }

    /// Returns the query string from the QUERY_STRING param.
    pub fn query_string(&self) -> &str {
        self.params.get("QUERY_STRING").map(|s| s.as_str()).unwrap_or("")
    }

    /// Returns the request URI from the REQUEST_URI param.
    pub fn request_uri(&self) -> &str {
        self.params
            .get("REQUEST_URI")
            .map(|s| s.as_str())
            .unwrap_or("/")
    }

    /// Returns the content type from the CONTENT_TYPE param.
    pub fn content_type(&self) -> Option<&str> {
        self.params.get("CONTENT_TYPE").map(|s| s.as_str())
    }

    /// Returns the content length from the CONTENT_LENGTH param.
    pub fn content_length(&self) -> Option<usize> {
        self.params
            .get("CONTENT_LENGTH")
            .and_then(|s| s.parse().ok())
    }

    /// Returns a header value (HTTP_* params).
    pub fn header(&self, name: &str) -> Option<&str> {
        let key = format!("HTTP_{}", name.to_uppercase().replace('-', "_"));
        self.params.get(&key).map(|s| s.as_str())
    }
}

/// A FastCGI response, to be encoded as records.
#[derive(Debug, Clone)]
pub struct FcgiResponse {
    /// Response body (goes to STDOUT).
    pub body: Bytes,
    /// Error output (goes to STDERR).
    pub stderr: Bytes,
    /// Application status code.
    pub app_status: u32,
}

impl FcgiResponse {
    /// Creates a new response.
    pub fn new(body: Bytes) -> Self {
        Self {
            body,
            stderr: Bytes::new(),
            app_status: 0,
        }
    }

    /// Creates a response with a specific status code.
    pub fn with_status(body: Bytes, app_status: u32) -> Self {
        Self {
            body,
            stderr: Bytes::new(),
            app_status,
        }
    }

    /// Encodes the response as FastCGI records.
    pub fn to_records(&self, request_id: u16) -> Vec<Record> {
        let mut records = Vec::new();

        // STDOUT records
        if !self.body.is_empty() {
            records.push(Record::new(RecordType::Stdout, request_id, self.body.clone()));
        }

        // Empty STDOUT to signal end of output
        records.push(Record::new(RecordType::Stdout, request_id, Bytes::new()));

        // STDERR records
        if !self.stderr.is_empty() {
            records.push(Record::new(RecordType::Stderr, request_id, self.stderr.clone()));
        }

        // Empty STDERR to signal end of error output
        records.push(Record::new(RecordType::Stderr, request_id, Bytes::new()));

        // END_REQUEST record
        let mut end_body = BytesMut::with_capacity(EndRequestBody::BODY_SIZE);
        EndRequestBody::new(self.app_status, ProtocolStatus::RequestComplete).write(&mut end_body);
        records.push(Record::new(
            RecordType::EndRequest,
            request_id,
            end_body.freeze(),
        ));

        records
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_type_from_u8() {
        assert_eq!(RecordType::from_u8(1), Some(RecordType::BeginRequest));
        assert_eq!(RecordType::from_u8(4), Some(RecordType::Params));
        assert_eq!(RecordType::from_u8(6), Some(RecordType::Stdout));
        assert_eq!(RecordType::from_u8(99), None);
    }

    #[test]
    fn role_from_u16() {
        assert_eq!(Role::from_u16(1), Some(Role::Responder));
        assert_eq!(Role::from_u16(2), Some(Role::Authorizer));
        assert_eq!(Role::from_u16(3), Some(Role::Filter));
        assert_eq!(Role::from_u16(99), None);
    }

    #[test]
    fn protocol_status_from_u8() {
        assert_eq!(ProtocolStatus::from_u8(0), Some(ProtocolStatus::RequestComplete));
        assert_eq!(ProtocolStatus::from_u8(1), Some(ProtocolStatus::CantMpxConn));
        assert_eq!(ProtocolStatus::from_u8(99), None);
    }

    #[test]
    fn parse_name_value_pairs_empty() {
        let data = Bytes::new();
        let pairs = parse_name_value_pairs(data).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn parse_name_value_pairs_single() {
        let mut buf = BytesMut::new();
        buf.put_u8(3); // name length
        buf.put_u8(5); // value length
        buf.put_slice(b"foo");
        buf.put_slice(b"hello");

        let pairs = parse_name_value_pairs(buf.freeze()).unwrap();
        assert_eq!(pairs.get("foo").unwrap(), "hello");
    }

    #[test]
    fn parse_name_value_pairs_multiple() {
        let mut buf = BytesMut::new();
        // First pair: "a" = "1"
        buf.put_u8(1);
        buf.put_u8(1);
        buf.put_slice(b"a");
        buf.put_slice(b"1");
        // Second pair: "bb" = "22"
        buf.put_u8(2);
        buf.put_u8(2);
        buf.put_slice(b"bb");
        buf.put_slice(b"22");

        let pairs = parse_name_value_pairs(buf.freeze()).unwrap();
        assert_eq!(pairs.get("a").unwrap(), "1");
        assert_eq!(pairs.get("bb").unwrap(), "22");
    }

    #[test]
    fn parse_name_value_pairs_long_length() {
        let mut buf = BytesMut::new();
        // Name length 200 (requires 4-byte encoding)
        buf.put_u8(0x80 | 0);
        buf.put_u8(0);
        buf.put_u8(0);
        buf.put_u8(200);
        // Value length 1
        buf.put_u8(1);
        // Name: 200 bytes
        for _ in 0..200 {
            buf.put_u8(b'x');
        }
        // Value: "y"
        buf.put_u8(b'y');

        let pairs = parse_name_value_pairs(buf.freeze()).unwrap();
        let name = "x".repeat(200);
        assert_eq!(pairs.get(&name).unwrap(), "y");
    }

    #[test]
    fn encode_name_value_pairs_roundtrip() {
        let mut pairs = NameValuePairs::new();
        pairs.insert("key1".to_string(), "value1".to_string());
        pairs.insert("key2".to_string(), "value2".to_string());

        let encoded = encode_name_value_pairs(&pairs);
        let decoded = parse_name_value_pairs(encoded.freeze()).unwrap();

        assert_eq!(decoded.get("key1").unwrap(), "value1");
        assert_eq!(decoded.get("key2").unwrap(), "value2");
    }

    #[test]
    fn record_roundtrip() {
        let content = Bytes::from_static(b"test content");
        let record = Record::new(RecordType::Stdout, 42, content.clone());

        let mut buf = BytesMut::new();
        record.write(&mut buf);

        let mut read_buf = BytesMut::from(&buf[..]);
        let parsed = Record::parse(&mut read_buf).unwrap();

        assert_eq!(parsed.header.record_type, RecordType::Stdout);
        assert_eq!(parsed.header.request_id, 42);
        assert_eq!(parsed.content, content);
    }

    #[test]
    fn fcgi_request_accessors() {
        let mut params = NameValuePairs::new();
        params.insert("REQUEST_METHOD".to_string(), "GET".to_string());
        params.insert("QUERY_STRING".to_string(), "SERVICE=WMS&REQUEST=GetCapabilities".to_string());
        params.insert("REQUEST_URI".to_string(), "/cgi-bin/qgis_mapserv.fcgi".to_string());
        params.insert("CONTENT_TYPE".to_string(), "application/xml".to_string());
        params.insert("CONTENT_LENGTH".to_string(), "100".to_string());
        params.insert("HTTP_HOST".to_string(), "example.com".to_string());

        let req = FcgiRequest {
            request_id: 1,
            params,
            body: Bytes::new(),
        };

        assert_eq!(req.method(), "GET");
        assert_eq!(req.query_string(), "SERVICE=WMS&REQUEST=GetCapabilities");
        assert_eq!(req.request_uri(), "/cgi-bin/qgis_mapserv.fcgi");
        assert_eq!(req.content_type(), Some("application/xml"));
        assert_eq!(req.content_length(), Some(100));
        assert_eq!(req.header("host"), Some("example.com"));
    }

    #[test]
    fn fcgi_response_to_records() {
        let response = FcgiResponse::new(Bytes::from_static(b"hello"));
        let records = response.to_records(1);

        // Should have: STDOUT (body), STDOUT (empty), STDERR (empty), END_REQUEST
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].header.record_type, RecordType::Stdout);
        assert_eq!(records[0].content, Bytes::from_static(b"hello"));
        assert_eq!(records[1].header.record_type, RecordType::Stdout);
        assert!(records[1].content.is_empty());
        assert_eq!(records[2].header.record_type, RecordType::Stderr);
        assert!(records[2].content.is_empty());
        assert_eq!(records[3].header.record_type, RecordType::EndRequest);
    }

    #[test]
    fn begin_request_body_parse() {
        let content = Bytes::from_static(&[0, 1, 0, 0, 0, 0, 0, 0]);
        let body = BeginRequestBody::parse(&content).unwrap();
        assert_eq!(body.role, Role::Responder);
        assert_eq!(body.flags, 0);
    }

    #[test]
    fn end_request_body_write() {
        let body = EndRequestBody::new(0, ProtocolStatus::RequestComplete);
        let mut buf = BytesMut::new();
        body.write(&mut buf);
        assert_eq!(buf.len(), 8);
        assert_eq!(&buf[0..4], &[0, 0, 0, 0]); // app_status
        assert_eq!(buf[4], 0); // protocol_status
    }
}
