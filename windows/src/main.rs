use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use audio_selector::audio::{
    AudioEventKind, AudioService, DataFlow, Endpoint, NotificationDebouncer,
    NotificationSubscription, RoleDefaults,
};
use audio_selector::bluetooth;
use audio_selector::config;
use audio_selector::protocol::{
    EndpointRecord, Message, RoleDefaults as ProtocolRoleDefaults, SetRequest, SetResult,
};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

mod runtime;

fn main() -> Result<()> {
    let log_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("audio_selector=info"));
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .with_target(false)
        .compact()
        .init();

    info!(
        application_version = config::APPLICATION_VERSION,
        protocol_version = config::PROTOCOL_VERSION,
        "Audio Selector started"
    );

    let command = parse_command()?;
    if command == Command::FindBluetooth {
        let device = bluetooth::find_paired_audio_selector()?;
        info!(
            name = %device.name,
            address = format_args!("{:012X}", device.address),
            connected = device.connected,
            remembered = device.remembered,
            authenticated = device.authenticated,
            spp = device.has_serial_port_service,
            "対象Bluetoothデバイスを特定しました"
        );
        if !device.has_serial_port_service {
            bail!("AudioSelectorにSerial Port Profileサービスがありません");
        }
        return Ok(());
    }
    if command == Command::TestSpp {
        let mut connection = bluetooth::SppConnection::connect_audio_selector()?;
        let response = connection.handshake(0xA510_0001, Duration::from_secs(2))?;
        info!(
            selected_version = response.selected_version,
            device_type = response.device_type,
            firmware = format_args!(
                "{}.{}.{}",
                response.firmware_major, response.firmware_minor, response.firmware_patch
            ),
            boot_id = response.boot_id,
            "Bluetooth SPP HELLOハンドシェイクに成功しました"
        );
        let audio = AudioService::new()?;
        let _ = send_full_sync(&mut connection, &audio, 1)?;
        connection.ping(0xA510_0002, Duration::from_secs(2))?;
        info!(generation = 1, "初回完全同期とPING/PONGに成功しました");
        return Ok(());
    }
    if let Command::RunSpp { duration } = command {
        let audio = AudioService::new()?;
        run_spp_session_loop(&audio, Some(duration))?;
        return Ok(());
    }
    if command == Command::Run {
        let _single_instance = runtime::SingleInstance::acquire()?;
        runtime::install_shutdown_handler()?;
        let audio = AudioService::new()?;
        run_spp_session_loop(&audio, None)?;
        return Ok(());
    }
    let audio = AudioService::new()?;

    match command {
        Command::Read => log_audio_state(&audio)?,
        Command::Set { flow, handle } => {
            log_audio_state(&audio)?;
            set_default(&audio, flow, handle)?;
        }
        Command::Watch { duration } => watch_changes(&audio, duration)?,
        Command::FindBluetooth => unreachable!("handled before Core Audio initialization"),
        Command::TestSpp => unreachable!("handled before Core Audio initialization"),
        Command::RunSpp { .. } => unreachable!("handled before Core Audio initialization"),
        Command::Run => unreachable!("handled before Core Audio initialization"),
    }

    Ok(())
}

