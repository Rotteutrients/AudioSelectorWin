#include "state/device_state.h"

#include <cstring>

namespace audio_selector::state {
namespace {

constexpr const char* kNoEndpointName = "(none)";

std::uint16_t displayCurrentHandle(const CurrentHandles& handles) {
  if (handles.multimedia != 0) {
    return handles.multimedia;
  }
  if (handles.console != 0) {
    return handles.console;
  }
  return handles.communications;
}

}  // namespace

bool DeviceState::beginSync(std::uint32_t generation,
                            std::uint16_t output_count,
                            std::uint16_t input_count) {
  if (sync_in_progress_) {
    return fail(StateError::kSyncAlreadyStarted);
  }
  if (set_pending_) {
    return fail(StateError::kBusy);
  }
  if (output_count > config::kMaximumEndpointsPerFlow ||
      input_count > config::kMaximumEndpointsPerFlow || generation == 0) {
    return fail(StateError::kInvalidCount);
  }

  pending_ = {};
  pending_.generation = generation;
  expected_output_count_ = output_count;
  expected_input_count_ = input_count;
  sync_in_progress_ = true;
  last_error_ = StateError::kNone;
  return true;
}

bool DeviceState::addEndpoint(DataFlow data_flow, std::uint16_t handle,
                              const char* name, std::size_t name_length) {
  if (!sync_in_progress_) {
    return fail(StateError::kNoSyncStarted);
  }
  if (handle == 0) {
    return fail(StateError::kInvalidHandle);
  }
  if (name == nullptr || name_length > config::kMaximumEndpointNameLength) {
    return fail(StateError::kNameTooLong);
  }

  FlowState& pending_flow = flow(pending_, data_flow);
  if (find(pending_flow, handle) != nullptr) {
    return fail(StateError::kDuplicateHandle);
  }
  if (pending_flow.count >= config::kMaximumEndpointsPerFlow) {
    return fail(StateError::kInvalidCount);
  }

  Endpoint& endpoint = pending_flow.endpoints[pending_flow.count++];
  endpoint.handle = handle;
  endpoint.name_length = static_cast<std::uint16_t>(name_length);
  std::memcpy(endpoint.name.data(), name, name_length);
  endpoint.name[name_length] = '\0';
  return true;
}

bool DeviceState::setCurrent(DataFlow data_flow, CurrentHandles handles) {
  if (!sync_in_progress_) {
    return fail(StateError::kNoSyncStarted);
  }
  flow(pending_, data_flow).current = handles;
  return true;
}

bool DeviceState::commitSync(std::uint32_t generation) {
  if (!sync_in_progress_) {
    return fail(StateError::kNoSyncStarted);
  }
  if (generation != pending_.generation) {
    return fail(StateError::kGenerationMismatch);
  }

  if (!validateAndFinalizeFlow(pending_.output, active_.output,
                              expected_output_count_) ||
      !validateAndFinalizeFlow(pending_.input, active_.input,
                              expected_input_count_)) {
    sync_in_progress_ = false;
    return false;
  }

  active_ = pending_;
  pending_ = {};
  sync_in_progress_ = false;
  ready_ = true;
  awaiting_sync_ = false;
  last_error_ = StateError::kNone;
  return true;
}

void DeviceState::cancelSync() {
  pending_ = {};
  sync_in_progress_ = false;
}

void DeviceState::disconnectSession() {
  pending_ = {};
  expected_output_count_ = 0;
  expected_input_count_ = 0;
  ready_ = false;
  sync_in_progress_ = false;
  set_pending_ = false;
  awaiting_sync_ = false;
  pending_request_id_ = 0;
  last_error_ = StateError::kNone;
}

void DeviceState::clear() {
  active_ = {};
  pending_ = {};
  expected_output_count_ = 0;
  expected_input_count_ = 0;
  ready_ = false;
  sync_in_progress_ = false;
  set_pending_ = false;
  awaiting_sync_ = false;
  pending_request_id_ = 0;
  next_request_id_ = 1;
  last_error_ = StateError::kNone;
}

bool DeviceState::cycleCandidate(DataFlow data_flow, int direction) {
  FlowState& active_flow = flow(active_, data_flow);
  if (!ready_ || sync_in_progress_ || set_pending_ || awaiting_sync_ ||
      active_flow.count == 0 || direction == 0) {
    return false;
  }

  std::size_t index = 0;
  for (; index < active_flow.count; ++index) {
    if (active_flow.endpoints[index].handle == active_flow.candidate_handle) {
      break;
    }
  }
  if (index == active_flow.count) {
    index = 0;
  } else if (direction < 0) {
    index = (index + active_flow.count - 1) % active_flow.count;
  } else {
    index = (index + 1) % active_flow.count;
  }
  active_flow.candidate_handle = active_flow.endpoints[index].handle;
  return true;
}

bool DeviceState::beginSetRequest(DataFlow data_flow, SetRequest& request) {
  if (!ready_) {
    return fail(StateError::kNotReady);
  }
  if (sync_in_progress_ || set_pending_ || awaiting_sync_) {
    return fail(StateError::kBusy);
  }

  const std::uint16_t handle = candidateHandle(data_flow);
  if (handle == 0 || find(flow(active_, data_flow), handle) == nullptr) {
    return fail(StateError::kInvalidHandle);
  }

  request = {data_flow, next_request_id_, active_.generation, handle};
  pending_request_id_ = next_request_id_;
  set_pending_ = true;
  next_request_id_ = next_request_id_ == 0xFFFFFFFFU ? 1 : next_request_id_ + 1;
  last_error_ = StateError::kNone;
  return true;
}

bool DeviceState::finishSetRequest(std::uint32_t request_id) {
  if (!set_pending_ || request_id != pending_request_id_) {
    return fail(StateError::kUnexpectedRequestId);
  }
  set_pending_ = false;
  awaiting_sync_ = true;
  pending_request_id_ = 0;
  last_error_ = StateError::kNone;
  return true;
}

void DeviceState::cancelSetRequest() {
  set_pending_ = false;
  pending_request_id_ = 0;
}

std::uint16_t DeviceState::currentHandle(DataFlow data_flow) const {
  return flow(active_, data_flow).current_handle;
}

std::uint16_t DeviceState::candidateHandle(DataFlow data_flow) const {
  return flow(active_, data_flow).candidate_handle;
}

bool DeviceState::rolesSplit(DataFlow data_flow) const {
  const CurrentHandles& current = flow(active_, data_flow).current;
  const std::uint16_t handles[] = {
      current.console, current.multimedia, current.communications};
  std::uint16_t first = 0;
  for (const std::uint16_t handle : handles) {
    if (handle == 0) {
      continue;
    }
    if (first == 0) {
      first = handle;
    } else if (handle != first) {
      return true;
    }
  }
  return false;
}

const char* DeviceState::currentName(DataFlow data_flow) const {
  const FlowState& active_flow = flow(active_, data_flow);
  const Endpoint* endpoint = find(active_flow, active_flow.current_handle);
  return endpoint == nullptr ? kNoEndpointName : endpoint->name.data();
}

const char* DeviceState::candidateName(DataFlow data_flow) const {
  const FlowState& active_flow = flow(active_, data_flow);
  const Endpoint* endpoint = find(active_flow, active_flow.candidate_handle);
  return endpoint == nullptr ? kNoEndpointName : endpoint->name.data();
}

std::size_t DeviceState::endpointCount(DataFlow data_flow) const {
  return flow(active_, data_flow).count;
}

DeviceState::FlowState& DeviceState::flow(Snapshot& snapshot,
                                          DataFlow data_flow) {
  return data_flow == DataFlow::kOutput ? snapshot.output : snapshot.input;
}

const DeviceState::FlowState& DeviceState::flow(
    const Snapshot& snapshot, DataFlow data_flow) const {
  return data_flow == DataFlow::kOutput ? snapshot.output : snapshot.input;
}

const Endpoint* DeviceState::find(const FlowState& flow_state,
                                  std::uint16_t handle) const {
  if (handle == 0) {
    return nullptr;
  }
  for (std::size_t index = 0; index < flow_state.count; ++index) {
    if (flow_state.endpoints[index].handle == handle) {
      return &flow_state.endpoints[index];
    }
  }
  return nullptr;
}

bool DeviceState::validateAndFinalizeFlow(FlowState& pending_flow,
                                          const FlowState& previous_flow,
                                          std::uint16_t expected_count) {
  if (pending_flow.count != expected_count) {
    return fail(StateError::kCountMismatch);
  }

  const CurrentHandles& current = pending_flow.current;
  const std::uint16_t handles[] = {
      current.console, current.multimedia, current.communications};
  for (const std::uint16_t handle : handles) {
    if (handle != 0 && find(pending_flow, handle) == nullptr) {
      return fail(StateError::kInvalidHandle);
    }
  }

  pending_flow.current_handle = displayCurrentHandle(current);
  if (find(pending_flow, previous_flow.candidate_handle) != nullptr) {
    pending_flow.candidate_handle = previous_flow.candidate_handle;
  } else if (pending_flow.current_handle != 0) {
    pending_flow.candidate_handle = pending_flow.current_handle;
  } else if (pending_flow.count != 0) {
    pending_flow.candidate_handle = pending_flow.endpoints[0].handle;
  }
  return true;
}

bool DeviceState::fail(StateError error) {
  last_error_ = error;
  return false;
}

}  // namespace audio_selector::state
