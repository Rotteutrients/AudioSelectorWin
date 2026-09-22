#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

#include "config.h"
#include "protocol/frame.h"

namespace audio_selector::protocol {

class StreamParser {
 public:
  void clear();
  std::size_t bufferedLength() const;
  void push(const std::uint8_t* bytes, std::size_t length,
            FrameCallback onFrame, ErrorCallback onError, void* context);

 private:
  void discardBeforeMagic();
  void consume(std::size_t length);

  std::array<std::uint8_t, config::kReceiveBufferLength> buffer_{};
  std::size_t length_{};
};

}  // namespace audio_selector::protocol

