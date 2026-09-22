#pragma once

#include <cstddef>
#include <cstdint>

namespace audio_selector::config {

inline constexpr std::uint8_t kProtocolVersion = 0x01;
inline constexpr std::uint8_t kDeviceType = 0x01;

inline constexpr std::uint16_t kFirmwareVersionMajor = 0;
inline constexpr std::uint16_t kFirmwareVersionMinor = 1;
inline constexpr std::uint16_t kFirmwareVersionPatch = 0;

inline constexpr char kBluetoothDeviceName[] = "AudioSelector";

inline constexpr std::size_t kMaximumPayloadLength = 512;
inline constexpr std::size_t kReceiveBufferLength = 2048;
inline constexpr std::size_t kMaximumEndpointsPerFlow = 64;
inline constexpr std::size_t kMaximumEndpointNameLength = 240;

}  // namespace audio_selector::config

