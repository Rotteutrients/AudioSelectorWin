use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use windows_core::PCWSTR;

use super::{AudioRole, DataFlow};

pub const NOTIFICATION_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioEventKind {
    DeviceAdded,
    DeviceRemoved,
    DeviceStateChanged,
    DefaultDeviceChanged,
    PropertyChanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioEvent {
    pub kind: AudioEventKind,
    pub device_id: Option<String>,
    pub flow: Option<DataFlow>,
    pub role: Option<AudioRole>,
    pub state: Option<u32>,
}

impl AudioEvent {
    fn device(kind: AudioEventKind, device_id: &PCWSTR) -> Self {
        Self {
            kind,
            device_id: pcwstr_to_string(device_id),
            flow: None,
            role: None,
            state: None,
        }
    }
}

#[windows_core::implement(IMMNotificationClient)]
struct NotificationClient {
    sender: Sender<AudioEvent>,
}

impl NotificationClient {
    fn publish(&self, event: AudioEvent) {
        // The receiver may already have shut down. Core Audio callbacks must still succeed.
        let _ = self.sender.send(event);
    }
}

impl IMMNotificationClient_Impl for NotificationClient_Impl {
    fn OnDeviceStateChanged(
        &self,
        device_id: &PCWSTR,
        new_state: DEVICE_STATE,
    ) -> windows_core::Result<()> {
        let mut event = AudioEvent::device(AudioEventKind::DeviceStateChanged, device_id);
        event.state = Some(new_state.0);
        self.publish(event);
        Ok(())
    }

    fn OnDeviceAdded(&self, device_id: &PCWSTR) -> windows_core::Result<()> {
        self.publish(AudioEvent::device(AudioEventKind::DeviceAdded, device_id));
        Ok(())
    }

    fn OnDeviceRemoved(&self, device_id: &PCWSTR) -> windows_core::Result<()> {
        self.publish(AudioEvent::device(AudioEventKind::DeviceRemoved, device_id));
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        device_id: &PCWSTR,
    ) -> windows_core::Result<()> {
        self.publish(AudioEvent {
            kind: AudioEventKind::DefaultDeviceChanged,
            device_id: pcwstr_to_string(device_id),
            flow: data_flow(flow),
            role: audio_role(role),
            state: None,
        });
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        device_id: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows_core::Result<()> {
        self.publish(AudioEvent::device(
            AudioEventKind::PropertyChanged,
            device_id,
        ));
        Ok(())
    }
}

pub struct NotificationSubscription {
    enumerator: IMMDeviceEnumerator,
    callback: IMMNotificationClient,
    receiver: Receiver<AudioEvent>,
}

impl NotificationSubscription {
    pub(super) fn register(enumerator: &IMMDeviceEnumerator) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let callback: IMMNotificationClient = NotificationClient { sender }.into();
        // SAFETY: callback remains alive in the returned subscription until it is unregistered.
        unsafe { enumerator.RegisterEndpointNotificationCallback(&callback) }
            .context("Core Audio変更通知の登録に失敗しました")?;
        Ok(Self {
            enumerator: enumerator.clone(),
            callback,
            receiver,
        })
    }

    pub fn try_recv(&self) -> std::result::Result<AudioEvent, TryRecvError> {
        self.receiver.try_recv()
    }
}

impl Drop for NotificationSubscription {
    fn drop(&mut self) {
        // SAFETY: this is the same enumerator and callback pair used for registration.
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.callback)
        };
    }
}

#[derive(Debug)]
pub struct NotificationDebouncer {
    debounce: Duration,
    deadline: Option<Instant>,
    events: Vec<AudioEvent>,
}

impl NotificationDebouncer {
    pub fn new() -> Self {
        Self::with_duration(NOTIFICATION_DEBOUNCE)
    }

    fn with_duration(debounce: Duration) -> Self {
        Self {
            debounce,
            deadline: None,
            events: Vec::new(),
        }
    }

    pub fn push(&mut self, event: AudioEvent) {
        self.push_at(event, Instant::now());
    }

    fn push_at(&mut self, event: AudioEvent, now: Instant) {
        self.events.push(event);
        self.deadline = Some(now + self.debounce);
    }

    pub fn take_due(&mut self) -> Option<Vec<AudioEvent>> {
        self.take_due_at(Instant::now())
    }

    fn take_due_at(&mut self, now: Instant) -> Option<Vec<AudioEvent>> {
        if self.deadline.is_none_or(|deadline| now < deadline) {
            return None;
        }
        self.deadline = None;
        Some(std::mem::take(&mut self.events))
    }
}

impl Default for NotificationDebouncer {
    fn default() -> Self {
        Self::new()
    }
}

fn pcwstr_to_string(value: &PCWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: Core Audio supplies a null-terminated string for the callback duration.
    unsafe { value.to_string().ok() }
}

fn data_flow(flow: EDataFlow) -> Option<DataFlow> {
    if flow == eRender {
        Some(DataFlow::Render)
    } else if flow == eCapture {
        Some(DataFlow::Capture)
    } else {
        None
    }
}

fn audio_role(role: ERole) -> Option<AudioRole> {
    if role == eConsole {
        Some(AudioRole::Console)
    } else if role == eMultimedia {
        Some(AudioRole::Multimedia)
    } else if role == eCommunications {
        Some(AudioRole::Communications)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: AudioEventKind) -> AudioEvent {
        AudioEvent {
            kind,
            device_id: Some("test-device".into()),
            flow: None,
            role: None,
            state: None,
        }
    }

    #[test]
    fn debounce_waits_250_ms_after_last_event() {
        let start = Instant::now();
        let mut debounce = NotificationDebouncer::new();
        debounce.push_at(event(AudioEventKind::DeviceAdded), start);
        debounce.push_at(
            event(AudioEventKind::DeviceStateChanged),
            start + Duration::from_millis(200),
        );

        assert!(
            debounce
                .take_due_at(start + Duration::from_millis(449))
                .is_none()
        );
        let events = debounce
            .take_due_at(start + Duration::from_millis(450))
            .expect("the batch should be due");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, AudioEventKind::DeviceAdded);
        assert_eq!(events[1].kind, AudioEventKind::DeviceStateChanged);
    }

    #[test]
    fn debounce_emits_each_batch_once() {
        let start = Instant::now();
        let mut debounce = NotificationDebouncer::with_duration(Duration::from_millis(10));
        debounce.push_at(event(AudioEventKind::DeviceRemoved), start);

        assert_eq!(
            debounce
                .take_due_at(start + Duration::from_millis(10))
                .expect("the batch should be due")
                .len(),
            1
        );
        assert!(
            debounce
                .take_due_at(start + Duration::from_secs(1))
                .is_none()
        );
    }

    #[test]
    fn callback_queue_is_safe_across_threads() {
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            sender
                .send(event(AudioEventKind::DefaultDeviceChanged))
                .expect("receiver remains alive");
        });
        worker.join().expect("worker should finish");

        assert_eq!(
            receiver.recv().expect("event should arrive").kind,
            AudioEventKind::DefaultDeviceChanged
        );
    }
}
