//! Windows Core Audio endpoint enumeration and management.

mod notification;
mod policy_config;

pub use notification::{
    AudioEvent, AudioEventKind, NOTIFICATION_DEBOUNCE, NotificationDebouncer,
    NotificationSubscription,
};

use anyhow::{Context, Result};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToString;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};

use crate::config::{MAXIMUM_ENDPOINT_NAME_LENGTH, MAXIMUM_ENDPOINTS_PER_FLOW};
use policy_config::PolicyConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataFlow {
    Render,
    Capture,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioRole {
    Console,
    Multimedia,
    Communications,
}

impl AudioRole {
    fn as_windows(self) -> ERole {
        match self {
            Self::Console => eConsole,
            Self::Multimedia => eMultimedia,
            Self::Communications => eCommunications,
        }
    }
}

impl DataFlow {
    fn as_windows(self) -> EDataFlow {
        match self {
            Self::Render => eRender,
            Self::Capture => eCapture,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub handle: u16,
    pub id: String,
    pub name: String,
    pub flow: DataFlow,
    pub state: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DefaultEndpoint {
    pub id: Option<String>,
    pub handle: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RoleDefaults {
    pub console: DefaultEndpoint,
    pub multimedia: DefaultEndpoint,
    pub communications: DefaultEndpoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetDefaultOutcome {
    pub defaults: RoleDefaults,
    pub failed_roles: Vec<AudioRole>,
    pub verified: bool,
    pub partial_success: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetProtocolResult {
    pub status: u8,
    pub error_code: u16,
}

impl SetDefaultOutcome {
    pub fn protocol_result(&self) -> SetProtocolResult {
        if self.verified {
            SetProtocolResult {
                status: 0x00,
                error_code: 0x0000,
            }
        } else if self.failed_roles.is_empty() {
            SetProtocolResult {
                status: 0x01,
                error_code: 0x000D,
            }
        } else {
            SetProtocolResult {
                status: 0x01,
                error_code: 0x000C,
            }
        }
    }
}

impl RoleDefaults {
    pub fn is_split(&self) -> bool {
        let handles = [
            self.console.handle,
            self.multimedia.handle,
            self.communications.handle,
        ];
        let mut nonzero = handles.into_iter().filter(|handle| *handle != 0);
        let Some(first) = nonzero.next() else {
            return false;
        };
        nonzero.any(|handle| handle != first)
    }
}

#[derive(Debug)]
struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self> {
        // SAFETY: COM is initialized once on this thread; Drop balances it.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
            .context("COMの初期化に失敗しました")?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: Paired with the successful CoInitializeEx call above.
        unsafe { CoUninitialize() };
    }
}

#[derive(Debug)]
pub struct AudioService {
    _com: ComApartment,
    enumerator: IMMDeviceEnumerator,
}

impl AudioService {
    pub fn new() -> Result<Self> {
        let com = ComApartment::initialize()?;
        // SAFETY: COM is initialized and creation does not use aggregation.
        let enumerator = unsafe {
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        }
        .context("MMDeviceEnumeratorの作成に失敗しました")?;
        Ok(Self {
            _com: com,
            enumerator,
        })
    }

    pub fn enumerate_active(&self, flow: DataFlow) -> Result<Vec<Endpoint>> {
        // SAFETY: The enumerator is a valid COM interface.
        let collection = unsafe {
            self.enumerator
                .EnumAudioEndpoints(flow.as_windows(), DEVICE_STATE_ACTIVE)
        }
        .with_context(|| format!("{flow:?} Endpoint一覧の取得に失敗しました"))?;

        // SAFETY: collection is a valid IMMDeviceCollection.
        let count = unsafe { collection.GetCount() }.context("Endpoint件数の取得に失敗しました")?;
        let mut endpoints = Vec::with_capacity((count as usize).min(MAXIMUM_ENDPOINTS_PER_FLOW));

        for index in 0..count {
            // SAFETY: index is within the count returned by this collection.
            let device = unsafe { collection.Item(index) }
                .with_context(|| format!("Endpoint {index}の取得に失敗しました"))?;
            endpoints.push(read_endpoint(&device, flow)?);
        }

        Ok(normalize_endpoints(endpoints))
    }

    pub fn subscribe(&self) -> Result<NotificationSubscription> {
        NotificationSubscription::register(&self.enumerator)
    }

    pub fn default_endpoints(
        &self,
        flow: DataFlow,
        active_endpoints: &[Endpoint],
    ) -> Result<RoleDefaults> {
        Ok(RoleDefaults {
            console: self.default_endpoint(flow, eConsole, active_endpoints)?,
            multimedia: self.default_endpoint(flow, eMultimedia, active_endpoints)?,
            communications: self.default_endpoint(flow, eCommunications, active_endpoints)?,
        })
    }

    pub fn set_default_all_roles(
        &self,
        flow: DataFlow,
        endpoint: &Endpoint,
        active_endpoints: &[Endpoint],
    ) -> Result<SetDefaultOutcome> {
        if endpoint.flow != flow || endpoint.state != DEVICE_STATE_ACTIVE.0 {
            anyhow::bail!("指定Endpointは対象データフローの有効Endpointではありません");
        }

        let policy = PolicyConfig::new().context("PolicyConfigClientの作成に失敗しました")?;
        let roles = [
            AudioRole::Console,
            AudioRole::Multimedia,
            AudioRole::Communications,
        ];
        let mut failed_roles = Vec::new();
        for role in roles {
            if policy
                .set_default_endpoint(&endpoint.id, role.as_windows())
                .is_err()
            {
                failed_roles.push(role);
            }
        }

        let defaults = self.default_endpoints(flow, active_endpoints)?;
        Ok(evaluate_set_outcome(endpoint, defaults, failed_roles))
    }

    fn default_endpoint(
        &self,
        flow: DataFlow,
        role: ERole,
        active_endpoints: &[Endpoint],
    ) -> Result<DefaultEndpoint> {
        // SAFETY: The enumerator and the Core Audio enum values are valid.
        let device = match unsafe {
            self.enumerator
                .GetDefaultAudioEndpoint(flow.as_windows(), role)
        } {
            Ok(device) => device,
            Err(error) if error.code() == windows::core::HRESULT(0x80070490u32 as i32) => {
                return Ok(DefaultEndpoint::default());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("{flow:?} role {}の既定Endpoint取得に失敗しました", role.0)
                });
            }
        };

        // SAFETY: device is valid. endpoint_id releases the GetId allocation.
        let id =
            endpoint_id(unsafe { device.GetId() }.context("既定Endpoint IDの取得に失敗しました")?)?;
        let handle = active_endpoints
            .iter()
            .find(|endpoint| endpoint.id == id)
            .map_or(0, |endpoint| endpoint.handle);
        Ok(DefaultEndpoint {
            id: Some(id),
            handle,
        })
    }
}

fn read_endpoint(device: &IMMDevice, flow: DataFlow) -> Result<Endpoint> {
    // SAFETY: device is valid. endpoint_id releases the GetId allocation.
    let id = endpoint_id(unsafe { device.GetId() }.context("Endpoint IDの取得に失敗しました")?)?;

    // SAFETY: device is valid and the property store is opened read-only.
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }
        .context("Endpointプロパティの取得に失敗しました")?;
    // SAFETY: PKEY_Device_FriendlyName is a valid property key.
    let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName) }
        .context("Endpoint表示名の取得に失敗しました")?;
    let mut buffer = [0u16; 512];
    // SAFETY: value and the writable buffer are valid for this call.
    unsafe { PropVariantToString(&value, &mut buffer) }
        .context("Endpoint表示名の文字列変換に失敗しました")?;
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    let name = String::from_utf16(&buffer[..length]).context("Endpoint表示名が不正なUTF-16です")?;
    // SAFETY: device is a valid IMMDevice.
    let state = unsafe { device.GetState() }.context("Endpoint状態の取得に失敗しました")?;

    Ok(Endpoint {
        handle: 0,
        id,
        name: truncate_utf8(name, MAXIMUM_ENDPOINT_NAME_LENGTH),
        flow,
        state: state.0,
    })
}

fn endpoint_id(id: windows::core::PWSTR) -> Result<String> {
    // SAFETY: id is a null-terminated string returned by IMMDevice::GetId.
    let result = unsafe { id.to_string() }.context("Endpoint IDが不正なUTF-16です");
    // SAFETY: GetId allocated this pointer with CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(id.0.cast())) };
    result
}

fn normalize_endpoints(mut endpoints: Vec<Endpoint>) -> Vec<Endpoint> {
    endpoints.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    endpoints.truncate(MAXIMUM_ENDPOINTS_PER_FLOW);
    for (index, endpoint) in endpoints.iter_mut().enumerate() {
        endpoint.handle = u16::try_from(index + 1).expect("Endpoint上限はu16の範囲内");
    }
    endpoints
}

fn evaluate_set_outcome(
    endpoint: &Endpoint,
    defaults: RoleDefaults,
    failed_roles: Vec<AudioRole>,
) -> SetDefaultOutcome {
    let matches = [
        defaults.console.id.as_deref() == Some(endpoint.id.as_str()),
        defaults.multimedia.id.as_deref() == Some(endpoint.id.as_str()),
        defaults.communications.id.as_deref() == Some(endpoint.id.as_str()),
    ];
    let matching_count = matches.into_iter().filter(|matches| *matches).count();
    SetDefaultOutcome {
        defaults,
        failed_roles,
        verified: matching_count == 3,
        partial_success: (1..3).contains(&matching_count),
    }
}

fn truncate_utf8(mut value: String, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value;
    }
    let mut boundary = maximum_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_truncation_does_not_split_a_character() {
        let value = "a".repeat(238) + "マ";
        let truncated = truncate_utf8(value, 240);
        assert_eq!(truncated.len(), 238);
        assert!(truncated.ends_with('a'));
    }

    #[test]
    fn short_names_are_unchanged() {
        assert_eq!(truncate_utf8("マイク".into(), 240), "マイク");
    }

    #[test]
    fn endpoint_order_handles_and_limit_are_stable() {
        let mut endpoints = Vec::new();
        for index in (0..64).rev() {
            endpoints.push(Endpoint {
                handle: 0,
                id: format!("id-{index:02}"),
                name: if index < 2 {
                    "Same Name".into()
                } else {
                    format!("Device {index:02}")
                },
                flow: DataFlow::Render,
                state: DEVICE_STATE_ACTIVE.0,
            });
        }
        endpoints.push(Endpoint {
            handle: 0,
            id: "discarded".into(),
            name: "ZZZ after limit".into(),
            flow: DataFlow::Render,
            state: DEVICE_STATE_ACTIVE.0,
        });

        let endpoints = normalize_endpoints(endpoints);
        assert_eq!(endpoints.len(), 64);
        assert_eq!(endpoints[0].name, "Device 02");
        assert_eq!(endpoints[0].handle, 1);
        assert_eq!(endpoints[62].id, "id-00");
        assert_eq!(endpoints[63].id, "id-01");
        assert_eq!(endpoints[63].handle, 64);
    }

    #[test]
    fn role_split_ignores_missing_roles() {
        let unified = RoleDefaults {
            console: DefaultEndpoint {
                id: None,
                handle: 4,
            },
            multimedia: DefaultEndpoint {
                id: None,
                handle: 4,
            },
            communications: DefaultEndpoint::default(),
        };
        assert!(!unified.is_split());

        let split = RoleDefaults {
            communications: DefaultEndpoint {
                id: None,
                handle: 9,
            },
            ..unified
        };
        assert!(split.is_split());
    }

    #[test]
    fn set_outcome_detects_verified_and_partial_states() {
        let endpoint = Endpoint {
            handle: 1,
            id: "target".into(),
            name: "Target".into(),
            flow: DataFlow::Render,
            state: DEVICE_STATE_ACTIVE.0,
        };
        let target = || DefaultEndpoint {
            id: Some("target".into()),
            handle: 1,
        };
        let other = || DefaultEndpoint {
            id: Some("other".into()),
            handle: 2,
        };

        let verified = evaluate_set_outcome(
            &endpoint,
            RoleDefaults {
                console: target(),
                multimedia: target(),
                communications: target(),
            },
            Vec::new(),
        );
        assert!(verified.verified);
        assert!(!verified.partial_success);
        assert_eq!(
            verified.protocol_result(),
            SetProtocolResult {
                status: 0,
                error_code: 0
            }
        );

        let partial = evaluate_set_outcome(
            &endpoint,
            RoleDefaults {
                console: target(),
                multimedia: other(),
                communications: other(),
            },
            vec![AudioRole::Multimedia],
        );
        assert!(!partial.verified);
        assert!(partial.partial_success);
        assert_eq!(partial.protocol_result().error_code, 0x000C);

        let verification_failure = evaluate_set_outcome(
            &endpoint,
            RoleDefaults {
                console: other(),
                multimedia: other(),
                communications: other(),
            },
            Vec::new(),
        );
        assert_eq!(verification_failure.protocol_result().error_code, 0x000D);
    }

    #[test]
    fn vanished_endpoint_reports_failure_with_retrieved_actual_state() {
        let vanished = Endpoint {
            handle: 7,
            id: "vanished".into(),
            name: "Disconnected device".into(),
            flow: DataFlow::Render,
            state: DEVICE_STATE_ACTIVE.0,
        };
        let fallback = || DefaultEndpoint {
            id: Some("fallback".into()),
            handle: 2,
        };
        let actual = RoleDefaults {
            console: fallback(),
            multimedia: fallback(),
            communications: fallback(),
        };
        let outcome = evaluate_set_outcome(
            &vanished,
            actual.clone(),
            vec![
                AudioRole::Console,
                AudioRole::Multimedia,
                AudioRole::Communications,
            ],
        );

        assert!(!outcome.verified);
        assert_eq!(outcome.defaults, actual);
        assert_eq!(
            outcome.protocol_result(),
            SetProtocolResult {
                status: 0x01,
                error_code: 0x000C,
            }
        );
    }
}
