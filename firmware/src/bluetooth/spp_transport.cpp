#include "bluetooth/spp_transport.h"

#include <cstring>

namespace audio_selector::bluetooth {

bool SppTransport::begin(const char* device_name) {
  if (device_name == nullptr || device_name[0] == '\0') {
    return false;
  }
  end();
  std::strncpy(device_name_.data(), device_name, device_name_.size() - 1);
  device_name_.back() = '\0';
  started_ = serial_.begin(device_name_.data());
  connected_ = false;
  return started_;
}

void SppTransport::end() {
  if (started_) {
    serial_.disconnect();
    serial_.end();
  }
  started_ = false;
  connected_ = false;
  clearQueues();
}

bool SppTransport::reconnect() {
  if (device_name_[0] == '\0') {
    return false;
  }
  std::array<char, 64> name = device_name_;
  end();
  return begin(name.data());
}

ConnectionEvent SppTransport::update() {
  const bool now_connected = started_ && serial_.hasClient();
  ConnectionEvent event = ConnectionEvent::kNone;
  if (now_connected != connected_) {
    connected_ = now_connected;
    event = connected_ ? ConnectionEvent::kConnected
                       : ConnectionEvent::kDisconnected;
    if (!connected_) {
      clearQueues();
    }
  }
  if (connected_) {
    flushWrites();
  }
  return event;
}

std::size_t SppTransport::read(std::uint8_t* output,
                               const std::size_t capacity) {
  if (!connected_ || output == nullptr || capacity == 0) {
    return 0;
  }
  std::size_t length = 0;
  while (length < capacity && serial_.available() > 0) {
    const int value = serial_.read();
    if (value < 0) {
      break;
    }
    output[length++] = static_cast<std::uint8_t>(value);
  }
  return length;
}

bool SppTransport::enqueue(const protocol::Frame& frame) {
  if (!connected_ || write_count_ >= writes_.size()) {
    return false;
  }
  PendingWrite& pending = writes_[write_tail_];
  pending = {};
  if (!protocol::encodeFrame(frame, pending.bytes.data(), pending.bytes.size(),
                             pending.length)) {
    return false;
  }
  write_tail_ = (write_tail_ + 1) % writes_.size();
  ++write_count_;
  return true;
}

void SppTransport::clearQueues() {
  for (PendingWrite& pending : writes_) {
    pending = {};
  }
  write_head_ = 0;
  write_tail_ = 0;
  write_count_ = 0;
}

void SppTransport::flushWrites() {
  while (write_count_ != 0) {
    PendingWrite& pending = writes_[write_head_];
    const std::size_t remaining = pending.length - pending.offset;
    const std::size_t written =
        serial_.write(pending.bytes.data() + pending.offset, remaining);
    if (written == 0) {
      return;
    }
    pending.offset += written;
    if (pending.offset != pending.length) {
      return;
    }
    pending = {};
    write_head_ = (write_head_ + 1) % writes_.size();
    --write_count_;
  }
}

}  // namespace audio_selector::bluetooth
