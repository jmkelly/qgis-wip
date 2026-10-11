//! FastCGI TCP listener.
//!
//! Accepts TCP connections and handles the FastCGI protocol:
//! reads records, decodes requests, routes them, and writes responses.

use bytes::{BufMut, Bytes, BytesMut};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use crate::error::{Result, ServerError};
use crate::fcgi::{
    parse_name_value_pairs, BeginRequestBody, FcgiRequest, FcgiResponse, Record, RecordType,
};
use crate::service;

/// Maximum request body size (16 MB).
const MAX_BODY_SIZE: usize = 16 * 1024 * 1024;

/// Maximum number of concurrent connections.
const MAX_CONNECTIONS: usize = 128;

/// Runs the FastCGI TCP listener.
pub async fn run(config: crate::config::ServerConfig) -> Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("FastCGI listener bound to {}", addr);

    let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        debug!("new FastCGI connection from {}", peer_addr);

        let permit = semaphore.clone().acquire_owned().await.unwrap();

        tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = handle_connection(stream).await {
                error!("error handling connection from {}: {}", peer_addr, e);
            }
        });
    }
}

/// Handles a single FastCGI connection.
async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let mut buf = BytesMut::with_capacity(8192);
    let mut requests: Vec<FcgiRequest> = Vec::new();

    loop {
        // Read data from the socket
        let mut chunk = [0u8; 8192];
        let n = stream.read(&mut chunk).await?;

        if n == 0 {
            // Connection closed
            debug!("connection closed by peer");
            break;
        }

        buf.extend_from_slice(&chunk[..n]);

        // Try to parse records from the buffer
        while !buf.is_empty() {
            // Peek at the header to see if we have enough data
            if buf.len() < crate::fcgi::RecordHeader::HEADER_SIZE {
                break;
            }

            // Parse content length from header
            let content_length = u16::from_be_bytes([buf[4], buf[5]]) as usize;
            let padding_length = buf[6] as usize;
            let total_needed =
                crate::fcgi::RecordHeader::HEADER_SIZE + content_length + padding_length;

            if buf.len() < total_needed {
                break; // Need more data
            }

            // Parse the record
            let record = Record::parse(&mut buf)?;
            debug!(
                "received record: type={:?}, request_id={}, content_len={}",
                record.header.record_type, record.header.request_id, record.header.content_length
            );

            match record.header.record_type {
                RecordType::BeginRequest => {
                    let begin_body = BeginRequestBody::parse(&record.content)?;
                    debug!(
                        "begin request: role={:?}, flags={}",
                        begin_body.role, begin_body.flags
                    );

                    // Only support responder role
                    if begin_body.role != crate::fcgi::Role::Responder {
                        warn!("unsupported role: {:?}", begin_body.role);
                        // Send end request with unknown role
                        let end_record = make_end_request(
                            record.header.request_id,
                            crate::fcgi::ProtocolStatus::UnknownRole,
                        );
                        let mut out = BytesMut::new();
                        end_record.write(&mut out);
                        stream.write_all(&out).await?;
                    }
                }
                RecordType::AbortRequest => {
                    debug!("abort request: id={}", record.header.request_id);
                    // Remove any pending request with this ID
                    requests.retain(|r| r.request_id != record.header.request_id);
                }
                RecordType::Params => {
                    if record.content.is_empty() {
                        // Empty PARAMS signals end of params
                        // Find the request and process it
                        if requests
                            .iter()
                            .any(|r| r.request_id == record.header.request_id)
                        {
                            let req = requests
                                .iter()
                                .find(|r| r.request_id == record.header.request_id)
                                .cloned()
                                .unwrap();

                            let response = process_request(&req)?;
                            send_response(&mut stream, &response, req.request_id).await?;

                            // Remove the request
                            requests.retain(|r| r.request_id != record.header.request_id);
                        }
                    } else {
                        // Parse name-value pairs
                        let pairs = parse_name_value_pairs(record.content)?;

                        // Find or create the request
                        if let Some(req) = requests
                            .iter_mut()
                            .find(|r| r.request_id == record.header.request_id)
                        {
                            req.params.extend(pairs);
                        } else {
                            requests.push(FcgiRequest {
                                request_id: record.header.request_id,
                                params: pairs,
                                body: Bytes::new(),
                            });
                        }
                    }
                }
                RecordType::Stdin => {
                    if record.content.is_empty() {
                        // Empty STDIN signals end of body
                        // If there's no PARAMS data, process now
                        if let Some(req) = requests
                            .iter()
                            .find(|r| r.request_id == record.header.request_id)
                        {
                            let req = req.clone();
                            let response = process_request(&req)?;
                            send_response(&mut stream, &response, req.request_id).await?;
                            requests.retain(|r| r.request_id != record.header.request_id);
                        }
                    } else {
                        // Append to request body
                        if let Some(req) = requests
                            .iter_mut()
                            .find(|r| r.request_id == record.header.request_id)
                        {
                            let new_len = req.body.len() + record.content.len();
                            if new_len > MAX_BODY_SIZE {
                                return Err(ServerError::BodyTooLarge(new_len));
                            }
                            let mut body = BytesMut::from(&req.body[..]);
                            body.extend_from_slice(&record.content);
                            req.body = body.freeze();
                        }
                    }
                }
                RecordType::Data => {
                    // DATA records are for filter role — ignore for responder
                    debug!("ignoring DATA record");
                }
                RecordType::GetValues => {
                    // Respond to GET_VALUES with minimal info
                    let response = make_get_values_response(record.header.request_id);
                    let mut out = BytesMut::new();
                    response.write(&mut out);
                    stream.write_all(&out).await?;
                }
                _ => {
                    warn!("unexpected record type: {:?}", record.header.record_type);
                }
            }
        }
    }

    Ok(())
}

