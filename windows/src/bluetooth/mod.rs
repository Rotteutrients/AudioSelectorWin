//! Bluetooth Classic SPP discovery and transport.

use std::mem::size_of;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use windows::Devices::Bluetooth::Rfcomm::{RfcommDeviceService, RfcommServiceId};
use windows::Devices::Bluetooth::{BluetoothDevice as WinRtBluetoothDevice, BluetoothError};
use windows::Networking::Sockets::StreamSocket;
use windows::Storage::Streams::{
    DataReader, DataReaderLoadOperation, DataWriter, InputStreamOptions,
};
use windows::Win32::Devices::Bluetooth::{
    BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS, BluetoothEnumerateInstalledServices,
    BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindNextDevice,
    HBLUETOOTH_DEVICE_FIND,
};
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, HANDLE};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows_core::GUID;

use crate::protocol::{HelloResponse, Message, StreamParser};

pub const SERIAL_PORT_SERVICE_CLASS_ID: GUID =
    GUID::from_u128(0x00001101_0000_1000_8000_00805f9b34fb);
pub const KEEPALIVE_IDLE: Duration = Duration::from_secs(10);
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct ReconnectBackoff {
    next: Duration,
    initial: Duration,
    maximum: Duration,
}

impl ReconnectBackoff {
    pub fn new(initial: Duration, maximum: Duration) -> Self {
        assert!(!initial.is_zero() && initial <= maximum);
        Self {
            next: initial,
            initial,
            maximum,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = self.next.saturating_mul(2).min(self.maximum);
        delay
    }

    pub fn reset(&mut self) {
        self.next = self.initial;
    }
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self::new(Duration::from_secs(1), Duration::from_secs(30))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothDevice {
    pub name: String,
    pub address: u64,
    pub authenticated: bool,
    pub remembered: bool,
    pub connected: bool,
    pub has_serial_port_service: bool,
}

#[derive(Debug)]
struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self> {
        // SAFETY: Drop balances every successful COM initialization on this thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
            .context("Bluetooth用COM初期化に失敗しました")?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: paired with the successful CoInitializeEx call above.
        unsafe { CoUninitialize() };
    }
}

pub struct SppConnection {
    device: WinRtBluetoothDevice,
    service: RfcommDeviceService,
    socket: StreamSocket,
    reader: DataReader,
    writer: DataWriter,
    parser: StreamParser,
    pending_read: Option<DataReaderLoadOperation>,
    // Must be dropped after all WinRT objects above.
    _com: ComApartment,
}

impl SppConnection {
    pub fn connect_audio_selector() -> Result<Self> {
        let discovered = find_paired_audio_selector()?;
        if !discovered.has_serial_port_service {
            bail!("AudioSelectorにSerial Port Profileサービスがありません");
        }

        let com = ComApartment::initialize()?;
        let device = WinRtBluetoothDevice::FromBluetoothAddressAsync(discovered.address)
            .context("Bluetoothデバイス取得の開始に失敗しました")?
            .join()
            .context("Bluetoothデバイスの取得に失敗しました")?;
        // The Win32 discovery above already authenticated the exact device name and
        // supplied this address. WinRT can transiently expose an empty cached Name
        // immediately after an RFCOMM disconnect, so do not reject that same address
        // using a second, less stable name lookup.

        let service_id =
            RfcommServiceId::SerialPort().context("Serial Port Profile IDの作成に失敗しました")?;
        let service_result = device
            .GetRfcommServicesForIdAsync(&service_id)
            .context("SPPサービス取得の開始に失敗しました")?
            .join()
            .context("SPPサービスの取得に失敗しました")?;
        if service_result.Error()? != BluetoothError::Success {
            bail!("SPPサービスの取得結果が成功ではありません");
        }
        let services = service_result.Services()?;
        if services.Size()? != 1 {
            bail!(
                "AudioSelectorのSPPサービス数が1ではありません: {}",
                services.Size()?
            );
        }
        let service = services.GetAt(0)?;

        let socket = StreamSocket::new().context("RFCOMMソケットの作成に失敗しました")?;
        socket
            .ConnectAsync(
                &service.ConnectionHostName()?,
                &service.ConnectionServiceName()?,
            )
            .context("RFCOMM接続の開始に失敗しました")?
            .join()
            .context("RFCOMM接続に失敗しました")?;

        let reader = DataReader::CreateDataReader(&socket.InputStream()?)?;
        reader.SetInputStreamOptions(InputStreamOptions::Partial)?;
        let writer = DataWriter::CreateDataWriter(&socket.OutputStream()?)?;

        Ok(Self {
            device,
            service,
            socket,
            reader,
            writer,
            parser: StreamParser::new(),
            pending_read: None,
            _com: com,
        })
    }

