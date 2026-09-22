#include "input/jog_switch.h"

#include "pins.h"

namespace audio_selector::input {
namespace {

constexpr std::uint8_t kUpMask = 1U << 0;
constexpr std::uint8_t kDownMask = 1U << 1;
constexpr std::uint8_t kLeftMask = 1U << 2;
constexpr std::uint8_t kRightMask = 1U << 3;
constexpr std::uint8_t kCenterMask = 1U << 4;

bool hasMultipleBits(std::uint8_t value) {
  return value != 0 && (value & (value - 1U)) != 0;
}

JogEvent directionEvent(std::uint8_t mask) {
  switch (mask) {
    case kUpMask:
      return JogEvent::kUp;
    case kDownMask:
      return JogEvent::kDown;
    case kLeftMask:
      return JogEvent::kLeft;
    case kRightMask:
      return JogEvent::kRight;
    default:
      return JogEvent::kNone;
  }
}

}  // namespace

void JogSwitch::begin() {
  pinMode(pins::kJogUp, INPUT_PULLUP);
  pinMode(pins::kJogDown, INPUT_PULLUP);
  pinMode(pins::kJogLeft, INPUT_PULLUP);
  pinMode(pins::kJogRight, INPUT_PULLUP);
  pinMode(pins::kJogCenter, INPUT_PULLUP);

  candidate_mask_ = readPressedMask();
  stable_mask_ = candidate_mask_;
  candidate_since_ms_ = millis();
  chord_locked_ = stable_mask_ != 0;
  center_long_sent_ = false;
}

JogEvent JogSwitch::update(std::uint32_t now_ms) {
  const std::uint8_t raw_mask = readPressedMask();
  if (raw_mask != candidate_mask_) {
    candidate_mask_ = raw_mask;
    candidate_since_ms_ = now_ms;
    return JogEvent::kNone;
  }

  if (candidate_mask_ != stable_mask_ &&
      now_ms - candidate_since_ms_ >= kDebounceMs) {
    const std::uint8_t previous_mask = stable_mask_;
    stable_mask_ = candidate_mask_;

    if (hasMultipleBits(stable_mask_) ||
        (previous_mask != 0 && stable_mask_ != 0)) {
      chord_locked_ = true;
      center_long_sent_ = false;
      return JogEvent::kNone;
    }

    if (chord_locked_) {
      if (stable_mask_ == 0) {
        chord_locked_ = false;
      }
      return JogEvent::kNone;
    }

    if (stable_mask_ == kCenterMask) {
      center_pressed_at_ms_ = now_ms;
      center_long_sent_ = false;
      return JogEvent::kNone;
    }

    if (stable_mask_ == 0) {
      if (previous_mask == kCenterMask && !center_long_sent_) {
        return JogEvent::kCenterShort;
      }
      return JogEvent::kNone;
    }

    return directionEvent(stable_mask_);
  }

  if (!chord_locked_ && stable_mask_ == kCenterMask &&
      !center_long_sent_ && now_ms - center_pressed_at_ms_ >= kLongPressMs) {
    center_long_sent_ = true;
    return JogEvent::kCenterLong;
  }

  return JogEvent::kNone;
}

std::uint8_t JogSwitch::readPressedMask() const {
  std::uint8_t mask = 0;
  if (digitalRead(pins::kJogUp) == LOW) {
    mask |= kUpMask;
  }
  if (digitalRead(pins::kJogDown) == LOW) {
    mask |= kDownMask;
  }
  if (digitalRead(pins::kJogLeft) == LOW) {
    mask |= kLeftMask;
  }
  if (digitalRead(pins::kJogRight) == LOW) {
    mask |= kRightMask;
  }
  if (digitalRead(pins::kJogCenter) == LOW) {
    mask |= kCenterMask;
  }
  return mask;
}

const char* eventName(JogEvent event) {
  switch (event) {
    case JogEvent::kUp:
      return "UP";
    case JogEvent::kDown:
      return "DOWN";
    case JogEvent::kLeft:
      return "LEFT";
    case JogEvent::kRight:
      return "RIGHT";
    case JogEvent::kCenterShort:
      return "CENTER_SHORT";
    case JogEvent::kCenterLong:
      return "CENTER_LONG";
    case JogEvent::kNone:
    default:
      return "NONE";
  }
}

}  // namespace audio_selector::input
