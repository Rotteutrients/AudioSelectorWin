#pragma once

#include <BluetoothSerial.h>

#include <array>
#include <cstddef>
#include <cstdint>

#include "protocol/frame.h"

namespace audio_selector::bluetooth {

enum class ConnectionEvent : std::uint8_t {
  kNone,
  kConnected,
  kDisconnected,
};

class SppTransport final {
 public:
  bool begin(const char* device_name);
  void end();
  bool reconnect();

  ConnectionEvent update();
  bool connected() const { return connected_; }
  std::size_t read(std::uint8_t* output, std::size_t capacity);
  bool enqueue(const protocol::Frame& frame);
  void clearQueues();

 private:
  static constexpr std::size_t kMaximumFrameLength =
      protocol::kHeaderLength + config::kMaximumPayloadLength;
  static constexpr std::size_t kSendQueueLength = 4;

  struct PendingWrite {
    std::array<std::uint8_t, kMaximumFrameLength> bytes{};
    std::size_t length = 0;
    std::size_t offset = 0;
  };

  void flushWrites();

  BluetoothSerial serial_{};
  std::array<PendingWrite, kSendQueueLength> writes_{};
  std::array<char, 64> device_name_{};
  std::size_t write_head_ = 0;
  std::size_t write_tail_ = 0;
  std::size_t write_count_ = 0;
  bool started_ = false;
  bool connected_ = false;
};

}  // namespace audio_selector::bluetooth
