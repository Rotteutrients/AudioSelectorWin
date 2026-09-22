pub const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u8 = 0x01;
pub const DEVICE_TYPE_AUDIO_SELECTOR: u8 = 0x01;
pub const BLUETOOTH_DEVICE_NAME: &str = "AudioSelector";

pub const MAXIMUM_PAYLOAD_LENGTH: usize = 512;
pub const RECEIVE_BUFFER_LENGTH: usize = 2048;
pub const MAXIMUM_ENDPOINTS_PER_FLOW: usize = 64;
pub const MAXIMUM_ENDPOINT_NAME_LENGTH: usize = 240;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_limits_match_version_one_specification() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(MAXIMUM_PAYLOAD_LENGTH, 512);
        assert_eq!(RECEIVE_BUFFER_LENGTH, 2048);
        assert_eq!(MAXIMUM_ENDPOINTS_PER_FLOW, 64);
        assert_eq!(MAXIMUM_ENDPOINT_NAME_LENGTH, 240);
    }
}
