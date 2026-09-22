#include "protocol/frame.h"

#include <cstring>

namespace audio_selector::protocol {

bool encodeFrame(const Frame& frame, std::uint8_t* output,
                 const std::size_t outputCapacity, std::size_t& outputLength) {
  outputLength = 0;
  if (frame.payloadLength > config::kMaximumPayloadLength || output == nullptr ||
      outputCapacity < kHeaderLength + frame.payloadLength) {
    return false;
  }
  output[0] = kMagic0;
  output[1] = kMagic1;
  output[2] = frame.version;
  output[3] = frame.messageType;
  output[4] = static_cast<std::uint8_t>(frame.payloadLength & 0xff);
  output[5] = static_cast<std::uint8_t>(frame.payloadLength >> 8);
  std::memcpy(output + kHeaderLength, frame.payload.data(), frame.payloadLength);
  outputLength = kHeaderLength + frame.payloadLength;
  return true;
}

}  // namespace audio_selector::protocol