fn send_full_sync(
    connection: &mut bluetooth::SppConnection,
    audio: &AudioService,
    generation: u32,
) -> Result<AudioSnapshot> {
    let snapshot = capture_audio_snapshot(audio)?;
    send_audio_snapshot(connection, &snapshot, generation)?;
    Ok(snapshot)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AudioSnapshot {
    outputs: Vec<Endpoint>,
    inputs: Vec<Endpoint>,
    output_defaults: RoleDefaults,
    input_defaults: RoleDefaults,
}

fn capture_audio_snapshot(audio: &AudioService) -> Result<AudioSnapshot> {
    let outputs = audio.enumerate_active(DataFlow::Render)?;
    let inputs = audio.enumerate_active(DataFlow::Capture)?;
    let output_defaults = audio.default_endpoints(DataFlow::Render, &outputs)?;
    let input_defaults = audio.default_endpoints(DataFlow::Capture, &inputs)?;
    Ok(AudioSnapshot {
        outputs,
        inputs,
        output_defaults,
        input_defaults,
    })
}

fn send_audio_snapshot(
    connection: &mut bluetooth::SppConnection,
    snapshot: &AudioSnapshot,
    generation: u32,
) -> Result<()> {
    connection.send_message(&Message::SyncBegin {
        generation,
        output_count: u16::try_from(snapshot.outputs.len())?,
        input_count: u16::try_from(snapshot.inputs.len())?,
    })?;
    for endpoint in &snapshot.outputs {
        connection.send_message(&Message::OutputEndpoint(EndpointRecord {
            generation,
            handle: endpoint.handle,
            friendly_name: endpoint.name.clone(),
        }))?;
    }
    for endpoint in &snapshot.inputs {
        connection.send_message(&Message::InputEndpoint(EndpointRecord {
            generation,
            handle: endpoint.handle,
            friendly_name: endpoint.name.clone(),
        }))?;
    }
    connection.send_message(&Message::CurrentOutput(protocol_roles(
        generation,
        &snapshot.output_defaults,
    )))?;
    connection.send_message(&Message::CurrentInput(protocol_roles(
        generation,
        &snapshot.input_defaults,
    )))?;
    connection.send_message(&Message::SyncEnd { generation })?;
    Ok(())
}

fn protocol_roles(
    generation: u32,
    defaults: &audio_selector::audio::RoleDefaults,
) -> ProtocolRoleDefaults {
    ProtocolRoleDefaults {
        generation,
        console: defaults.console.handle,
        multimedia: defaults.multimedia.handle,
        communications: defaults.communications.handle,
    }
}

fn run_spp_session_loop(audio: &AudioService, duration: Option<Duration>) -> Result<()> {
    let finish = duration.map(|duration| Instant::now() + duration);
    let mut backoff = bluetooth::ReconnectBackoff::default();
    let mut session_number = 0u32;
    let notifications = audio.subscribe()?;
    if let Some(duration) = duration {
        info!(seconds = duration.as_secs(), "SPP常駐通信を開始します");
    } else {
        info!("SPP常駐通信を開始します");
    }

    while !runtime::shutdown_requested() && finish.is_none_or(|finish| Instant::now() < finish) {
        session_number = session_number.wrapping_add(1).max(1);
        drain_audio_notifications(&notifications, None);
        match run_spp_session(audio, &notifications, finish, session_number, &mut backoff) {
            Ok(()) => break,
            Err(error) => {
                warn!(session_number, error = %error, "SPPセッションが終了しました");
                let delay = backoff.next_delay();
                let remaining = finish
                    .map(|finish| finish.saturating_duration_since(Instant::now()))
                    .unwrap_or(delay);
                if remaining.is_zero() {
                    break;
                }
                let actual_delay = delay.min(remaining);
                info!(
                    delay_seconds = actual_delay.as_secs_f32(),
                    "SPP再接続を待機します"
                );
                interruptible_sleep(actual_delay);
            }
        }
    }
    info!("SPP常駐通信を終了します");
    Ok(())
}

fn run_spp_session(
    audio: &AudioService,
    notifications: &NotificationSubscription,
    finish: Option<Instant>,
    session_number: u32,
    backoff: &mut bluetooth::ReconnectBackoff,
) -> Result<()> {
    let mut connection = bluetooth::SppConnection::connect_audio_selector()?;
    let nonce = 0xA510_0000 | (session_number & 0xFFFF);
    let response = connection.handshake(nonce, bluetooth::RESPONSE_TIMEOUT)?;
    let mut generation = 1;
    let mut snapshot = send_full_sync(&mut connection, audio, generation)?;
    backoff.reset();
    info!(
        session_number,
        boot_id = response.boot_id,
        "SPPセッションを確立しました"
    );

    let mut token = nonce;
    let mut keepalive_deadline = Instant::now() + bluetooth::KEEPALIVE_IDLE;
    let mut notification_debouncer = NotificationDebouncer::new();
    loop {
        if runtime::shutdown_requested() {
            return Ok(());
        }
        let remaining = finish
            .map(|finish| finish.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::MAX);
        if remaining.is_zero() {
            return Ok(());
        }
        let until_keepalive = keepalive_deadline.saturating_duration_since(Instant::now());
        let wait = remaining
            .min(until_keepalive)
            .min(Duration::from_millis(50));
        let messages = connection.receive_messages(wait)?;
        drain_audio_notifications(notifications, Some(&mut notification_debouncer));
        if let Some(events) = notification_debouncer.take_due() {
            let latest = capture_audio_snapshot(audio)?;
            if latest != snapshot {
                generation = next_generation(generation);
                send_audio_snapshot(&mut connection, &latest, generation)?;
                snapshot = latest;
                info!(
                    generation,
                    event_count = events.len(),
                    "Core Audio変更を完全同期しました"
                );
            } else {
                debug!(
                    event_count = events.len(),
                    "実状態に変化のないCore Audio通知を無視しました"
                );
            }
        }
        if finish.is_some_and(|finish| Instant::now() >= finish) {
            return Ok(());
        }
        if !messages.is_empty() {
            process_session_messages(
                &mut connection,
                audio,
                &mut generation,
                &mut snapshot,
                messages,
                None,
            )?;
            keepalive_deadline = Instant::now() + bluetooth::KEEPALIVE_IDLE;
            continue;
        }
        if Instant::now() < keepalive_deadline {
            continue;
        }

        let mut pong_received = false;
        for attempt in 1..=2 {
            token = token.wrapping_add(1);
            connection.send_message(&Message::Ping { token })?;
            let deadline = Instant::now() + bluetooth::RESPONSE_TIMEOUT;
            while Instant::now() < deadline {
                let messages = connection
                    .receive_messages(deadline.saturating_duration_since(Instant::now()))?;
                if messages.is_empty() {
                    break;
                }
                if process_session_messages(
                    &mut connection,
                    audio,
                    &mut generation,
                    &mut snapshot,
                    messages,
                    Some(token),
                )? {
                    pong_received = true;
                    break;
                }
            }
            if pong_received {
                debug!(session_number, token, "SPP PING/PONG成功");
                keepalive_deadline = Instant::now() + bluetooth::KEEPALIVE_IDLE;
                break;
            }
            warn!(
                session_number,
                token, attempt, "SPP PING応答がタイムアウトしました"
            );
        }
        if !pong_received {
            bail!("SPP PINGが2回連続でタイムアウトしました");
        }
    }
}

fn interruptible_sleep(duration: Duration) {
    let deadline = Instant::now() + duration;
    while !runtime::shutdown_requested() && Instant::now() < deadline {
        thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50)),
        );
    }
}