    pub fn handshake(&mut self, host_nonce: u32, timeout: Duration) -> Result<HelloResponse> {
        self.send_message(&Message::HelloRequest {
            minimum_version: 1,
            maximum_version: 1,
            capabilities: 0,
            host_nonce,
        })?;

        let deadline = Instant::now() + timeout;
        loop {
            let bytes = self.read_until(deadline)?;
            let batch = self.parser.push(&bytes);
            if let Some(error) = batch.errors.first() {
                bail!("HELLO応答のフレーム解析に失敗しました: {error}");
            }
            for frame in batch.frames {
                match Message::decode(&frame)? {
                    Message::HelloResponse(response) => {
                        validate_hello_response(&response, host_nonce)?;
                        return Ok(response);
                    }
                    Message::Ping { token } => self.send_message(&Message::Pong { token })?,
                    Message::Error(error) => {
                        bail!("デバイスがHELLOエラーを返しました: {error:?}");
                    }
                    other => bail!("HELLO前に想定外のメッセージを受信しました: {other:?}"),
                }
            }
        }
    }

    pub fn ping(&mut self, token: u32, timeout: Duration) -> Result<()> {
        self.send_message(&Message::Ping { token })?;
        let deadline = Instant::now() + timeout;
        loop {
            let bytes = self.read_until(deadline)?;
            let batch = self.parser.push(&bytes);
            if let Some(error) = batch.errors.first() {
                bail!("PING応答のフレーム解析に失敗しました: {error}");
            }
            for frame in batch.frames {
                match Message::decode(&frame)? {
                    Message::Pong {
                        token: response_token,
                    } if response_token == token => return Ok(()),
                    Message::Ping { token } => self.send_message(&Message::Pong { token })?,
                    Message::Error(error) => {
                        bail!("デバイスがPINGエラーを返しました: {error:?}");
                    }
                    other => bail!("PING待機中に想定外のメッセージを受信しました: {other:?}"),
                }
            }
        }
    }

    pub fn receive_messages(&mut self, timeout: Duration) -> Result<Vec<Message>> {
        let deadline = Instant::now() + timeout;
        let Some(bytes) = self.read_until_optional(deadline)? else {
            return Ok(Vec::new());
        };
        let batch = self.parser.push(&bytes);
        if let Some(error) = batch.errors.first() {
            bail!("受信フレームの解析に失敗しました: {error}");
        }
        batch
            .frames
            .iter()
            .map(Message::decode)
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("受信メッセージのデコードに失敗しました")
    }

    pub fn send_message(&self, message: &Message) -> Result<()> {
        let bytes = message.encode()?.encode()?;
        self.writer.WriteBytes(&bytes)?;
        let stored = self.writer.StoreAsync()?.join()?;
        if stored as usize != bytes.len() {
            bail!("SPP書き込みが途中で終了しました: {stored}/{}", bytes.len());
        }
        if !self.writer.FlushAsync()?.join()? {
            bail!("SPP書き込みのflushに失敗しました");
        }
        Ok(())
    }

