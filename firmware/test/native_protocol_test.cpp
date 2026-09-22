#include <algorithm>
#include <cassert>
#include <cstdint>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#include "protocol/frame.h"
#include "protocol/message.h"
#include "protocol/parser.h"

using audio_selector::protocol::Frame;
using audio_selector::protocol::ParseError;
using audio_selector::protocol::StreamParser;
using audio_selector::protocol::Message;
using audio_selector::protocol::MessageType;

struct Results { std::vector<Frame> frames; std::vector<ParseError> errors; };
void onFrame(const Frame& frame, void* context) { static_cast<Results*>(context)->frames.push_back(frame); }
void onError(ParseError error, void* context) { static_cast<Results*>(context)->errors.push_back(error); }

std::vector<std::uint8_t> parseHex(const std::string& text) {
  std::istringstream stream(text); std::vector<std::uint8_t> bytes; std::string hex;
  while (stream >> hex) bytes.push_back(static_cast<std::uint8_t>(std::stoul(hex, nullptr, 16)));
  return bytes;
}

std::string codecErrorName(audio_selector::protocol::CodecError error) {
  using E = audio_selector::protocol::CodecError;
  switch (error) {
    case E::kUnsupportedVersion: return "unsupported_version";
    case E::kUnknownMessageType: return "unknown_type";
    case E::kInvalidLength: return "invalid_length";
    case E::kInvalidValue: return "invalid_value";
    case E::kReservedNotZero: return "reserved_not_zero";
    case E::kInvalidUtf8: return "invalid_utf8";
    default: return "unexpected";
  }
}

