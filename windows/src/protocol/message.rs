use std::{error::Error, fmt};

use super::{Frame, FrameError, MessageType, PROTOCOL_VERSION};
use crate::config::{MAXIMUM_ENDPOINT_NAME_LENGTH, MAXIMUM_ENDPOINTS_PER_FLOW};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelloResponse {
    pub selected_version: u8,
    pub device_type: u8,
    pub firmware_major: u16,
    pub firmware_minor: u16,
    pub firmware_patch: u16,
    pub capabilities: u32,
    pub echoed_host_nonce: u32,
    pub boot_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointRecord {
    pub generation: u32,
    pub handle: u16,
    pub friendly_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoleDefaults {
    pub generation: u32,
    pub console: u16,
    pub multimedia: u16,
    pub communications: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetRequest {
    pub request_id: u32,
    pub generation: u32,
    pub handle: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetResult {
    pub request_id: u32,
    pub operation: u8,
    pub status: u8,
    pub error_code: u16,
    pub known_generation: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorMessage {
    pub error_code: u16,
    pub related_message_type: u8,
    pub context_id: u32,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message {
    HelloRequest {
        minimum_version: u8,
        maximum_version: u8,
        capabilities: u32,
        host_nonce: u32,
    },
    HelloResponse(HelloResponse),
    SyncRequest {
        reason: u8,
    },
    SyncBegin {
        generation: u32,
        output_count: u16,
        input_count: u16,
    },
    OutputEndpoint(EndpointRecord),
    InputEndpoint(EndpointRecord),
    CurrentOutput(RoleDefaults),
    CurrentInput(RoleDefaults),
    SyncEnd {
        generation: u32,
    },
    SetOutputRequest(SetRequest),
    SetInputRequest(SetRequest),
    SetResult(SetResult),
    Ping {
        token: u32,
    },
    Pong {
        token: u32,
    },
    Error(ErrorMessage),
}

impl Message {
    pub fn encode(&self) -> Result<Frame, CodecError> {
        let (kind, payload) = match self {
            Self::HelloRequest {
                minimum_version,
                maximum_version,
                capabilities,
                host_nonce,
            } => {
                let mut p = vec![*minimum_version, *maximum_version];
                put_u32(&mut p, *capabilities);
                put_u32(&mut p, *host_nonce);
                put_u16(&mut p, 0);
                (MessageType::HelloRequest, p)
            }
            Self::HelloResponse(v) => {
                let mut p = vec![v.selected_version, v.device_type];
                put_u16(&mut p, v.firmware_major);
                put_u16(&mut p, v.firmware_minor);
                put_u16(&mut p, v.firmware_patch);
                put_u32(&mut p, v.capabilities);
                put_u32(&mut p, v.echoed_host_nonce);
                put_u32(&mut p, v.boot_id);
                (MessageType::HelloResponse, p)
            }
            Self::SyncRequest { reason } => {
                validate_range("sync reason", *reason, 0, 2)?;
                (MessageType::SyncRequest, vec![*reason])
            }
            Self::SyncBegin {
                generation,
                output_count,
                input_count,
            } => {
                validate_generation(*generation)?;
                if usize::from(*output_count) > MAXIMUM_ENDPOINTS_PER_FLOW
                    || usize::from(*input_count) > MAXIMUM_ENDPOINTS_PER_FLOW
                {
                    return Err(CodecError::InvalidValue("endpoint count"));
                }
                let mut p = Vec::new();
                put_u32(&mut p, *generation);
                put_u16(&mut p, *output_count);
                put_u16(&mut p, *input_count);
                (MessageType::SyncBegin, p)
            }
            Self::OutputEndpoint(v) => (MessageType::OutputEndpoint, encode_endpoint(v)?),
            Self::InputEndpoint(v) => (MessageType::InputEndpoint, encode_endpoint(v)?),
            Self::CurrentOutput(v) => (MessageType::CurrentOutput, encode_roles(v)?),
            Self::CurrentInput(v) => (MessageType::CurrentInput, encode_roles(v)?),
            Self::SyncEnd { generation } => {
                validate_generation(*generation)?;
                let mut p = Vec::new();
                put_u32(&mut p, *generation);
                (MessageType::SyncEnd, p)
            }
            Self::SetOutputRequest(v) => (MessageType::SetOutputRequest, encode_set_request(v)?),
            Self::SetInputRequest(v) => (MessageType::SetInputRequest, encode_set_request(v)?),
            Self::SetResult(v) => {
                validate_nonzero("request id", v.request_id)?;
                validate_range("operation", v.operation, 0, 1)?;
                validate_range("set status", v.status, 0, 4)?;
                validate_generation(v.known_generation)?;
                if (v.status == 0) != (v.error_code == 0) {
                    return Err(CodecError::InvalidValue("set result error code"));
                }
                let mut p = Vec::new();
                put_u32(&mut p, v.request_id);
                p.extend([v.operation, v.status]);
                put_u16(&mut p, v.error_code);
                put_u32(&mut p, v.known_generation);
                (MessageType::SetResult, p)
            }
            Self::Ping { token } => (MessageType::Ping, token.to_le_bytes().to_vec()),
            Self::Pong { token } => (MessageType::Pong, token.to_le_bytes().to_vec()),
            Self::Error(v) => {
                if v.error_code == 0 || v.error_code > 0x10 {
                    return Err(CodecError::InvalidValue("error code"));
                }
                let detail = v.detail.as_bytes();
                if detail.len() > 240 {
                    return Err(CodecError::StringTooLong(detail.len()));
                }
                let mut p = Vec::new();
                put_u16(&mut p, v.error_code);
                p.extend([v.related_message_type, 0]);
                put_u32(&mut p, v.context_id);
                put_u16(&mut p, detail.len() as u16);
                p.extend_from_slice(detail);
                (MessageType::Error, p)
            }
        };
        Frame::new(kind as u8, payload).map_err(CodecError::Frame)
    }

    pub fn decode(frame: &Frame) -> Result<Self, CodecError> {
        if frame.version != PROTOCOL_VERSION {
            return Err(CodecError::UnsupportedVersion(frame.version));
        }
        let kind =
            MessageType::try_from(frame.message_type).map_err(CodecError::UnknownMessageType)?;
        let p = &frame.payload;
        Ok(match kind {
            MessageType::HelloRequest => {
                exact(p, 12)?;
                if p[10] != 0 || p[11] != 0 {
                    return Err(CodecError::ReservedNotZero);
                };
                Self::HelloRequest {
                    minimum_version: p[0],
                    maximum_version: p[1],
                    capabilities: u32_at(p, 2),
                    host_nonce: u32_at(p, 6),
                }
            }
            MessageType::HelloResponse => {
                exact(p, 20)?;
                Self::HelloResponse(HelloResponse {
                    selected_version: p[0],
                    device_type: p[1],
                    firmware_major: u16_at(p, 2),
                    firmware_minor: u16_at(p, 4),
                    firmware_patch: u16_at(p, 6),
                    capabilities: u32_at(p, 8),
                    echoed_host_nonce: u32_at(p, 12),
                    boot_id: u32_at(p, 16),
                })
            }
            MessageType::SyncRequest => {
                exact(p, 1)?;
                validate_range("sync reason", p[0], 0, 2)?;
                Self::SyncRequest { reason: p[0] }
            }
            MessageType::SyncBegin => {
                exact(p, 8)?;
                let g = u32_at(p, 0);
                validate_generation(g)?;
                let o = u16_at(p, 4);
                let i = u16_at(p, 6);
                if usize::from(o) > 64 || usize::from(i) > 64 {
                    return Err(CodecError::InvalidValue("endpoint count"));
                }
                Self::SyncBegin {
                    generation: g,
                    output_count: o,
                    input_count: i,
                }
            }
            MessageType::OutputEndpoint => Self::OutputEndpoint(decode_endpoint(p)?),
            MessageType::InputEndpoint => Self::InputEndpoint(decode_endpoint(p)?),
            MessageType::CurrentOutput => Self::CurrentOutput(decode_roles(p)?),
            MessageType::CurrentInput => Self::CurrentInput(decode_roles(p)?),
            MessageType::SyncEnd => {
                exact(p, 4)?;
                let g = u32_at(p, 0);
                validate_generation(g)?;
                Self::SyncEnd { generation: g }
            }
            MessageType::SetOutputRequest => Self::SetOutputRequest(decode_set_request(p)?),
            MessageType::SetInputRequest => Self::SetInputRequest(decode_set_request(p)?),
            MessageType::SetResult => {
                exact(p, 12)?;
                let v = SetResult {
                    request_id: u32_at(p, 0),
                    operation: p[4],
                    status: p[5],
                    error_code: u16_at(p, 6),
                    known_generation: u32_at(p, 8),
                };
                validate_nonzero("request id", v.request_id)?;
                validate_range("operation", v.operation, 0, 1)?;
                validate_range("set status", v.status, 0, 4)?;
                validate_generation(v.known_generation)?;
                if (v.status == 0) != (v.error_code == 0) {
                    return Err(CodecError::InvalidValue("set result error code"));
                }
                Self::SetResult(v)
            }
            MessageType::Ping => {
                exact(p, 4)?;
                Self::Ping {
                    token: u32_at(p, 0),
                }
            }
            MessageType::Pong => {
                exact(p, 4)?;
                Self::Pong {
                    token: u32_at(p, 0),
                }
            }
            MessageType::Error => {
                if p.len() < 10 {
                    return Err(CodecError::InvalidLength {
                        expected: 10,
                        actual: p.len(),
                    });
                }
                if p[3] != 0 {
                    return Err(CodecError::ReservedNotZero);
                };
                let code = u16_at(p, 0);
                if code == 0 || code > 0x10 {
                    return Err(CodecError::InvalidValue("error code"));
                }
                let n = usize::from(u16_at(p, 8));
                if n > 240 || p.len() != 10 + n {
                    return Err(CodecError::InvalidLength {
                        expected: 10 + n,
                        actual: p.len(),
                    });
                }
                let detail = decode_utf8(&p[10..])?;
                Self::Error(ErrorMessage {
                    error_code: code,
                    related_message_type: p[2],
                    context_id: u32_at(p, 4),
                    detail,
                })
            }
        })
    }
}

fn encode_endpoint(v: &EndpointRecord) -> Result<Vec<u8>, CodecError> {
    validate_generation(v.generation)?;
    if v.handle == 0 {
        return Err(CodecError::InvalidValue("endpoint handle"));
    }
    let name = v.friendly_name.as_bytes();
    if name.is_empty() {
        return Err(CodecError::InvalidValue("endpoint name"));
    }
    if name.len() > MAXIMUM_ENDPOINT_NAME_LENGTH {
        return Err(CodecError::StringTooLong(name.len()));
    }
    let mut p = Vec::new();
    put_u32(&mut p, v.generation);
    put_u16(&mut p, v.handle);
    put_u16(&mut p, name.len() as u16);
    p.extend_from_slice(name);
    Ok(p)
}
fn decode_endpoint(p: &[u8]) -> Result<EndpointRecord, CodecError> {
    if p.len() < 8 {
        return Err(CodecError::InvalidLength {
            expected: 8,
            actual: p.len(),
        });
    }
    let n = usize::from(u16_at(p, 6));
    if n == 0 || n > MAXIMUM_ENDPOINT_NAME_LENGTH || p.len() != 8 + n {
        return Err(CodecError::InvalidLength {
            expected: 8 + n,
            actual: p.len(),
        });
    }
    let generation = u32_at(p, 0);
    validate_generation(generation)?;
    let handle = u16_at(p, 4);
    if handle == 0 {
        return Err(CodecError::InvalidValue("endpoint handle"));
    }
    Ok(EndpointRecord {
        generation,
        handle,
        friendly_name: decode_utf8(&p[8..])?,
    })
}
fn encode_roles(v: &RoleDefaults) -> Result<Vec<u8>, CodecError> {
    validate_generation(v.generation)?;
    let mut p = Vec::new();
    put_u32(&mut p, v.generation);
    put_u16(&mut p, v.console);
    put_u16(&mut p, v.multimedia);
    put_u16(&mut p, v.communications);
    Ok(p)
}
fn decode_roles(p: &[u8]) -> Result<RoleDefaults, CodecError> {
    exact(p, 10)?;
    let generation = u32_at(p, 0);
    validate_generation(generation)?;
    Ok(RoleDefaults {
        generation,
        console: u16_at(p, 4),
        multimedia: u16_at(p, 6),
        communications: u16_at(p, 8),
    })
}
fn encode_set_request(v: &SetRequest) -> Result<Vec<u8>, CodecError> {
    validate_nonzero("request id", v.request_id)?;
    validate_generation(v.generation)?;
    if v.handle == 0 {
        return Err(CodecError::InvalidValue("endpoint handle"));
    }
    let mut p = Vec::new();
    put_u32(&mut p, v.request_id);
    put_u32(&mut p, v.generation);
    put_u16(&mut p, v.handle);
    Ok(p)
}
fn decode_set_request(p: &[u8]) -> Result<SetRequest, CodecError> {
    exact(p, 10)?;
    let v = SetRequest {
        request_id: u32_at(p, 0),
        generation: u32_at(p, 4),
        handle: u16_at(p, 8),
    };
    validate_nonzero("request id", v.request_id)?;
    validate_generation(v.generation)?;
    if v.handle == 0 {
        return Err(CodecError::InvalidValue("endpoint handle"));
    }
    Ok(v)
}
fn exact(p: &[u8], n: usize) -> Result<(), CodecError> {
    if p.len() == n {
        Ok(())
    } else {
        Err(CodecError::InvalidLength {
            expected: n,
            actual: p.len(),
        })
    }
}
fn validate_generation(v: u32) -> Result<(), CodecError> {
    validate_nonzero("generation", v)
}
fn validate_nonzero(name: &'static str, v: u32) -> Result<(), CodecError> {
    if v == 0 {
        Err(CodecError::InvalidValue(name))
    } else {
        Ok(())
    }
}
fn validate_range(name: &'static str, v: u8, min: u8, max: u8) -> Result<(), CodecError> {
    if (min..=max).contains(&v) {
        Ok(())
    } else {
        Err(CodecError::InvalidValue(name))
    }
}
fn put_u16(p: &mut Vec<u8>, v: u16) {
    p.extend_from_slice(&v.to_le_bytes())
}
fn put_u32(p: &mut Vec<u8>, v: u32) {
    p.extend_from_slice(&v.to_le_bytes())
}
fn u16_at(p: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([p[o], p[o + 1]])
}
fn u32_at(p: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]])
}
fn decode_utf8(p: &[u8]) -> Result<String, CodecError> {
    std::str::from_utf8(p)
        .map(str::to_owned)
        .map_err(|_| CodecError::InvalidUtf8)
}

#[derive(Debug)]
pub enum CodecError {
    Frame(FrameError),
    UnsupportedVersion(u8),
    UnknownMessageType(u8),
    InvalidLength { expected: usize, actual: usize },
    InvalidValue(&'static str),
    ReservedNotZero,
    InvalidUtf8,
    StringTooLong(usize),
}
impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for CodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frame(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_message_round_trips() {
        let roles = RoleDefaults {
            generation: 1,
            console: 1,
            multimedia: 1,
            communications: 1,
        };
        let request = SetRequest {
            request_id: 7,
            generation: 1,
            handle: 1,
        };
        let messages = vec![
            Message::HelloRequest {
                minimum_version: 1,
                maximum_version: 1,
                capabilities: 0,
                host_nonce: 42,
            },
            Message::HelloResponse(HelloResponse {
                selected_version: 1,
                device_type: 1,
                firmware_major: 0,
                firmware_minor: 1,
                firmware_patch: 0,
                capabilities: 0,
                echoed_host_nonce: 42,
                boot_id: 99,
            }),
            Message::SyncRequest { reason: 0 },
            Message::SyncBegin {
                generation: 1,
                output_count: 1,
                input_count: 1,
            },
            Message::OutputEndpoint(EndpointRecord {
                generation: 1,
                handle: 1,
                friendly_name: "USB DAC".into(),
            }),
            Message::InputEndpoint(EndpointRecord {
                generation: 1,
                handle: 1,
                friendly_name: "マイク".into(),
            }),
            Message::CurrentOutput(roles),
            Message::CurrentInput(roles),
            Message::SyncEnd { generation: 1 },
            Message::SetOutputRequest(request),
            Message::SetInputRequest(request),
            Message::SetResult(SetResult {
                request_id: 7,
                operation: 0,
                status: 0,
                error_code: 0,
                known_generation: 1,
            }),
            Message::Ping { token: 42 },
            Message::Pong { token: 42 },
            Message::Error(ErrorMessage {
                error_code: 2,
                related_message_type: 0x99,
                context_id: 0,
                detail: "unknown".into(),
            }),
        ];
        for message in messages {
            let frame = message.encode().unwrap();
            assert_eq!(Message::decode(&frame).unwrap(), message);
        }
    }
    #[test]
    fn rejects_bad_utf8_and_reserved_byte() {
        let frame = Frame::new(0x12, vec![1, 0, 0, 0, 1, 0, 1, 0, 0xff]).unwrap();
        assert!(matches!(
            Message::decode(&frame),
            Err(CodecError::InvalidUtf8)
        ));
        let frame = Frame::new(0x01, vec![1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]).unwrap();
        assert!(matches!(
            Message::decode(&frame),
            Err(CodecError::ReservedNotZero)
        ));
    }
    #[test]
    fn rejects_unknown_type_and_version() {
        let frame = Frame::new(0x99, vec![]).unwrap();
        assert!(matches!(
            Message::decode(&frame),
            Err(CodecError::UnknownMessageType(0x99))
        ));
        let frame = Frame::with_version(2, 0x30, vec![0; 4]).unwrap();
        assert!(matches!(
            Message::decode(&frame),
            Err(CodecError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn rejects_all_shared_message_error_vectors() {
        let vectors = include_str!("../../../protocol/message-error-vectors.txt");
        for line in vectors
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let fields: Vec<_> = line.split('|').collect();
            assert_eq!(fields.len(), 3);
            let bytes: Vec<u8> = fields[2]
                .split_ascii_whitespace()
                .map(|byte| u8::from_str_radix(byte, 16).unwrap())
                .collect();
            let batch = crate::protocol::StreamParser::new().push(&bytes);
            assert_eq!(batch.frames.len(), 1, "{}", fields[0]);
            let error = Message::decode(&batch.frames[0]).unwrap_err();
            let actual = match error {
                CodecError::UnknownMessageType(_) => "unknown_type",
                CodecError::UnsupportedVersion(_) => "unsupported_version",
                CodecError::InvalidUtf8 => "invalid_utf8",
                CodecError::ReservedNotZero => "reserved_not_zero",
                CodecError::InvalidLength { .. } => "invalid_length",
                CodecError::InvalidValue(_) => "invalid_value",
                other => panic!("{}: unexpected {other:?}", fields[0]),
            };
            assert_eq!(actual, fields[1], "{}", fields[0]);
        }
    }
}
