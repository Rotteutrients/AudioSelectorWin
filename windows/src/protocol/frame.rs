use std::error::Error;
use std::fmt;

use super::{MAXIMUM_PAYLOAD_LENGTH, PROTOCOL_VERSION};

pub const MAGIC: [u8; 2] = [0x41, 0x53];
pub const HEADER_LENGTH: usize = 6;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub version: u8,
    pub message_type: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(message_type: u8, payload: Vec<u8>) -> Result<Self, FrameError> {
        Self::with_version(PROTOCOL_VERSION, message_type, payload)
    }

    pub fn with_version(
        version: u8,
        message_type: u8,
        payload: Vec<u8>,
    ) -> Result<Self, FrameError> {
        if payload.len() > MAXIMUM_PAYLOAD_LENGTH {
            return Err(FrameError::PayloadTooLarge(payload.len()));
        }
        Ok(Self {
            version,
            message_type,
            payload,
        })
    }

    pub fn encoded_len(&self) -> usize {
        HEADER_LENGTH + self.payload.len()
    }

    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        if self.payload.len() > MAXIMUM_PAYLOAD_LENGTH {
            return Err(FrameError::PayloadTooLarge(self.payload.len()));
        }
        let length = u16::try_from(self.payload.len())
            .map_err(|_| FrameError::PayloadTooLarge(self.payload.len()))?;
        let mut bytes = Vec::with_capacity(self.encoded_len());
        bytes.extend_from_slice(&MAGIC);
        bytes.push(self.version);
        bytes.push(self.message_type);
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrameError {
    PayloadTooLarge(usize),
    ReceiveBufferOverflow,
    InvalidPayloadLength(u16),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge(n) => write!(f, "payload is too large: {n}"),
            Self::ReceiveBufferOverflow => f.write_str("receive buffer overflow"),
            Self::InvalidPayloadLength(n) => write!(f, "invalid payload length: {n}"),
        }
    }
}

impl Error for FrameError {}