int main() {
  const std::uint8_t ping[] = {0x41,0x53,0x01,0x30,0x04,0x00,0x78,0x56,0x34,0x12};
  StreamParser parser;
  Results results;
  parser.push(ping, 5, onFrame, onError, &results);
  assert(results.frames.empty());
  parser.push(ping + 5, sizeof(ping) - 5, onFrame, onError, &results);
  assert(results.frames.size() == 1 && results.frames[0].messageType == 0x30);

  std::vector<std::uint8_t> joined(ping, ping + sizeof(ping));
  joined.insert(joined.end(), ping, ping + sizeof(ping));
  results = {};
  parser.push(joined.data(), joined.size(), onFrame, onError, &results);
  assert(results.frames.size() == 2);

  const std::uint8_t bad[] = {0xff,0x41,0x00,0x41,0x53,0x01,0x30,0x01,0x02};
  std::vector<std::uint8_t> recovery(bad, bad + sizeof(bad));
  recovery.insert(recovery.end(), ping, ping + sizeof(ping));
  results = {};
  parser.push(recovery.data(), recovery.size(), onFrame, onError, &results);
  assert(results.errors.size() == 1 && results.frames.size() == 1);

  std::uint8_t encoded[16]{};
  std::size_t encodedLength = 0;
  assert(audio_selector::protocol::encodeFrame(results.frames[0], encoded, sizeof(encoded), encodedLength));
  assert(encodedLength == sizeof(ping));
  for (std::size_t i = 0; i < sizeof(ping); ++i) assert(encoded[i] == ping[i]);

  std::vector<Message> messages(15);
  int n = 0;
  messages[n].type=MessageType::kHelloRequest;messages[n].minimumVersion=1;messages[n].maximumVersion=1;messages[n++].hostNonce=42;
  messages[n].type=MessageType::kHelloResponse;messages[n].selectedVersion=1;messages[n].deviceType=1;messages[n].firmwareMinor=1;messages[n].echoedHostNonce=42;messages[n++].bootId=99;
  messages[n].type=MessageType::kSyncRequest;messages[n++].reason=0;
  messages[n].type=MessageType::kSyncBegin;messages[n].generation=1;messages[n].outputCount=1;messages[n++].inputCount=1;
  messages[n].type=MessageType::kOutputEndpoint;messages[n].generation=1;messages[n].handle=1;messages[n].textLength=7;std::copy_n(reinterpret_cast<const std::uint8_t*>("USB DAC"),7,messages[n++].text.data());
  messages[n].type=MessageType::kInputEndpoint;messages[n].generation=1;messages[n].handle=1;messages[n].textLength=3;std::copy_n(reinterpret_cast<const std::uint8_t*>("Mic"),3,messages[n++].text.data());
  for(auto type:{MessageType::kCurrentOutput,MessageType::kCurrentInput}){messages[n].type=type;messages[n].generation=1;messages[n].consoleHandle=1;messages[n].multimediaHandle=1;messages[n++].communicationsHandle=1;}
  messages[n].type=MessageType::kSyncEnd;messages[n++].generation=1;
  for(auto type:{MessageType::kSetOutputRequest,MessageType::kSetInputRequest}){messages[n].type=type;messages[n].requestId=7;messages[n].generation=1;messages[n++].handle=1;}
  messages[n].type=MessageType::kSetResult;messages[n].requestId=7;messages[n].knownGeneration=1;messages[n++].status=0;
  messages[n].type=MessageType::kPing;messages[n++].token=42;
  messages[n].type=MessageType::kPong;messages[n++].token=42;
  messages[n].type=MessageType::kError;messages[n].errorCode=2;messages[n].textLength=7;std::copy_n(reinterpret_cast<const std::uint8_t*>("unknown"),7,messages[n++].text.data());
  assert(n==15);
  for(const auto& source:messages){Frame wire{};assert(audio_selector::protocol::encodeMessage(source,wire)==audio_selector::protocol::CodecError::kNone);Message decoded{};assert(audio_selector::protocol::decodeMessage(wire,decoded)==audio_selector::protocol::CodecError::kNone);assert(decoded.type==source.type);}

  std::ifstream vectors("../protocol/test-vectors.txt");
  assert(vectors.good());
  std::string line;
  while(std::getline(vectors,line)){
    if(line.empty()||line[0]=='#')continue;
    const auto split=line.find('|');assert(split!=std::string::npos);
    const auto bytes=parseHex(line.substr(split+1));
    Results vectorResults{};StreamParser vectorParser;vectorParser.push(bytes.data(),bytes.size(),onFrame,onError,&vectorResults);
    assert(vectorResults.errors.empty()&&vectorResults.frames.size()==1);
    Message decoded{};assert(audio_selector::protocol::decodeMessage(vectorResults.frames[0],decoded)==audio_selector::protocol::CodecError::kNone);
    std::uint8_t roundTrip[518]{};std::size_t roundTripLength=0;assert(audio_selector::protocol::encodeFrame(vectorResults.frames[0],roundTrip,sizeof(roundTrip),roundTripLength));assert(roundTripLength==bytes.size());
    for(std::size_t i=0;i<bytes.size();++i)assert(roundTrip[i]==bytes[i]);
  }

  std::ifstream streamVectors("../protocol/stream-error-vectors.txt");assert(streamVectors.good());
  while(std::getline(streamVectors,line)){
    if(line.empty()||line[0]=='#') continue;
    std::vector<std::string> fields;std::size_t start=0;
    while(true){const auto split=line.find('|',start);fields.push_back(line.substr(start,split-start));if(split==std::string::npos)break;start=split+1;}assert(fields.size()==4);
    StreamParser vectorParser;Results vectorResults{};start=0;
    while(true){const auto split=fields[3].find('/',start);const auto bytes=parseHex(fields[3].substr(start,split-start));vectorParser.push(bytes.data(),bytes.size(),onFrame,onError,&vectorResults);if(split==std::string::npos)break;start=split+1;}
    assert(vectorResults.frames.size()==std::stoul(fields[1]));assert(vectorResults.errors.size()==std::stoul(fields[2]));
  }

  std::ifstream messageErrors("../protocol/message-error-vectors.txt");assert(messageErrors.good());
  while(std::getline(messageErrors,line)){
    if(line.empty()||line[0]=='#') continue;
    const auto first=line.find('|');const auto second=line.find('|',first+1);assert(first!=std::string::npos&&second!=std::string::npos);
    const auto bytes=parseHex(line.substr(second+1));StreamParser vectorParser;Results vectorResults{};vectorParser.push(bytes.data(),bytes.size(),onFrame,onError,&vectorResults);assert(vectorResults.frames.size()==1);
    Message decoded{};const auto error=audio_selector::protocol::decodeMessage(vectorResults.frames[0],decoded);assert(codecErrorName(error)==line.substr(first+1,second-first-1));
  }
}
