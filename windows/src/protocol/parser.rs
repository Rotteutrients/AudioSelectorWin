use super::{
    Frame, FrameError, HEADER_LENGTH, MAGIC, MAXIMUM_PAYLOAD_LENGTH, RECEIVE_BUFFER_LENGTH,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParseBatch {
    pub frames: Vec<Frame>,
    pub errors: Vec<FrameError>,
}

#[derive(Clone, Debug, Default)]
pub struct StreamParser {
    buffer: Vec<u8>,
}

impl StreamParser {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn push(&mut self, bytes: &[u8]) -> ParseBatch {
        let mut batch = ParseBatch::default();
        if self.buffer.len().saturating_add(bytes.len()) > RECEIVE_BUFFER_LENGTH {
            self.buffer.clear();
            batch.errors.push(FrameError::ReceiveBufferOverflow);
            return batch;
        }
        self.buffer.extend_from_slice(bytes);
        loop {
            self.discard_before_magic();
            if self.buffer.len() < HEADER_LENGTH {
                break;
            }
            let length = u16::from_le_bytes([self.buffer[4], self.buffer[5]]);
            if usize::from(length) > MAXIMUM_PAYLOAD_LENGTH {
                batch.errors.push(FrameError::InvalidPayloadLength(length));
                self.buffer.remove(0);
                continue;
            }
            let frame_length = HEADER_LENGTH + usize::from(length);
            if self.buffer.len() < frame_length {
                break;
            }
            let bytes: Vec<_> = self.buffer.drain(..frame_length).collect();
            batch.frames.push(Frame {
                version: bytes[2],
                message_type: bytes[3],
                payload: bytes[HEADER_LENGTH..].to_vec(),
            });
        }
        batch
    }

    fn discard_before_magic(&mut self) {
        if self.buffer.starts_with(&MAGIC) {
            return;
        }
        if let Some(position) = self.buffer.windows(2).position(|window| window == MAGIC) {
            self.buffer.drain(..position);
            return;
        }
        let keep_prefix = self.buffer.last() == Some(&MAGIC[0]);
        self.buffer.clear();
        if keep_prefix {
            self.buffer.push(MAGIC[0]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping() -> Vec<u8> {
        vec![0x41, 0x53, 0x01, 0x30, 0x04, 0x00, 0x78, 0x56, 0x34, 0x12]
    }

    #[test]
    fn partial_and_multiple_frames() {
        let mut parser = StreamParser::new();
        assert!(parser.push(&ping()[..5]).frames.is_empty());
        assert_eq!(parser.push(&ping()[5..]).frames.len(), 1);
        let mut two = ping();
        two.extend_from_slice(&ping());
        assert_eq!(StreamParser::new().push(&two).frames.len(), 2);
    }

    #[test]
    fn resynchronizes_after_garbage_and_bad_length() {
        let mut bytes = vec![0xff, 0x41, 0x00, 0x41, 0x53, 0x01, 0x30, 0x01, 0x02];
        bytes.extend_from_slice(&ping());
        let batch = StreamParser::new().push(&bytes);
        assert_eq!(batch.errors, [FrameError::InvalidPayloadLength(513)]);
        assert_eq!(batch.frames.len(), 1);
    }

    #[test]
    fn preserves_lone_magic_prefix_and_limits_buffer() {
        let mut parser = StreamParser::new();
        parser.push(&[0xff, 0x41]);
        assert_eq!(parser.buffered_len(), 1);
        assert_eq!(parser.push(&ping()[1..]).frames.len(), 1);
        let batch = parser.push(&vec![0; RECEIVE_BUFFER_LENGTH + 1]);
        assert_eq!(batch.errors, [FrameError::ReceiveBufferOverflow]);
    }

    #[test]
    fn parses_all_shared_vectors() {
        let vectors = include_str!("../../../protocol/test-vectors.txt");
        for line in vectors
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let (name, hex) = line.split_once('|').unwrap();
            let bytes: Vec<u8> = hex
                .split_ascii_whitespace()
                .map(|b| u8::from_str_radix(b, 16).unwrap())
                .collect();
            let batch = StreamParser::new().push(&bytes);
            assert!(batch.errors.is_empty(), "{name}");
            assert_eq!(batch.frames.len(), 1, "{name}");
            assert_eq!(batch.frames[0].encode().unwrap(), bytes, "{name}");
        }
    }

    #[test]
    fn parses_all_shared_stream_error_vectors() {
        let vectors = include_str!("../../../protocol/stream-error-vectors.txt");
        for line in vectors
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let fields: Vec<_> = line.split('|').collect();
            assert_eq!(fields.len(), 4);
            let mut parser = StreamParser::new();
            let mut frame_count = 0;
            let mut error_count = 0;
            for chunk in fields[3].split('/') {
                let bytes: Vec<u8> = chunk
                    .split_ascii_whitespace()
                    .map(|byte| u8::from_str_radix(byte, 16).unwrap())
                    .collect();
                let batch = parser.push(&bytes);
                frame_count += batch.frames.len();
                error_count += batch.errors.len();
            }
            assert_eq!(frame_count, fields[1].parse().unwrap(), "{}", fields[0]);
            assert_eq!(error_count, fields[2].parse().unwrap(), "{}", fields[0]);
        }
    }
}
