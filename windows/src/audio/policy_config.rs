use windows::Win32::Media::Audio::ERole;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::{GUID, HRESULT, IUnknown_Vtbl, Interface, PCWSTR};

const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

windows::core::imp::define_interface!(
    IPolicyConfig,
    IPolicyConfig_Vtbl,
    0xf8679f50_850a_41cf_9c72_430f290290c8
);
windows::core::imp::interface_hierarchy!(IPolicyConfig, windows::core::IUnknown);

#[repr(C)]
pub struct IPolicyConfig_Vtbl {
    pub base__: IUnknown_Vtbl,
    get_mix_format: usize,
    get_device_format: usize,
    reset_device_format: usize,
    set_device_format: usize,
    get_processing_period: usize,
    set_processing_period: usize,
    get_share_mode: usize,
    set_share_mode: usize,
    get_property_value: usize,
    set_property_value: usize,
    set_default_endpoint:
        unsafe extern "system" fn(*mut core::ffi::c_void, PCWSTR, ERole) -> HRESULT,
    set_endpoint_visibility: usize,
}

pub struct PolicyConfig {
    interface: IPolicyConfig,
}

impl PolicyConfig {
    pub fn new() -> windows::core::Result<Self> {
        // SAFETY: COM is initialized by AudioService and no aggregation is used.
        let interface = unsafe {
            CoCreateInstance::<_, IPolicyConfig>(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?
        };
        Ok(Self { interface })
    }

    pub fn set_default_endpoint(
        &self,
        endpoint_id: &str,
        role: ERole,
    ) -> windows::core::Result<()> {
        let wide: Vec<u16> = endpoint_id.encode_utf16().chain([0]).collect();
        // SAFETY: wide is null-terminated and remains alive for the COM call;
        // interface is a valid IPolicyConfig instance.
        unsafe {
            (Interface::vtable(&self.interface).set_default_endpoint)(
                Interface::as_raw(&self.interface),
                PCWSTR::from_raw(wide.as_ptr()),
                role,
            )
            .ok()
        }
    }
}