/// Processes a FastCGI request and returns a response.
fn process_request(request: &FcgiRequest) -> Result<FcgiResponse> {
    debug!(
        "processing request: id={}, method={}, uri={}",
        request.request_id,
        request.method(),
        request.request_uri()
    );

    service::route(request)
}

/// Sends a FastCGI response over the stream.
async fn send_response(
    stream: &mut TcpStream,
    response: &FcgiResponse,
    request_id: u16,
) -> Result<()> {
    let records = response.to_records(request_id);
    let mut out = BytesMut::new();

    for record in &records {
        record.write(&mut out);
    }

    stream.write_all(&out).await?;
    stream.flush().await?;

    debug!("sent response: {} records, {} bytes", records.len(), out.len());

    Ok(())
}

/// Creates an END_REQUEST record.
fn make_end_request(request_id: u16, status: crate::fcgi::ProtocolStatus) -> Record {
    let mut content = BytesMut::with_capacity(8);
    content.put_u32(0); // app_status
    content.put_u8(status as u8);
    content.put_u8(0); // reserved
    content.put_u8(0); // reserved
    content.put_u8(0); // reserved

    Record::new(RecordType::EndRequest, request_id, content.freeze())
}

/// Creates a GET_VALUES_RESULT record.
fn make_get_values_response(request_id: u16) -> Record {
    let mut content = BytesMut::new();

    // FCGI_MAX_CONNS
    content.put_u8(11); // name length
    content.put_u8(1); // value length
    content.put_slice(b"FCGI_MAX_CONNS");
    content.put_u8(128.to_string().len() as u8);
    content.put_slice(128.to_string().as_bytes());

    // FCGI_MAX_REQS
    content.put_u8(11); // name length
    content.put_u8(1); // value length
    content.put_slice(b"FCGI_MAX_REQS");
    content.put_u8(128.to_string().len() as u8);
    content.put_slice(128.to_string().as_bytes());

    // FCGI_MPXS_CONNS
    content.put_u8(14); // name length
    content.put_u8(1); // value length
    content.put_slice(b"FCGI_MPXS_CONNS");
    content.put_u8(1);
    content.put_u8(b'0');

    Record::new(RecordType::GetValuesResult, request_id, content.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_end_request_record() {
        let record = make_end_request(1, crate::fcgi::ProtocolStatus::RequestComplete);
        assert_eq!(record.header.record_type, RecordType::EndRequest);
        assert_eq!(record.header.request_id, 1);
        assert_eq!(record.content.len(), 8);
    }

    #[test]
    fn make_get_values_response_record() {
        let record = make_get_values_response(1);
        assert_eq!(record.header.record_type, RecordType::GetValuesResult);
        assert_eq!(record.header.request_id, 1);
        assert!(!record.content.is_empty());
    }

    #[test]
    fn max_body_size_constant() {
        assert_eq!(MAX_BODY_SIZE, 16 * 1024 * 1024);
    }

    #[test]
    fn max_connections_constant() {
        assert_eq!(MAX_CONNECTIONS, 128);
    }
}
