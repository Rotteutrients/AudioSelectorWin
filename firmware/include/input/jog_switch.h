#pragma once

#include <Arduino.h>

#include <cstdint>

namespace audio_selector::input {

enum class JogEvent : std::uint8_t {
  kNone,
  kUp,
  kDown,
  kLeft,
  kRight,
  kCenterShort,
  kCenterLong,
};

class JogSwitch final {
 public:
  void begin();
  JogEvent update(std::uint32_t now_ms);

 private:
  static constexpr std::uint32_t kDebounceMs = 25;
  static constexpr std::uint32_t kLongPressMs = 800;

  std::uint8_t readPressedMask() const;

  std::uint8_t candidate_mask_ = 0;
  std::uint8_t stable_mask_ = 0;
  std::uint32_t candidate_since_ms_ = 0;
  std::uint32_t center_pressed_at_ms_ = 0;
  bool chord_locked_ = false;
  bool center_long_sent_ = false;
};

const char* eventName(JogEvent event);

}  // namespace audio_selector::input