    fn read_until(&mut self, deadline: Instant) -> Result<Vec<u8>> {
        self.read_until_optional(deadline)?
            .context("SPP受信がタイムアウトしました")
    }

    fn read_until_optional(&mut self, deadline: Instant) -> Result<Option<Vec<u8>>> {
        if self.pending_read.is_none() {
            self.pending_read = Some(self.reader.LoadAsync(518)?);
        }
        while self
            .pending_read
            .as_ref()
            .context("SPP受信操作がありません")?
            .Status()?
            .0
            == 0
        {
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(10));
        }
        let operation = self
            .pending_read
            .take()
            .context("SPP受信操作がありません")?;
        let loaded = operation.GetResults()?;
        if loaded == 0 {
            bail!("SPP接続が切断されました");
        }
        let mut bytes = vec![0u8; loaded as usize];
        self.reader.ReadBytes(&mut bytes)?;
        Ok(Some(bytes))
    }
}

impl Drop for SppConnection {
    fn drop(&mut self) {
        if let Some(operation) = self.pending_read.take() {
            let _ = operation.Cancel();
        }
        let _ = self.writer.DetachStream();
        let _ = self.reader.DetachStream();
        let _ = self.socket.Close();
        let _ = self.service.Close();
        let _ = self.device.Close();
    }
}

fn validate_hello_response(response: &HelloResponse, expected_nonce: u32) -> Result<()> {
    if response.selected_version != 1 {
        bail!("選択されたプロトコルバージョンが1ではありません");
    }
    if response.device_type != 1 {
        bail!("接続先がAudio Selectorではありません");
    }
    if response.echoed_host_nonce != expected_nonce {
        bail!("HELLO応答のnonceが一致しません");
    }
    if response.boot_id == 0 {
        bail!("HELLO応答のboot IDが0です");
    }
    Ok(())
}

struct DeviceSearch(HBLUETOOTH_DEVICE_FIND);

impl Drop for DeviceSearch {
    fn drop(&mut self) {
        // SAFETY: this handle came from BluetoothFindFirstDevice and is closed once here.
        let _ = unsafe { BluetoothFindDeviceClose(self.0) };
    }
}

pub fn find_paired_audio_selector() -> Result<BluetoothDevice> {
    find_paired_device("AudioSelector")
}

fn find_paired_device(expected_name: &str) -> Result<BluetoothDevice> {
    let search_parameters = BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize: u32::try_from(size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>())?,
        fReturnAuthenticated: true.into(),
        fReturnRemembered: true.into(),
        fReturnUnknown: false.into(),
        fReturnConnected: true.into(),
        fIssueInquiry: false.into(),
        cTimeoutMultiplier: 0,
        hRadio: HANDLE::default(),
    };
    let mut information = empty_device_information()?;
    // SAFETY: both structures have their required size fields and remain valid for the call.
    let search = DeviceSearch(
        unsafe { BluetoothFindFirstDevice(&search_parameters, &mut information) }
            .context("ペアリング済みBluetoothデバイスが見つかりません")?,
    );

    loop {
        let name = device_name(&information)?;
        if is_target_device(&name, information.fAuthenticated.as_bool(), expected_name) {
            return Ok(BluetoothDevice {
                name,
                // SAFETY: ullLong is the canonical 64-bit view of BLUETOOTH_ADDRESS.
                address: unsafe { information.Address.Anonymous.ullLong },
                authenticated: information.fAuthenticated.as_bool(),
                remembered: information.fRemembered.as_bool(),
                connected: information.fConnected.as_bool(),
                has_serial_port_service: has_serial_port_service(&information)?,
            });
        }

        information = empty_device_information()?;
        // SAFETY: search remains open and information has the required size field.
        if unsafe { BluetoothFindNextDevice(search.0, &mut information) }.is_err() {
            break;
        }
    }

    bail!("ペアリング済みの{expected_name}が見つかりません")
}

