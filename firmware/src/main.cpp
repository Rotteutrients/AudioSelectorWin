#include <Arduino.h>
#include <LovyanGFX.hpp>

#include <cstdint>
#include <cstring>

#include "bluetooth/spp_transport.h"
#include "config.h"
#include "display/display.h"
#include "input/jog_switch.h"
#include "protocol/message.h"
#include "protocol/parser.h"
#include "state/device_state.h"

namespace {

audio_selector::display::Display display;
audio_selector::input::JogSwitch jog_switch;
audio_selector::state::DeviceState device_state;
audio_selector::bluetooth::SppTransport spp_transport;
audio_selector::protocol::StreamParser stream_parser;

constexpr const char* kOutputNames[] = {
    "スピーカー (Realtek High Definition Audio)",
    "ヘッドホン (USB DAC)",
    "ディスプレイ音声 (HDMI Audio Output)"};
constexpr const char* kInputNames[] = {
    "マイク (USB Audio Device)",
    "マイク配列 (Realtek High Definition Audio)",
    "ライン入力"};

audio_selector::display::Focus focus =
    audio_selector::display::Focus::kOutput;
audio_selector::display::ConnectionStatus connection_status =
    audio_selector::display::ConnectionStatus::kDisconnected;
const char* error_message = nullptr;
std::uint32_t error_until_ms = 0;

enum class ScreenMode {
  kMain,
  kSettings,
  kRestartConfirmation,
  kBluetoothInfo,
  kFirmwareInfo,
};

ScreenMode screen_mode = ScreenMode::kMain;
std::size_t settings_index = 0;
bool restart_selected = false;
std::uint32_t boot_id = 0;
bool handshake_complete = false;

bool initializeDemoState() {
  using audio_selector::state::CurrentHandles;
  using audio_selector::state::DataFlow;

  if (!device_state.beginSync(1, 3, 3)) {
    return false;
  }
  for (std::size_t index = 0; index < 3; ++index) {
    if (!device_state.addEndpoint(DataFlow::kOutput, index + 1,
                                  kOutputNames[index],
                                  std::strlen(kOutputNames[index])) ||
        !device_state.addEndpoint(DataFlow::kInput, index + 1,
                                  kInputNames[index],
                                  std::strlen(kInputNames[index]))) {
      return false;
    }
  }
  return device_state.setCurrent(DataFlow::kOutput,
                                 CurrentHandles{1, 1, 1}) &&
         device_state.setCurrent(DataFlow::kInput, CurrentHandles{1, 1, 1}) &&
         device_state.commitSync(1);
}

audio_selector::display::MainScreenView makeDemoView() {
  using audio_selector::state::DataFlow;
  return {
      focus,
      {device_state.currentName(DataFlow::kOutput),
       device_state.candidateName(DataFlow::kOutput),
       device_state.rolesSplit(DataFlow::kOutput)},
      {device_state.currentName(DataFlow::kInput),
       device_state.candidateName(DataFlow::kInput),
       device_state.rolesSplit(DataFlow::kInput)},
      connection_status,
      error_message,
  };
}

void drawDemoScreen() {
  display.showMainScreen(makeDemoView(), millis());
}

bool prepareSetRequest(audio_selector::state::DataFlow data_flow) {
  audio_selector::state::SetRequest request{};
  if (!device_state.beginSetRequest(data_flow, request)) {
    return false;
  }

  audio_selector::protocol::Message message{};
  message.type = data_flow == audio_selector::state::DataFlow::kOutput
                     ? audio_selector::protocol::MessageType::kSetOutputRequest
                     : audio_selector::protocol::MessageType::kSetInputRequest;
  message.requestId = request.request_id;
  message.generation = request.generation;
  message.handle = request.handle;

  audio_selector::protocol::Frame frame{};
  if (audio_selector::protocol::encodeMessage(message, frame) !=
          audio_selector::protocol::CodecError::kNone ||
      !spp_transport.enqueue(frame)) {
    device_state.cancelSetRequest();
    return false;
  }

  Serial.printf("Set request prepared: id=%lu generation=%lu handle=%u\n",
                static_cast<unsigned long>(request.request_id),
                static_cast<unsigned long>(request.generation), request.handle);
  return true;
}

bool sendProtocolMessage(const audio_selector::protocol::Message& message) {
  audio_selector::protocol::Frame frame{};
  return audio_selector::protocol::encodeMessage(message, frame) ==
             audio_selector::protocol::CodecError::kNone &&
         spp_transport.enqueue(frame);
}

void handleProtocolFrame(const audio_selector::protocol::Frame& frame,
                         void*) {
  audio_selector::protocol::Message message{};
  const auto error = audio_selector::protocol::decodeMessage(frame, message);
  if (error != audio_selector::protocol::CodecError::kNone) {
    Serial.printf("Protocol decode error: %u\n",
                  static_cast<unsigned>(error));
    return;
  }
  using audio_selector::protocol::MessageType;
  switch (message.type) {
    case MessageType::kHelloRequest: {
      if (message.minimumVersion > audio_selector::config::kProtocolVersion ||
          message.maximumVersion < audio_selector::config::kProtocolVersion) {
        Serial.println("HELLO rejected: unsupported protocol version");
        return;
      }
      audio_selector::protocol::Message response{};
      response.type = MessageType::kHelloResponse;
      response.selectedVersion = audio_selector::config::kProtocolVersion;
      response.deviceType = audio_selector::config::kDeviceType;
      response.firmwareMajor =
          audio_selector::config::kFirmwareVersionMajor;
      response.firmwareMinor =
          audio_selector::config::kFirmwareVersionMinor;
      response.firmwarePatch =
          audio_selector::config::kFirmwareVersionPatch;
      response.capabilities = 0;
      response.echoedHostNonce = message.hostNonce;
      response.bootId = boot_id;
      if (!sendProtocolMessage(response)) {
        Serial.println("HELLO response enqueue failed");
        return;
      }
      handshake_complete = true;
      connection_status =
          audio_selector::display::ConnectionStatus::kConnected;
      Serial.printf("HELLO completed: nonce=%lu boot_id=%lu\n",
                    static_cast<unsigned long>(message.hostNonce),
                    static_cast<unsigned long>(boot_id));
      if (screen_mode == ScreenMode::kMain) {
        drawDemoScreen();
      }
      break;
    }
    case MessageType::kPing: {
      audio_selector::protocol::Message response{};
      response.type = MessageType::kPong;
      response.token = message.token;
      if (!sendProtocolMessage(response)) {
        Serial.println("PONG enqueue failed");
      }
      break;
    }
    case MessageType::kSyncBegin:
      if (handshake_complete &&
          device_state.beginSync(message.generation, message.outputCount,
                                 message.inputCount)) {
        Serial.printf("Sync begin: generation=%lu output=%u input=%u\n",
                      static_cast<unsigned long>(message.generation),
                      message.outputCount, message.inputCount);
      } else {
        Serial.println("Sync begin rejected");
      }
      break;
    case MessageType::kOutputEndpoint:
    case MessageType::kInputEndpoint: {
      const auto flow = message.type == MessageType::kOutputEndpoint
                            ? audio_selector::state::DataFlow::kOutput
                            : audio_selector::state::DataFlow::kInput;
      if (!handshake_complete ||
          !device_state.addEndpoint(
              flow, message.handle,
              reinterpret_cast<const char*>(message.text.data()),
              message.textLength)) {
        Serial.println("Sync endpoint rejected");
      }
      break;
    }
    case MessageType::kCurrentOutput:
    case MessageType::kCurrentInput: {
      const auto flow = message.type == MessageType::kCurrentOutput
                            ? audio_selector::state::DataFlow::kOutput
                            : audio_selector::state::DataFlow::kInput;
      const audio_selector::state::CurrentHandles handles{
          message.consoleHandle, message.multimediaHandle,
          message.communicationsHandle};
      if (!handshake_complete || !device_state.setCurrent(flow, handles)) {
        Serial.println("Sync current handles rejected");
      }
      break;
    }
    case MessageType::kSyncEnd:
      if (handshake_complete && device_state.commitSync(message.generation)) {
        Serial.printf("Sync committed: generation=%lu\n",
                      static_cast<unsigned long>(message.generation));
        if (screen_mode == ScreenMode::kMain) {
          drawDemoScreen();
        }
      } else {
        device_state.cancelSync();
        Serial.println("Sync commit rejected");
      }
      break;
    case MessageType::kSetResult:
      if (handshake_complete &&
          device_state.finishSetRequest(message.requestId)) {
        Serial.printf(
            "Set result: id=%lu operation=%u status=%u error=0x%04x "
            "generation=%lu\n",
            static_cast<unsigned long>(message.requestId), message.operation,
            message.status, message.errorCode,
            static_cast<unsigned long>(message.knownGeneration));
      } else {
        Serial.println("Set result rejected");
      }
      break;
    case MessageType::kPong:
    case MessageType::kError:
      break;
    default:
      if (!handshake_complete) {
        Serial.printf("Message before HELLO ignored: 0x%02x\n",
                      static_cast<unsigned>(message.type));
      } else {
        Serial.printf("Protocol message received: 0x%02x\n",
                      static_cast<unsigned>(message.type));
      }
      break;
  }
}

void handleProtocolParseError(audio_selector::protocol::ParseError error,
                              void*) {
  Serial.printf("Protocol parse error: %u\n", static_cast<unsigned>(error));
}

void handleConnectionEvent(audio_selector::bluetooth::ConnectionEvent event) {
  using audio_selector::bluetooth::ConnectionEvent;
  if (event == ConnectionEvent::kNone) {
    return;
  }
  stream_parser.clear();
  handshake_complete = false;
  error_message = nullptr;
  if (event == ConnectionEvent::kConnected) {
    // A new transport connection starts a new handle/generation namespace.
    device_state.clear();
    // The UI remains disconnected until the protocol HELLO handshake completes.
    connection_status =
        audio_selector::display::ConnectionStatus::kDisconnected;
    Serial.println("Bluetooth SPP connected");
  } else {
    // Keep the last names for display, but invalidate every session operation.
    device_state.disconnectSession();
    connection_status =
        audio_selector::display::ConnectionStatus::kDisconnected;
    Serial.println("Bluetooth SPP disconnected");
  }
  if (screen_mode == ScreenMode::kMain) {
    drawDemoScreen();
  }
}

void showSettings() {
  screen_mode = ScreenMode::kSettings;
  display.showSettingsScreen(settings_index);
}

void showMain() {
  screen_mode = ScreenMode::kMain;
  drawDemoScreen();
}

void activateSettingsItem() {
  switch (settings_index) {
    case 0:
      Serial.println("Reconnect requested");
      stream_parser.clear();
      device_state.clear();
      connection_status =
          audio_selector::display::ConnectionStatus::kDisconnected;
      if (!spp_transport.reconnect()) {
        connection_status = audio_selector::display::ConnectionStatus::kError;
        error_message = "SPP RESTART FAILED";
        error_until_ms = millis() + 2000;
      }
      showMain();
      break;
    case 1:
      screen_mode = ScreenMode::kRestartConfirmation;
      restart_selected = false;
      display.showRestartConfirmation(restart_selected);
      break;
    case 2:
      screen_mode = ScreenMode::kBluetoothInfo;
      display.showBluetoothInfo(
          audio_selector::config::kBluetoothDeviceName,
          connection_status ==
              audio_selector::display::ConnectionStatus::kConnected);
      break;
    case 3:
      screen_mode = ScreenMode::kFirmwareInfo;
      display.showFirmwareInfo(
          audio_selector::config::kFirmwareVersionMajor,
          audio_selector::config::kFirmwareVersionMinor,
          audio_selector::config::kFirmwareVersionPatch,
          audio_selector::config::kProtocolVersion,
          audio_selector::config::kDeviceType, __DATE__ " " __TIME__);
      break;
    case 4:
    default:
      showMain();
      break;
  }
}

void handleMainJogEvent(audio_selector::input::JogEvent event) {
  using audio_selector::display::Focus;
  using audio_selector::input::JogEvent;

  bool changed = false;
  switch (event) {
    case JogEvent::kUp:
      focus = Focus::kOutput;
      changed = true;
      break;
    case JogEvent::kDown:
      focus = Focus::kInput;
      changed = true;
      break;
    case JogEvent::kLeft:
      if (focus == Focus::kOutput) {
        device_state.cycleCandidate(audio_selector::state::DataFlow::kOutput,
                                    -1);
      } else {
        device_state.cycleCandidate(audio_selector::state::DataFlow::kInput,
                                    -1);
      }
      changed = true;
      break;
    case JogEvent::kRight:
      if (focus == Focus::kOutput) {
        device_state.cycleCandidate(audio_selector::state::DataFlow::kOutput,
                                    1);
      } else {
        device_state.cycleCandidate(audio_selector::state::DataFlow::kInput,
                                    1);
      }
      changed = true;
      break;
    case JogEvent::kCenterShort:
      if (connection_status ==
          audio_selector::display::ConnectionStatus::kDisconnected) {
        connection_status = audio_selector::display::ConnectionStatus::kError;
        error_message = "PC NOT CONNECTED";
        error_until_ms = millis() + 2000;
        changed = true;
      } else if (connection_status ==
                 audio_selector::display::ConnectionStatus::kConnected) {
        const auto data_flow =
            focus == Focus::kOutput
                ? audio_selector::state::DataFlow::kOutput
                : audio_selector::state::DataFlow::kInput;
        prepareSetRequest(data_flow);
      }
      break;
    case JogEvent::kCenterLong:
      settings_index = 0;
      showSettings();
      return;
    case JogEvent::kNone:
      break;
  }

  if (changed) {
    drawDemoScreen();
  }
}

void handleSettingsJogEvent(audio_selector::input::JogEvent event) {
  using audio_selector::input::JogEvent;
  switch (event) {
    case JogEvent::kUp:
      settings_index = (settings_index + 4) % 5;
      display.showSettingsScreen(settings_index);
      break;
    case JogEvent::kDown:
      settings_index = (settings_index + 1) % 5;
      display.showSettingsScreen(settings_index);
      break;
    case JogEvent::kLeft:
      showMain();
      break;
    case JogEvent::kCenterShort:
      activateSettingsItem();
      break;
    case JogEvent::kRight:
    case JogEvent::kCenterLong:
    case JogEvent::kNone:
      break;
  }
}

void handleRestartJogEvent(audio_selector::input::JogEvent event) {
  using audio_selector::input::JogEvent;
  switch (event) {
    case JogEvent::kUp:
    case JogEvent::kLeft:
      restart_selected = false;
      display.showRestartConfirmation(restart_selected);
      break;
    case JogEvent::kDown:
    case JogEvent::kRight:
      restart_selected = true;
      display.showRestartConfirmation(restart_selected);
      break;
    case JogEvent::kCenterShort:
      if (!restart_selected) {
        showSettings();
      } else {
        display.showRestarting();
        Serial.println("Restarting device");
        delay(250);
        ESP.restart();
      }
      break;
    case JogEvent::kCenterLong:
    case JogEvent::kNone:
      break;
  }
}

void handleInfoJogEvent(audio_selector::input::JogEvent event) {
  using audio_selector::input::JogEvent;
  if (event == JogEvent::kLeft || event == JogEvent::kCenterShort) {
    showSettings();
  }
}

void handleJogEvent(audio_selector::input::JogEvent event) {
  switch (screen_mode) {
    case ScreenMode::kMain:
      handleMainJogEvent(event);
      break;
    case ScreenMode::kSettings:
      handleSettingsJogEvent(event);
      break;
    case ScreenMode::kRestartConfirmation:
      handleRestartJogEvent(event);
      break;
    case ScreenMode::kBluetoothInfo:
    case ScreenMode::kFirmwareInfo:
      handleInfoJogEvent(event);
      break;
  }
}

void printStartupBanner() {
  Serial.printf(
      "AudioSelector firmware %u.%u.%u (protocol %u)\n",
      audio_selector::config::kFirmwareVersionMajor,
      audio_selector::config::kFirmwareVersionMinor,
      audio_selector::config::kFirmwareVersionPatch,
      audio_selector::config::kProtocolVersion);
}

}  // namespace

