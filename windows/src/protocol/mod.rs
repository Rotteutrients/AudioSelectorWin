//! Audio Selector binary protocol framing and stream parsing.

mod frame;
mod message;
mod parser;

pub use frame::{Frame, FrameError, HEADER_LENGTH, MAGIC};
pub use message::{
    CodecError, EndpointRecord, ErrorMessage, HelloResponse, Message, RoleDefaults, SetRequest,
    SetResult,
};
pub use parser::{ParseBatch, StreamParser};

pub use crate::config::{MAXIMUM_PAYLOAD_LENGTH, PROTOCOL_VERSION, RECEIVE_BUFFER_LENGTH};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageType {
    HelloRequest = 0x01,
    HelloResponse = 0x02,
    SyncRequest = 0x10,
    SyncBegin = 0x11,
    OutputEndpoint = 0x12,
    InputEndpoint = 0x13,
    CurrentOutput = 0x14,
    CurrentInput = 0x15,
    SyncEnd = 0x16,
    SetOutputRequest = 0x20,
    SetInputRequest = 0x21,
    SetResult = 0x22,
    Ping = 0x30,
    Pong = 0x31,
    Error = 0x7f,
}

impl TryFrom<u8> for MessageType {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, u8> {
        Ok(match value {
            0x01 => Self::HelloRequest,
            0x02 => Self::HelloResponse,
            0x10 => Self::SyncRequest,
            0x11 => Self::SyncBegin,
            0x12 => Self::OutputEndpoint,
            0x13 => Self::InputEndpoint,
            0x14 => Self::CurrentOutput,
            0x15 => Self::CurrentInput,
            0x16 => Self::SyncEnd,
            0x20 => Self::SetOutputRequest,
            0x21 => Self::SetInputRequest,
            0x22 => Self::SetResult,
            0x30 => Self::Ping,
            0x31 => Self::Pong,
            0x7f => Self::Error,
            unknown => return Err(unknown),
        })
    }
}