fn drain_audio_notifications(
    notifications: &NotificationSubscription,
    mut debouncer: Option<&mut NotificationDebouncer>,
) {
    while let Ok(event) = notifications.try_recv() {
        if let Some(debouncer) = debouncer.as_deref_mut() {
            debouncer.push(event);
        }
    }
}

fn process_session_messages(
    connection: &mut bluetooth::SppConnection,
    audio: &AudioService,
    generation: &mut u32,
    snapshot: &mut AudioSnapshot,
    messages: Vec<Message>,
    expected_pong: Option<u32>,
) -> Result<bool> {
    let mut pong_received = false;
    for message in messages {
        match message {
            Message::SetOutputRequest(request) => {
                handle_set_request(
                    connection,
                    audio,
                    DataFlow::Render,
                    request,
                    generation,
                    snapshot,
                )?;
            }
            Message::SetInputRequest(request) => {
                handle_set_request(
                    connection,
                    audio,
                    DataFlow::Capture,
                    request,
                    generation,
                    snapshot,
                )?;
            }
            Message::SyncRequest { reason } => {
                *generation = next_generation(*generation);
                *snapshot = send_full_sync(connection, audio, *generation)?;
                info!(
                    reason,
                    generation = *generation,
                    "同期要求へ完全同期を返しました"
                );
            }
            Message::Ping { token } => connection.send_message(&Message::Pong { token })?,
            Message::Pong { token } if Some(token) == expected_pong => pong_received = true,
            Message::Pong { token } => debug!(token, "未要求のPONGを無視しました"),
            Message::Error(error) => warn!(?error, "ESP32からプロトコルエラーを受信しました"),
            other => bail!("確立済みセッションで想定外のメッセージを受信しました: {other:?}"),
        }
    }
    Ok(pong_received)
}

