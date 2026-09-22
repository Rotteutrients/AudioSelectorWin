#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

#include "config.h"
#include "protocol/frame.h"

namespace audio_selector::protocol {

enum class MessageType : std::uint8_t {
  kHelloRequest = 0x01, kHelloResponse = 0x02,
  kSyncRequest = 0x10, kSyncBegin = 0x11,
  kOutputEndpoint = 0x12, kInputEndpoint = 0x13,
  kCurrentOutput = 0x14, kCurrentInput = 0x15, kSyncEnd = 0x16,
  kSetOutputRequest = 0x20, kSetInputRequest = 0x21, kSetResult = 0x22,
  kPing = 0x30, kPong = 0x31, kError = 0x7f,
};

struct Message {
  MessageType type{};
  std::uint8_t minimumVersion{}, maximumVersion{}, selectedVersion{}, deviceType{};
  std::uint8_t reason{}, operation{}, status{}, relatedMessageType{};
  std::uint16_t firmwareMajor{}, firmwareMinor{}, firmwarePatch{};
  std::uint16_t outputCount{}, inputCount{}, handle{};
  std::uint16_t consoleHandle{}, multimediaHandle{}, communicationsHandle{};
  std::uint16_t errorCode{}, textLength{};
  std::uint32_t capabilities{}, hostNonce{}, echoedHostNonce{}, bootId{};
  std::uint32_t generation{}, requestId{}, knownGeneration{}, token{}, contextId{};
  std::array<std::uint8_t, config::kMaximumEndpointNameLength> text{};
};

enum class CodecError {
  kNone, kUnsupportedVersion, kUnknownMessageType, kInvalidLength,
  kInvalidValue, kReservedNotZero, kInvalidUtf8, kStringTooLong,
};

CodecError decodeMessage(const Frame& frame, Message& message);
CodecError encodeMessage(const Message& message, Frame& frame);

}  // namespace audio_selector::protocol

