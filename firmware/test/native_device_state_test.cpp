#include <cassert>
#include <cstdio>
#include <cstring>

#include "state/device_state.h"

using audio_selector::state::CurrentHandles;
using audio_selector::state::DataFlow;
using audio_selector::state::DeviceState;
using audio_selector::state::SetRequest;
using audio_selector::state::StateError;

namespace {

void add(DeviceState& state, DataFlow flow, std::uint16_t handle,
         const char* name) {
  assert(state.addEndpoint(flow, handle, name, std::strlen(name)));
}

}  // namespace

int main() {
  DeviceState state;

  assert(state.beginSync(10, 3, 1));
  add(state, DataFlow::kOutput, 1, "Speakers");
  add(state, DataFlow::kOutput, 2, "USB DAC");
  add(state, DataFlow::kOutput, 3, "HDMI");
  add(state, DataFlow::kInput, 8, "Microphone");
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{1, 2, 1}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{8, 8, 8}));
  assert(state.commitSync(10));
  assert(state.ready());
  assert(state.generation() == 10);
  assert(state.currentHandle(DataFlow::kOutput) == 2);
  assert(state.rolesSplit(DataFlow::kOutput));
  assert(!state.rolesSplit(DataFlow::kInput));
  assert(state.candidateHandle(DataFlow::kOutput) == 2);
  assert(std::strcmp(state.currentName(DataFlow::kOutput), "USB DAC") == 0);

  assert(state.cycleCandidate(DataFlow::kOutput, 1));
  assert(state.candidateHandle(DataFlow::kOutput) == 3);
  assert(state.cycleCandidate(DataFlow::kOutput, 1));
  assert(state.candidateHandle(DataFlow::kOutput) == 1);
  assert(state.cycleCandidate(DataFlow::kOutput, -1));
  assert(state.candidateHandle(DataFlow::kOutput) == 3);

  assert(state.beginSync(11, 3, 0));
  add(state, DataFlow::kOutput, 3, "HDMI renamed");
  add(state, DataFlow::kOutput, 2, "USB DAC");
  add(state, DataFlow::kOutput, 1, "Speakers");
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{1, 1, 1}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{}));
  assert(state.commitSync(11));
  assert(state.candidateHandle(DataFlow::kOutput) == 3);
  assert(std::strcmp(state.candidateName(DataFlow::kOutput), "HDMI renamed") ==
         0);
  assert(state.endpointCount(DataFlow::kInput) == 0);

  SetRequest request{};
  assert(state.beginSetRequest(DataFlow::kOutput, request));
  assert(request.request_id == 1);
  assert(request.generation == 11);
  assert(request.handle == 3);
  assert(request.flow == DataFlow::kOutput);
  assert(state.setPending());
  assert(!state.beginSetRequest(DataFlow::kOutput, request));
  assert(state.lastError() == StateError::kBusy);
  assert(!state.cycleCandidate(DataFlow::kOutput, 1));
  assert(!state.finishSetRequest(99));
  assert(state.lastError() == StateError::kUnexpectedRequestId);
  assert(state.finishSetRequest(1));
  assert(state.awaitingSync());
  assert(!state.beginSetRequest(DataFlow::kOutput, request));
  assert(state.lastError() == StateError::kBusy);

  assert(state.beginSync(12, 2, 0));
  add(state, DataFlow::kOutput, 4, "First");
  assert(!state.addEndpoint(DataFlow::kOutput, 4, "Duplicate", 9));
  assert(state.lastError() == StateError::kDuplicateHandle);
  state.cancelSync();

  assert(state.beginSync(13, 2, 0));
  add(state, DataFlow::kOutput, 4, "Only one");
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{4, 4, 4}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{}));
  assert(!state.commitSync(13));
  assert(state.lastError() == StateError::kCountMismatch);
  assert(state.generation() == 11);
  assert(state.awaitingSync());

  assert(state.beginSync(14, 1, 0));
  add(state, DataFlow::kOutput, 4, "Valid");
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{9, 9, 9}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{}));
  assert(!state.commitSync(14));
  assert(state.lastError() == StateError::kInvalidHandle);
  assert(state.generation() == 11);

  assert(state.beginSync(15, 1, 0));
  add(state, DataFlow::kOutput, 5, "Next");
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{5, 5, 5}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{}));
  assert(state.commitSync(15));
  assert(!state.awaitingSync());
  assert(state.beginSetRequest(DataFlow::kOutput, request));
  assert(request.request_id == 2);
  assert(request.generation == 15);
  assert(request.handle == 5);

  state.clear();
  assert(!state.beginSync(0, 0, 0));
  assert(state.lastError() == StateError::kInvalidCount);

  assert(!state.ready());
  assert(std::strcmp(state.currentName(DataFlow::kOutput), "(none)") == 0);

  state.clear();
  assert(state.beginSync(20, 0, 0));
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{}));
  assert(state.commitSync(20));
  assert(state.endpointCount(DataFlow::kOutput) == 0);
  assert(!state.cycleCandidate(DataFlow::kOutput, 1));

  state.clear();
  assert(state.beginSync(21, 1, 1));
  add(state, DataFlow::kOutput, 1, "Only output");
  add(state, DataFlow::kInput, 1, "Only input");
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{1, 1, 1}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{1, 1, 1}));
  assert(state.commitSync(21));
  assert(state.cycleCandidate(DataFlow::kOutput, 1));
  assert(state.candidateHandle(DataFlow::kOutput) == 1);

  state.disconnectSession();
  assert(!state.ready());
  assert(!state.beginSetRequest(DataFlow::kOutput, request));
  assert(state.lastError() == StateError::kNotReady);
  assert(std::strcmp(state.currentName(DataFlow::kOutput), "Only output") ==
         0);
  assert(std::strcmp(state.currentName(DataFlow::kInput), "Only input") == 0);

  state.clear();
  assert(state.beginSync(22, 64, 64));
  for (std::uint16_t handle = 1; handle <= 64; ++handle) {
    char name[32]{};
    std::snprintf(name, sizeof(name), "Endpoint %u", handle);
    add(state, DataFlow::kOutput, handle, name);
    add(state, DataFlow::kInput, handle, name);
  }
  assert(state.setCurrent(DataFlow::kOutput, CurrentHandles{64, 64, 64}));
  assert(state.setCurrent(DataFlow::kInput, CurrentHandles{64, 64, 64}));
  assert(state.commitSync(22));
  assert(state.endpointCount(DataFlow::kOutput) == 64);
  assert(state.endpointCount(DataFlow::kInput) == 64);
  assert(state.cycleCandidate(DataFlow::kOutput, 1));
  assert(state.candidateHandle(DataFlow::kOutput) == 1);
  return 0;
}
