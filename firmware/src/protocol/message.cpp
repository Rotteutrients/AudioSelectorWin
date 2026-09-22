#include "protocol/message.h"

#include <algorithm>

namespace audio_selector::protocol {
namespace {

void put16(Frame& f, std::size_t o, std::uint16_t v) { f.payload[o]=v&0xff; f.payload[o+1]=v>>8; }
void put32(Frame& f, std::size_t o, std::uint32_t v) { for(int i=0;i<4;++i) f.payload[o+i]=(v>>(8*i))&0xff; }
std::uint16_t get16(const Frame& f,std::size_t o){return f.payload[o]|(std::uint16_t(f.payload[o+1])<<8);}
std::uint32_t get32(const Frame& f,std::size_t o){return f.payload[o]|(std::uint32_t(f.payload[o+1])<<8)|(std::uint32_t(f.payload[o+2])<<16)|(std::uint32_t(f.payload[o+3])<<24);}
bool exact(const Frame& f,std::size_t n){return f.payloadLength==n;}
bool knownType(std::uint8_t v){switch(static_cast<MessageType>(v)){case MessageType::kHelloRequest:case MessageType::kHelloResponse:case MessageType::kSyncRequest:case MessageType::kSyncBegin:case MessageType::kOutputEndpoint:case MessageType::kInputEndpoint:case MessageType::kCurrentOutput:case MessageType::kCurrentInput:case MessageType::kSyncEnd:case MessageType::kSetOutputRequest:case MessageType::kSetInputRequest:case MessageType::kSetResult:case MessageType::kPing:case MessageType::kPong:case MessageType::kError:return true;}return false;}
bool validUtf8(const std::uint8_t* s,std::size_t n){std::size_t i=0;while(i<n){std::uint8_t c=s[i++];if(c<0x80)continue;int need=0;std::uint32_t cp=0,min=0;if((c&0xe0)==0xc0){need=1;cp=c&0x1f;min=0x80;}else if((c&0xf0)==0xe0){need=2;cp=c&0x0f;min=0x800;}else if((c&0xf8)==0xf0){need=3;cp=c&7;min=0x10000;}else return false;if(i+need>n)return false;while(need--){c=s[i++];if((c&0xc0)!=0x80)return false;cp=(cp<<6)|(c&0x3f);}if(cp<min||cp>0x10ffff||(cp>=0xd800&&cp<=0xdfff))return false;}return true;}
CodecError decodeEndpoint(const Frame& f,Message& m){if(f.payloadLength<8)return CodecError::kInvalidLength;m.generation=get32(f,0);m.handle=get16(f,4);m.textLength=get16(f,6);if(m.generation==0||m.handle==0)return CodecError::kInvalidValue;if(m.textLength==0||m.textLength>m.text.size())return CodecError::kStringTooLong;if(f.payloadLength!=8+m.textLength)return CodecError::kInvalidLength;if(!validUtf8(f.payload.data()+8,m.textLength))return CodecError::kInvalidUtf8;std::copy_n(f.payload.data()+8,m.textLength,m.text.data());return CodecError::kNone;}
CodecError encodeEndpoint(const Message& m,Frame& f){if(m.generation==0||m.handle==0||m.textLength==0)return CodecError::kInvalidValue;if(m.textLength>m.text.size())return CodecError::kStringTooLong;if(!validUtf8(m.text.data(),m.textLength))return CodecError::kInvalidUtf8;f.payloadLength=8+m.textLength;put32(f,0,m.generation);put16(f,4,m.handle);put16(f,6,m.textLength);std::copy_n(m.text.data(),m.textLength,f.payload.data()+8);return CodecError::kNone;}
}

CodecError decodeMessage(const Frame& f, Message& m) {
  m={}; if(f.version!=config::kProtocolVersion)return CodecError::kUnsupportedVersion;if(!knownType(f.messageType))return CodecError::kUnknownMessageType;m.type=static_cast<MessageType>(f.messageType);
  switch(m.type){
    case MessageType::kHelloRequest:if(!exact(f,12))return CodecError::kInvalidLength;if(f.payload[10]||f.payload[11])return CodecError::kReservedNotZero;m.minimumVersion=f.payload[0];m.maximumVersion=f.payload[1];m.capabilities=get32(f,2);m.hostNonce=get32(f,6);break;
    case MessageType::kHelloResponse:if(!exact(f,20))return CodecError::kInvalidLength;m.selectedVersion=f.payload[0];m.deviceType=f.payload[1];m.firmwareMajor=get16(f,2);m.firmwareMinor=get16(f,4);m.firmwarePatch=get16(f,6);m.capabilities=get32(f,8);m.echoedHostNonce=get32(f,12);m.bootId=get32(f,16);break;
    case MessageType::kSyncRequest:if(!exact(f,1))return CodecError::kInvalidLength;m.reason=f.payload[0];if(m.reason>2)return CodecError::kInvalidValue;break;
    case MessageType::kSyncBegin:if(!exact(f,8))return CodecError::kInvalidLength;m.generation=get32(f,0);m.outputCount=get16(f,4);m.inputCount=get16(f,6);if(!m.generation||m.outputCount>64||m.inputCount>64)return CodecError::kInvalidValue;break;
    case MessageType::kOutputEndpoint:case MessageType::kInputEndpoint:return decodeEndpoint(f,m);
    case MessageType::kCurrentOutput:case MessageType::kCurrentInput:if(!exact(f,10))return CodecError::kInvalidLength;m.generation=get32(f,0);m.consoleHandle=get16(f,4);m.multimediaHandle=get16(f,6);m.communicationsHandle=get16(f,8);if(!m.generation)return CodecError::kInvalidValue;break;
    case MessageType::kSyncEnd:if(!exact(f,4))return CodecError::kInvalidLength;m.generation=get32(f,0);if(!m.generation)return CodecError::kInvalidValue;break;
    case MessageType::kSetOutputRequest:case MessageType::kSetInputRequest:if(!exact(f,10))return CodecError::kInvalidLength;m.requestId=get32(f,0);m.generation=get32(f,4);m.handle=get16(f,8);if(!m.requestId||!m.generation||!m.handle)return CodecError::kInvalidValue;break;
    case MessageType::kSetResult:if(!exact(f,12))return CodecError::kInvalidLength;m.requestId=get32(f,0);m.operation=f.payload[4];m.status=f.payload[5];m.errorCode=get16(f,6);m.knownGeneration=get32(f,8);if(!m.requestId||m.operation>1||m.status>4||!m.knownGeneration||((m.status==0)!=(m.errorCode==0)))return CodecError::kInvalidValue;break;
    case MessageType::kPing:case MessageType::kPong:if(!exact(f,4))return CodecError::kInvalidLength;m.token=get32(f,0);break;
    case MessageType::kError:if(f.payloadLength<10)return CodecError::kInvalidLength;m.errorCode=get16(f,0);m.relatedMessageType=f.payload[2];if(f.payload[3])return CodecError::kReservedNotZero;m.contextId=get32(f,4);m.textLength=get16(f,8);if(!m.errorCode||m.errorCode>0x10||m.textLength>240)return CodecError::kInvalidValue;if(f.payloadLength!=10+m.textLength)return CodecError::kInvalidLength;if(!validUtf8(f.payload.data()+10,m.textLength))return CodecError::kInvalidUtf8;std::copy_n(f.payload.data()+10,m.textLength,m.text.data());break;
  } return CodecError::kNone;
}

CodecError encodeMessage(const Message& m, Frame& f) {
  f={};f.version=config::kProtocolVersion;f.messageType=static_cast<std::uint8_t>(m.type);
  switch(m.type){
    case MessageType::kHelloRequest:f.payloadLength=12;f.payload[0]=m.minimumVersion;f.payload[1]=m.maximumVersion;put32(f,2,m.capabilities);put32(f,6,m.hostNonce);put16(f,10,0);break;
    case MessageType::kHelloResponse:f.payloadLength=20;f.payload[0]=m.selectedVersion;f.payload[1]=m.deviceType;put16(f,2,m.firmwareMajor);put16(f,4,m.firmwareMinor);put16(f,6,m.firmwarePatch);put32(f,8,m.capabilities);put32(f,12,m.echoedHostNonce);put32(f,16,m.bootId);break;
    case MessageType::kSyncRequest:if(m.reason>2)return CodecError::kInvalidValue;f.payloadLength=1;f.payload[0]=m.reason;break;
    case MessageType::kSyncBegin:if(!m.generation||m.outputCount>64||m.inputCount>64)return CodecError::kInvalidValue;f.payloadLength=8;put32(f,0,m.generation);put16(f,4,m.outputCount);put16(f,6,m.inputCount);break;
    case MessageType::kOutputEndpoint:case MessageType::kInputEndpoint:return encodeEndpoint(m,f);
    case MessageType::kCurrentOutput:case MessageType::kCurrentInput:if(!m.generation)return CodecError::kInvalidValue;f.payloadLength=10;put32(f,0,m.generation);put16(f,4,m.consoleHandle);put16(f,6,m.multimediaHandle);put16(f,8,m.communicationsHandle);break;
    case MessageType::kSyncEnd:if(!m.generation)return CodecError::kInvalidValue;f.payloadLength=4;put32(f,0,m.generation);break;
    case MessageType::kSetOutputRequest:case MessageType::kSetInputRequest:if(!m.requestId||!m.generation||!m.handle)return CodecError::kInvalidValue;f.payloadLength=10;put32(f,0,m.requestId);put32(f,4,m.generation);put16(f,8,m.handle);break;
    case MessageType::kSetResult:if(!m.requestId||m.operation>1||m.status>4||!m.knownGeneration||((m.status==0)!=(m.errorCode==0)))return CodecError::kInvalidValue;f.payloadLength=12;put32(f,0,m.requestId);f.payload[4]=m.operation;f.payload[5]=m.status;put16(f,6,m.errorCode);put32(f,8,m.knownGeneration);break;
    case MessageType::kPing:case MessageType::kPong:f.payloadLength=4;put32(f,0,m.token);break;
    case MessageType::kError:if(!m.errorCode||m.errorCode>0x10||m.textLength>240)return CodecError::kInvalidValue;if(!validUtf8(m.text.data(),m.textLength))return CodecError::kInvalidUtf8;f.payloadLength=10+m.textLength;put16(f,0,m.errorCode);f.payload[2]=m.relatedMessageType;f.payload[3]=0;put32(f,4,m.contextId);put16(f,8,m.textLength);std::copy_n(m.text.data(),m.textLength,f.payload.data()+10);break;
  } return CodecError::kNone;
}
}  // namespace audio_selector::protocol