fn is_target_device(name: &str, authenticated: bool, expected_name: &str) -> bool {
    authenticated && name == expected_name
}

fn empty_device_information() -> Result<BLUETOOTH_DEVICE_INFO> {
    Ok(BLUETOOTH_DEVICE_INFO {
        dwSize: u32::try_from(size_of::<BLUETOOTH_DEVICE_INFO>())?,
        ..Default::default()
    })
}

fn device_name(information: &BLUETOOTH_DEVICE_INFO) -> Result<String> {
    let length = information
        .szName
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(information.szName.len());
    String::from_utf16(&information.szName[..length])
        .context("Bluetoothデバイス名が不正なUTF-16です")
}

fn has_serial_port_service(information: &BLUETOOTH_DEVICE_INFO) -> Result<bool> {
    let mut count = 0u32;
    // SAFETY: information is valid and a null service buffer requests the required count.
    let first_result =
        unsafe { BluetoothEnumerateInstalledServices(None, information, &mut count, None) };
    if first_result == ERROR_SUCCESS.0 && count == 0 {
        return Ok(false);
    }
    if first_result != ERROR_MORE_DATA.0 {
        bail!("Bluetoothサービス件数の取得に失敗しました: Win32 error {first_result}");
    }

    let mut services = vec![GUID::zeroed(); count as usize];
    // SAFETY: services has capacity for count GUID values and both pointers remain valid.
    let result = unsafe {
        BluetoothEnumerateInstalledServices(
            None,
            information,
            &mut count,
            Some(services.as_mut_ptr()),
        )
    };
    if result != ERROR_SUCCESS.0 {
        bail!("Bluetoothサービス一覧の取得に失敗しました: Win32 error {result}");
    }
    services.truncate(count as usize);
    Ok(services.contains(&SERIAL_PORT_SERVICE_CLASS_ID))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_port_service_uuid_matches_bluetooth_standard() {
        assert_eq!(
            SERIAL_PORT_SERVICE_CLASS_ID,
            GUID::from_u128(0x00001101_0000_1000_8000_00805f9b34fb)
        );
    }

    #[test]
    fn bluetooth_name_conversion_stops_at_nul() {
        let mut information = empty_device_information().expect("structure size should fit u32");
        for (target, source) in information
            .szName
            .iter_mut()
            .zip("AudioSelector\0ignored".encode_utf16())
        {
            *target = source;
        }
        assert_eq!(device_name(&information).unwrap(), "AudioSelector");
    }

    #[test]
    fn only_authenticated_exact_name_is_selected() {
        assert!(is_target_device("AudioSelector", true, "AudioSelector"));
        assert!(!is_target_device("AudioSelector", false, "AudioSelector"));
        assert!(!is_target_device(
            "AudioSelector-Other",
            true,
            "AudioSelector"
        ));
        assert!(!is_target_device("CH340", true, "AudioSelector"));
    }

    #[test]
    fn hello_response_validation_checks_identity_and_nonce() {
        let valid = HelloResponse {
            selected_version: 1,
            device_type: 1,
            firmware_major: 0,
            firmware_minor: 1,
            firmware_patch: 0,
            capabilities: 0,
            echoed_host_nonce: 42,
            boot_id: 7,
        };
        assert!(validate_hello_response(&valid, 42).is_ok());
        assert!(validate_hello_response(&valid, 41).is_err());
        assert!(
            validate_hello_response(
                &HelloResponse {
                    device_type: 2,
                    ..valid.clone()
                },
                42
            )
            .is_err()
        );
        assert!(
            validate_hello_response(
                &HelloResponse {
                    boot_id: 0,
                    ..valid
                },
                42
            )
            .is_err()
        );
    }

    #[test]
    fn reconnect_backoff_doubles_and_caps_then_resets() {
        let mut backoff = ReconnectBackoff::default();
        let delays: Vec<_> = (0..7).map(|_| backoff.next_delay().as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30]);
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    }
}
