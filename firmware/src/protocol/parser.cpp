#include "protocol/parser.h"

#include <algorithm>
#include <cstring>

namespace audio_selector::protocol {

void StreamParser::clear() { length_ = 0; }

std::size_t StreamParser::bufferedLength() const { return length_; }

void StreamParser::push(const std::uint8_t* bytes, const std::size_t length,
                        const FrameCallback onFrame, const ErrorCallback onError,
                        void* context) {
  if (bytes == nullptr && length != 0) return;
  if (length_ + length > buffer_.size()) {
    clear();
    if (onError != nullptr) onError(ParseError::kReceiveBufferOverflow, context);
    return;
  }
  std::memcpy(buffer_.data() + length_, bytes, length);
  length_ += length;

  while (true) {
    discardBeforeMagic();
    if (length_ < kHeaderLength) return;
    const auto payloadLength = static_cast<std::uint16_t>(
        static_cast<std::uint16_t>(buffer_[4]) |
        (static_cast<std::uint16_t>(buffer_[5]) << 8));
    if (payloadLength > config::kMaximumPayloadLength) {
      if (onError != nullptr) onError(ParseError::kInvalidPayloadLength, context);
      consume(1);
      continue;
    }
    const std::size_t frameLength = kHeaderLength + payloadLength;
    if (length_ < frameLength) return;
    Frame frame{};
    frame.version = buffer_[2];
    frame.messageType = buffer_[3];
    frame.payloadLength = payloadLength;
    std::copy_n(buffer_.data() + kHeaderLength, payloadLength, frame.payload.data());
    consume(frameLength);
    if (onFrame != nullptr) onFrame(frame, context);
  }
}

void StreamParser::discardBeforeMagic() {
  if (length_ >= 2 && buffer_[0] == kMagic0 && buffer_[1] == kMagic1) return;
  for (std::size_t position = 1; position + 1 < length_; ++position) {
    if (buffer_[position] == kMagic0 && buffer_[position + 1] == kMagic1) {
      consume(position);
      return;
    }
  }
  const bool keepPrefix = length_ != 0 && buffer_[length_ - 1] == kMagic0;
  length_ = keepPrefix ? 1 : 0;
  if (keepPrefix) buffer_[0] = kMagic0;
}

void StreamParser::consume(const std::size_t length) {
  if (length >= length_) {
    length_ = 0;
    return;
  }
  std::memmove(buffer_.data(), buffer_.data() + length, length_ - length);
  length_ -= length;
}

}  // namespace audio_selector::protocol