fn handle_set_request(
    connection: &mut bluetooth::SppConnection,
    audio: &AudioService,
    flow: DataFlow,
    request: SetRequest,
    generation: &mut u32,
    snapshot: &mut AudioSnapshot,
) -> Result<()> {
    let operation = if flow == DataFlow::Render { 0x00 } else { 0x01 };
    let (status, error_code) = if let Some(rejection) = stale_generation(request, *generation) {
        rejection
    } else {
        match audio.enumerate_active(flow) {
            Ok(endpoints) => match requested_endpoint(request, &endpoints) {
                Ok(endpoint) => match audio.default_endpoints(flow, &endpoints) {
                    Ok(defaults) if all_roles_match(&defaults, request.handle) => {
                        debug!(
                            ?flow,
                            handle = request.handle,
                            "既定Endpointと同じためWindows API呼び出しを省略しました"
                        );
                        (0x00, 0x0000)
                    }
                    Ok(_) => match audio.set_default_all_roles(flow, endpoint, &endpoints) {
                        Ok(outcome) => {
                            let result = outcome.protocol_result();
                            (result.status, result.error_code)
                        }
                        Err(error) => {
                            warn!(?flow, handle = request.handle, error = %error, "既定Endpointの設定に失敗しました");
                            (0x01, 0x000C)
                        }
                    },
                    Err(error) => {
                        warn!(?flow, error = %error, "既定Endpointの取得に失敗しました");
                        (0x01, 0x000C)
                    }
                },
                Err(rejection) => rejection,
            },
            Err(error) => {
                warn!(?flow, error = %error, "設定要求用のEndpoint列挙に失敗しました");
                (0x01, 0x000C)
            }
        }
    };

    connection.send_message(&Message::SetResult(SetResult {
        request_id: request.request_id,
        operation,
        status,
        error_code,
        known_generation: *generation,
    }))?;
    info!(
        ?flow,
        request_id = request.request_id,
        handle = request.handle,
        status,
        error_code,
        "設定要求を処理しました"
    );

    *generation = next_generation(*generation);
    *snapshot = send_full_sync(connection, audio, *generation)?;
    Ok(())
}

fn stale_generation(request: SetRequest, current_generation: u32) -> Option<(u8, u16)> {
    (request.generation != current_generation).then_some((0x02, 0x000A))
}

fn requested_endpoint(
    request: SetRequest,
    endpoints: &[Endpoint],
) -> std::result::Result<&Endpoint, (u8, u16)> {
    endpoints
        .iter()
        .find(|endpoint| endpoint.handle == request.handle)
        .ok_or((0x03, 0x000B))
}

fn all_roles_match(defaults: &RoleDefaults, handle: u16) -> bool {
    defaults.console.handle == handle
        && defaults.multimedia.handle == handle
        && defaults.communications.handle == handle
}

fn next_generation(current: u32) -> u32 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    Read,
    Run,
    Set { flow: DataFlow, handle: u16 },
    Watch { duration: Duration },
    FindBluetooth,
    TestSpp,
    RunSpp { duration: Duration },
}

fn log_audio_state(audio: &AudioService) -> Result<()> {
    for flow in [DataFlow::Render, DataFlow::Capture] {
        let endpoints = audio.enumerate_active(flow)?;
        let defaults = audio.default_endpoints(flow, &endpoints)?;
        info!(
            ?flow,
            count = endpoints.len(),
            "有効なEndpointを列挙しました"
        );
        for endpoint in &endpoints {
            info!(
                ?flow,
                handle = endpoint.handle,
                endpoint_id = %endpoint.id,
                name = %endpoint.name,
                state = endpoint.state,
                "Endpoint"
            );
        }
        info!(
            ?flow,
            console_handle = defaults.console.handle,
            multimedia_handle = defaults.multimedia.handle,
            communications_handle = defaults.communications.handle,
            role_split = defaults.is_split(),
            "既定Endpointを取得しました"
        );
    }
    Ok(())
}

fn set_default(audio: &AudioService, flow: DataFlow, handle: u16) -> Result<()> {
    let endpoints = audio.enumerate_active(flow)?;
    let endpoint = endpoints
        .iter()
        .find(|endpoint| endpoint.handle == handle)
        .with_context(|| format!("{flow:?}にハンドル{handle}が存在しません"))?;
    let outcome = audio.set_default_all_roles(flow, endpoint, &endpoints)?;
    info!(
        ?flow,
        handle,
        verified = outcome.verified,
        partial_success = outcome.partial_success,
        failed_roles = ?outcome.failed_roles,
        "既定Endpoint変更を検証しました"
    );
    if !outcome.verified {
        bail!("3ロールすべての既定Endpoint変更を検証できませんでした");
    }
    Ok(())
}

fn watch_changes(audio: &AudioService, duration: Duration) -> Result<()> {
    let subscription = audio.subscribe()?;
    let mut debouncer = NotificationDebouncer::new();
    let finish = Instant::now() + duration;
    info!(
        seconds = duration.as_secs(),
        "Core Audio変更通知を監視します"
    );
    // Register before taking the initial snapshot so changes during enumeration stay queued.
    log_audio_state(audio)?;

    while Instant::now() < finish {
        loop {
            match subscription.try_recv() {
                Ok(event) => {
                    debug!(?event, "Core Audio変更通知を受信しました");
                    debouncer.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    bail!("Core Audio変更通知キューが切断されました");
                }
            }
        }

        if let Some(events) = debouncer.take_due() {
            let count = |kind| events.iter().filter(|event| event.kind == kind).count();
            info!(
                event_count = events.len(),
                device_added = count(AudioEventKind::DeviceAdded),
                device_removed = count(AudioEventKind::DeviceRemoved),
                state_changed = count(AudioEventKind::DeviceStateChanged),
                default_changed = count(AudioEventKind::DefaultDeviceChanged),
                property_changed = count(AudioEventKind::PropertyChanged),
                "変更通知をまとめて再同期します"
            );
            log_audio_state(audio)?;
        }
        thread::sleep(Duration::from_millis(10));
    }

    info!("Core Audio変更通知の監視を終了します");
    Ok(())
}

