#include "display/display.h"

#include <cstring>

#include "pins.h"

namespace audio_selector::display {
namespace {

constexpr int kCandidateX = 76;
constexpr int kCandidateRightMargin = 16;
constexpr int kCandidateHeight = 22;
constexpr std::uint32_t kMarqueePauseMs = 1000;
constexpr std::uint32_t kMarqueeStepMs = 30;
constexpr std::uint32_t kMarqueeCycleLimit = 3;

}  // namespace

Display::Display() {
  auto busConfig = bus_.config();
  busConfig.spi_host = VSPI_HOST;
  busConfig.spi_mode = 0;
  busConfig.freq_write = 40000000;
  busConfig.freq_read = 0;
  busConfig.spi_3wire = true;
  busConfig.use_lock = true;
  busConfig.dma_channel = 1;
  busConfig.pin_sclk = pins::kLcdSclk;
  busConfig.pin_mosi = pins::kLcdMosi;
  busConfig.pin_miso = -1;
  busConfig.pin_dc = pins::kLcdDc;
  bus_.config(busConfig);
  panel_.setBus(&bus_);

  auto panelConfig = panel_.config();
  panelConfig.pin_cs = pins::kLcdCs;
  panelConfig.pin_rst = pins::kLcdReset;
  panelConfig.pin_busy = -1;
  panelConfig.panel_width = 240;
  panelConfig.panel_height = 320;
  panelConfig.memory_width = 240;
  panelConfig.memory_height = 320;
  panelConfig.offset_x = 0;
  panelConfig.offset_y = 0;
  panelConfig.offset_rotation = 0;
  panelConfig.readable = false;
  panelConfig.invert = true;
  panelConfig.rgb_order = false;
  panelConfig.dlen_16bit = false;
  panelConfig.bus_shared = false;
  panel_.config(panelConfig);
  setPanel(&panel_);
}

bool Display::begin() {
  if (!init()) return false;
  setRotation(1);
  setColorDepth(16);
  setTextWrap(false);
  return true;
}

void Display::showStartupScreen() {
  startWrite();
  fillScreen(TFT_BLACK);
  setTextDatum(textdatum_t::top_left);
  setTextColor(TFT_CYAN, TFT_BLACK);
  setFont(&fonts::Font2);
  drawString("AUDIO SELECTOR", 12, 12);
  drawFastHLine(12, 38, width() - 24, TFT_DARKGREY);

  setTextColor(TFT_WHITE, TFT_BLACK);
  drawString("Starting...", 12, 58);

  setTextColor(TFT_YELLOW, TFT_BLACK);
  setTextDatum(textdatum_t::bottom_right);
  drawString("PC DISCONNECTED", width() - 10, height() - 10);
  endWrite();
}

void Display::showMainScreen(const MainScreenView& view, std::uint32_t now_ms) {
  startWrite();
  fillScreen(TFT_BLACK);
  setFont(&fonts::Font2);
  setTextWrap(false);
  setTextDatum(textdatum_t::top_left);

  setTextColor(TFT_CYAN, TFT_BLACK);
  drawString("AUDIO SELECTOR", 12, 6);
  drawFastHLine(12, 32, width() - 24, TFT_DARKGREY);

  drawEndpointSection("OUTPUT", view.output, view.focus == Focus::kOutput, 40);
  drawEndpointSection("INPUT", view.input, view.focus == Focus::kInput, 122);
  drawConnectionStatus(view.connection, view.error_message);
  marquee_started_ms_ = now_ms;
  marquee_offset_ = 0;
  marquee_finished_ = false;
  drawCandidateText(view, 0, false);
  endWrite();
}

void Display::updateMarquee(const MainScreenView& view, std::uint32_t now_ms) {
  const char* text = view.focus == Focus::kOutput
                         ? view.output.candidate_name
                         : view.input.candidate_name;
  setFont(&fonts::efontJA_16);
  setTextSize(1.25F);
  const int area_width = width() - kCandidateX - kCandidateRightMargin;
  const int scroll_distance = textWidth(text) - area_width;
  setTextSize(1.0F);
  setFont(&fonts::Font2);

  if (scroll_distance <= 0) {
    return;
  }

  const std::uint32_t scroll_ms =
      static_cast<std::uint32_t>(scroll_distance) * kMarqueeStepMs;
  const std::uint32_t cycle_ms = kMarqueePauseMs + scroll_ms + kMarqueePauseMs;
  const std::uint32_t elapsed_ms = now_ms - marquee_started_ms_;
  if (elapsed_ms >= cycle_ms * kMarqueeCycleLimit) {
    if (!marquee_finished_) {
      marquee_finished_ = true;
      startWrite();
      drawCandidateText(view, 0, true);
      endWrite();
    }
    return;
  }

  const std::uint32_t phase = elapsed_ms % cycle_ms;

  int offset = 0;
  if (phase >= kMarqueePauseMs && phase < kMarqueePauseMs + scroll_ms) {
    offset = static_cast<int>((phase - kMarqueePauseMs) / kMarqueeStepMs);
  } else if (phase >= kMarqueePauseMs + scroll_ms) {
    offset = scroll_distance;
  }

  if (offset == marquee_offset_) {
    return;
  }

  marquee_offset_ = offset;
  startWrite();
  drawCandidateText(view, offset, false);
  endWrite();
}

void Display::showSettingsScreen(std::size_t selected_index) {
  constexpr const char* kItems[] = {
      "Reconnect", "Restart Device", "Bluetooth Info", "Firmware Info",
      "Back"};

  startWrite();
  fillScreen(TFT_BLACK);
  setTextSize(1.0F);
  setFont(&fonts::Font2);
  setTextDatum(textdatum_t::top_left);
  setTextColor(TFT_CYAN, TFT_BLACK);
  drawString("SETTINGS", 12, 8);
  drawFastHLine(12, 34, width() - 24, TFT_DARKGREY);

  for (std::size_t index = 0; index < 5; ++index) {
    const int y = 46 + static_cast<int>(index) * 34;
    const bool selected = index == selected_index;
    if (selected) {
      fillRoundRect(10, y - 3, width() - 20, 28, 4, TFT_DARKCYAN);
    }
    setTextColor(selected ? TFT_WHITE : TFT_LIGHTGREY,
                 selected ? TFT_DARKCYAN : TFT_BLACK);
    drawString(selected ? ">" : " ", 18, y);
    drawString(kItems[index], 42, y);
  }
  endWrite();
}

void Display::showRestartConfirmation(bool restart_selected) {
  startWrite();
  fillScreen(TFT_BLACK);
  setTextSize(1.0F);
  setFont(&fonts::Font2);
  setTextDatum(textdatum_t::top_left);
  setTextColor(TFT_YELLOW, TFT_BLACK);
  drawString("Restart Device?", 18, 24);
  drawFastHLine(18, 52, width() - 36, TFT_DARKGREY);

  const char* options[] = {"Cancel", "Restart"};
  for (std::size_t index = 0; index < 2; ++index) {
    const int y = 82 + static_cast<int>(index) * 48;
    const bool selected = restart_selected == (index == 1);
    setTextColor(selected ? TFT_WHITE : TFT_LIGHTGREY, TFT_BLACK);
    drawString(selected ? ">" : " ", 42, y);
    drawString(options[index], 68, y);
  }
  endWrite();
}

void Display::showBluetoothInfo(const char* device_name, bool connected) {
  startWrite();
  fillScreen(TFT_BLACK);
  setTextSize(1.0F);
  setFont(&fonts::Font2);
  setTextDatum(textdatum_t::top_left);
  setTextColor(TFT_CYAN, TFT_BLACK);
  drawString("BLUETOOTH", 12, 8);
  drawFastHLine(12, 34, width() - 24, TFT_DARKGREY);
  setTextColor(TFT_LIGHTGREY, TFT_BLACK);
  drawString("Name", 18, 58);
  drawString("Status", 18, 116);
  setTextColor(TFT_WHITE, TFT_BLACK);
  drawString(device_name, 88, 58);
  setTextColor(connected ? TFT_GREEN : TFT_ORANGE, TFT_BLACK);
  drawString(connected ? "Connected" : "Disconnected", 88, 116);
  setTextColor(TFT_DARKGREY, TFT_BLACK);
  setTextDatum(textdatum_t::bottom_right);
  drawString("LEFT / CENTER: Back", width() - 10, height() - 8);
  endWrite();
}

void Display::showFirmwareInfo(std::uint16_t major, std::uint16_t minor,
                               std::uint16_t patch,
                               std::uint8_t protocol_version,
                               std::uint8_t device_type,
                               const char* build_info) {
  startWrite();
  fillScreen(TFT_BLACK);
  setTextSize(1.0F);
  setFont(&fonts::Font2);
  setTextDatum(textdatum_t::top_left);
  setTextColor(TFT_CYAN, TFT_BLACK);
  drawString("FIRMWARE INFO", 12, 8);
  drawFastHLine(12, 34, width() - 24, TFT_DARKGREY);
  setTextColor(TFT_LIGHTGREY, TFT_BLACK);
  drawString("Firmware", 18, 54);
  drawString("Protocol", 18, 88);
  drawString("Device type", 18, 122);
  drawString("Build", 18, 156);
  setTextColor(TFT_WHITE, TFT_BLACK);
  drawString(String(major) + "." + String(minor) + "." + String(patch), 116,
             54);
  drawString(String(protocol_version), 116, 88);
  drawString(String(device_type), 116, 122);
  drawString(fitText(build_info, width() - 126), 116, 156);
  setTextColor(TFT_DARKGREY, TFT_BLACK);
  setTextDatum(textdatum_t::bottom_right);
  drawString("LEFT / CENTER: Back", width() - 10, height() - 8);
  endWrite();
}

void Display::showRestarting() {
  startWrite();
  fillScreen(TFT_BLACK);
  setTextSize(1.0F);
  setFont(&fonts::Font2);
  setTextDatum(textdatum_t::middle_center);
  setTextColor(TFT_YELLOW, TFT_BLACK);
  drawString("Restarting...", width() / 2, height() / 2);
  endWrite();
}

void Display::drawEndpointSection(const char* title,
                                  const EndpointView& endpoint, bool focused,
                                  int top) {
  const std::uint16_t border_color = focused ? TFT_CYAN : TFT_DARKGREY;
  drawRoundRect(8, top, width() - 16, 78, 5, border_color);

  setTextSize(1.0F);
  setTextColor(focused ? TFT_WHITE : TFT_LIGHTGREY, TFT_BLACK);
  setTextDatum(textdatum_t::top_left);
  drawString(focused ? ">" : " ", 16, top + 5);
  drawString(title, 34, top + 5);
  if (endpoint.roles_split) {
    setTextDatum(textdatum_t::top_right);
    setTextColor(TFT_MAGENTA, TFT_BLACK);
    drawString("ROLES SPLIT", width() - 16, top + 5);
    setTextDatum(textdatum_t::top_left);
  }

  setTextColor(TFT_CYAN, TFT_BLACK);
  drawString("NOW", 16, top + 28);
  setFont(&fonts::efontJA_16);
  setTextSize(1.25F);
  drawString(fitText(endpoint.current_name, width() - 72), 62, top + 27);

  setTextSize(1.0F);
  setFont(&fonts::Font2);
  setTextColor(TFT_YELLOW, TFT_BLACK);
  drawString("SELECT", 16, top + 53);
  setFont(&fonts::efontJA_16);
  setTextSize(1.25F);
  drawString(fitText(endpoint.candidate_name, width() - 86), kCandidateX,
             top + 52);
  setTextSize(1.0F);
  setFont(&fonts::Font2);
}

void Display::drawCandidateText(const MainScreenView& view, int offset,
                                bool truncate) {
  const bool output_focused = view.focus == Focus::kOutput;
  const char* text = output_focused ? view.output.candidate_name
                                    : view.input.candidate_name;
  const int text_y = (output_focused ? 40 : 122) + 52;
  const int area_width = width() - kCandidateX - kCandidateRightMargin;

  setClipRect(kCandidateX, text_y, area_width, kCandidateHeight);
  fillRect(kCandidateX, text_y, area_width, kCandidateHeight, TFT_BLACK);
  setTextDatum(textdatum_t::top_left);
  setTextColor(TFT_YELLOW, TFT_BLACK);
  setFont(&fonts::efontJA_16);
  setTextSize(1.25F);
  if (truncate) {
    drawString(fitText(text, area_width), kCandidateX, text_y);
  } else {
    drawString(text, kCandidateX - offset, text_y);
  }
  setTextSize(1.0F);
  setFont(&fonts::Font2);
  clearClipRect();
}

void Display::drawConnectionStatus(ConnectionStatus status,
                                   const char* error_message) {
  setTextDatum(textdatum_t::bottom_right);
  switch (status) {
    case ConnectionStatus::kConnected:
      setTextColor(TFT_GREEN, TFT_BLACK);
      drawString("CONNECTED", width() - 10, height() - 8);
      break;
    case ConnectionStatus::kError:
      setTextColor(TFT_RED, TFT_BLACK);
      drawString(error_message == nullptr ? "ERROR" : error_message,
                 width() - 10, height() - 8);
      break;
    case ConnectionStatus::kDisconnected:
    default:
      setTextColor(TFT_ORANGE, TFT_BLACK);
      drawString("PC DISCONNECTED", width() - 10, height() - 8);
      break;
  }
}

String Display::fitText(const char* text, int max_width) {
  const String source = text == nullptr ? "" : text;
  if (textWidth(source) <= max_width) {
    return source;
  }

  constexpr const char* kEllipsis = "...";
  const std::size_t byte_length = source.length();
  std::size_t byte_index = 0;
  std::size_t fitted_length = 0;

  while (byte_index < byte_length) {
    const auto first = static_cast<std::uint8_t>(source[byte_index]);
    std::size_t code_point_bytes = 1;
    if ((first & 0xF8U) == 0xF0U) {
      code_point_bytes = 4;
    } else if ((first & 0xF0U) == 0xE0U) {
      code_point_bytes = 3;
    } else if ((first & 0xE0U) == 0xC0U) {
      code_point_bytes = 2;
    }

    const std::size_t next = byte_index + code_point_bytes;
    if (next > byte_length) {
      break;
    }

    String trial = source.substring(0, next);
    trial += kEllipsis;
    if (textWidth(trial) > max_width) {
      break;
    }
    fitted_length = next;
    byte_index = next;
  }

  String fitted = source.substring(0, fitted_length);
  fitted += kEllipsis;
  return fitted;
}

}  // namespace audio_selector::display
