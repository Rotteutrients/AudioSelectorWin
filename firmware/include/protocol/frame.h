#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

#include "config.h"

namespace audio_selector::protocol {

inline constexpr std::uint8_t kMagic0 = 0x41;
inline constexpr std::uint8_t kMagic1 = 0x53;
inline constexpr std::size_t kHeaderLength = 6;

struct Frame {
  std::uint8_t version{};
  std::uint8_t messageType{};
  std::uint16_t payloadLength{};
  std::array<std::uint8_t, config::kMaximumPayloadLength> payload{};
};

enum class ParseError {
  kInvalidPayloadLength,
  kReceiveBufferOverflow,
};

using FrameCallback = void (*)(const Frame&, void* context);
using ErrorCallback = void (*)(ParseError, void* context);

bool encodeFrame(const Frame& frame, std::uint8_t* output,
                 std::size_t outputCapacity, std::size_t& outputLength);

}  // namespace audio_selector::protocol