fn parse_command() -> Result<Command> {
    let mut arguments = std::env::args().skip(1);
    let Some(option) = arguments.next() else {
        return Ok(Command::Read);
    };
    if option == "--watch" {
        let seconds = arguments
            .next()
            .context("監視秒数を指定してください")?
            .parse::<u64>()
            .context("監視秒数は1以上の整数で指定してください")?;
        if seconds == 0 || arguments.next().is_some() {
            usage_error()?;
        }
        return Ok(Command::Watch {
            duration: Duration::from_secs(seconds),
        });
    }
    if option == "--find-bluetooth" && arguments.next().is_none() {
        return Ok(Command::FindBluetooth);
    }
    if option == "--run" && arguments.next().is_none() {
        return Ok(Command::Run);
    }
    if option == "--test-spp" && arguments.next().is_none() {
        return Ok(Command::TestSpp);
    }
    if option == "--run-spp" {
        let seconds = arguments
            .next()
            .context("SPP常駐通信の実行秒数を指定してください")?
            .parse::<u64>()
            .context("実行秒数は1以上の整数で指定してください")?;
        if seconds == 0 || arguments.next().is_some() {
            return usage_error();
        }
        return Ok(Command::RunSpp {
            duration: Duration::from_secs(seconds),
        });
    }
    let flow = match option.as_str() {
        "--set-output" => DataFlow::Render,
        "--set-input" => DataFlow::Capture,
        _ => return usage_error(),
    };
    let handle = arguments
        .next()
        .context("変更先ハンドルを指定してください")?
        .parse::<u16>()
        .context("変更先ハンドルは1以上の整数で指定してください")?;
    if handle == 0 || arguments.next().is_some() {
        return usage_error();
    }
    Ok(Command::Set { flow, handle })
}

fn usage_error<T>() -> Result<T> {
    bail!(
        "使用方法: audio-selector [--run | --set-output HANDLE | --set-input HANDLE | --watch SECONDS | --find-bluetooth | --test-spp | --run-spp SECONDS]"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_variants_remain_distinct() {
        assert_ne!(
            Command::Read,
            Command::Watch {
                duration: Duration::from_secs(1)
            }
        );
        assert_ne!(
            Command::Set {
                flow: DataFlow::Render,
                handle: 1
            },
            Command::Set {
                flow: DataFlow::Capture,
                handle: 1
            }
        );
    }

    #[test]
    fn generation_wraps_to_one() {
        assert_eq!(next_generation(1), 2);
        assert_eq!(next_generation(u32::MAX), 1);
    }

    #[test]
    fn matching_all_roles_is_required_for_no_op_set() {
        use audio_selector::audio::DefaultEndpoint;

        let endpoint = |handle| DefaultEndpoint {
            id: Some(format!("endpoint-{handle}")),
            handle,
        };
        let same = RoleDefaults {
            console: endpoint(2),
            multimedia: endpoint(2),
            communications: endpoint(2),
        };
        assert!(all_roles_match(&same, 2));

        let split = RoleDefaults {
            communications: endpoint(1),
            ..same
        };
        assert!(!all_roles_match(&split, 2));
    }

    #[test]
    fn stale_generation_is_rejected_before_audio_operation() {
        let request = SetRequest {
            request_id: 1,
            generation: 9,
            handle: 2,
        };
        assert_eq!(stale_generation(request, 10), Some((0x02, 0x000A)));
        assert_eq!(stale_generation(request, 9), None);
    }

    #[test]
    fn invalid_handle_does_not_resolve_to_an_endpoint() {
        let endpoints = vec![Endpoint {
            handle: 1,
            id: "endpoint-1".into(),
            name: "Speakers".into(),
            flow: DataFlow::Render,
            state: 1,
        }];
        let request = SetRequest {
            request_id: 1,
            generation: 1,
            handle: 2,
        };
        assert_eq!(
            requested_endpoint(request, &endpoints).unwrap_err(),
            (0x03, 0x000B)
        );
    }
}