void setup() {
  Serial.begin(115200);
  boot_id = esp_random();
  if (boot_id == 0) {
    boot_id = 1;
  }
  jog_switch.begin();
  printStartupBanner();
  if (!display.begin()) {
    Serial.println("LCD initialization failed");
    return;
  }
  if (!initializeDemoState()) {
    Serial.println("Demo state initialization failed");
    return;
  }
  display.showStartupScreen();
  delay(800);
  device_state.clear();
  if (!spp_transport.begin(audio_selector::config::kBluetoothDeviceName)) {
    Serial.println("Bluetooth SPP initialization failed");
    connection_status = audio_selector::display::ConnectionStatus::kError;
    error_message = "SPP START FAILED";
    error_until_ms = millis() + 2000;
  } else {
    Serial.printf("Bluetooth SPP ready: %s\n",
                  audio_selector::config::kBluetoothDeviceName);
  }
  drawDemoScreen();
}

void loop() {
  const std::uint32_t now_ms = millis();
  handleConnectionEvent(spp_transport.update());
  std::uint8_t received[128]{};
  const std::size_t received_length =
      spp_transport.read(received, sizeof(received));
  if (received_length != 0) {
    stream_parser.push(received, received_length, handleProtocolFrame,
                       handleProtocolParseError, nullptr);
  }
  const auto event = jog_switch.update(now_ms);
  if (event != audio_selector::input::JogEvent::kNone) {
    Serial.printf("Jog: %s\n", audio_selector::input::eventName(event));
    handleJogEvent(event);
  }

  if (screen_mode == ScreenMode::kMain &&
      connection_status == audio_selector::display::ConnectionStatus::kError &&
      static_cast<std::int32_t>(now_ms - error_until_ms) >= 0) {
    connection_status =
        audio_selector::display::ConnectionStatus::kDisconnected;
    error_message = nullptr;
    drawDemoScreen();
  }

  if (screen_mode == ScreenMode::kMain) {
    display.updateMarquee(makeDemoView(), now_ms);
  }
  delay(10);
}
