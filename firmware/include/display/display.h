#pragma once

#include <LovyanGFX.hpp>

#include <cstddef>
#include <cstdint>

namespace audio_selector::display {

enum class Focus {
  kOutput,
  kInput,
};

enum class ConnectionStatus {
  kDisconnected,
  kConnected,
  kError,
};

struct EndpointView {
  const char* current_name;
  const char* candidate_name;
  bool roles_split;
};

struct MainScreenView {
  Focus focus;
  EndpointView output;
  EndpointView input;
  ConnectionStatus connection;
  const char* error_message;
};

class Display final : public lgfx::LGFX_Device {
 public:
  Display();
  bool begin();
  void showStartupScreen();
  void showMainScreen(const MainScreenView& view, std::uint32_t now_ms);
  void updateMarquee(const MainScreenView& view, std::uint32_t now_ms);
  void showSettingsScreen(std::size_t selected_index);
  void showRestartConfirmation(bool restart_selected);
  void showBluetoothInfo(const char* device_name, bool connected);
  void showFirmwareInfo(std::uint16_t major, std::uint16_t minor,
                        std::uint16_t patch, std::uint8_t protocol_version,
                        std::uint8_t device_type, const char* build_info);
  void showRestarting();

 private:
  void drawEndpointSection(const char* title, const EndpointView& endpoint,
                           bool focused, int top);
  void drawConnectionStatus(ConnectionStatus status,
                            const char* error_message);
  void drawCandidateText(const MainScreenView& view, int offset,
                         bool truncate);
  String fitText(const char* text, int max_width);

  lgfx::Panel_ST7789 panel_;
  lgfx::Bus_SPI bus_;
  std::uint32_t marquee_started_ms_ = 0;
  int marquee_offset_ = -1;
  bool marquee_finished_ = false;
};

}  // namespace audio_selector::display
