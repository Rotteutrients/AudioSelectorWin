#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

#include "config.h"

namespace audio_selector::state {

enum class DataFlow : std::uint8_t {
  kOutput,
  kInput,
};

enum class StateError : std::uint8_t {
  kNone,
  kSyncAlreadyStarted,
  kNoSyncStarted,
  kInvalidCount,
  kInvalidHandle,
  kDuplicateHandle,
  kNameTooLong,
  kCountMismatch,
  kGenerationMismatch,
  kNotReady,
  kBusy,
  kUnexpectedRequestId,
};

struct CurrentHandles {
  std::uint16_t console = 0;
  std::uint16_t multimedia = 0;
  std::uint16_t communications = 0;
};

struct Endpoint {
  std::uint16_t handle = 0;
  std::uint16_t name_length = 0;
  std::array<char, config::kMaximumEndpointNameLength + 1> name{};
};

struct SetRequest {
  DataFlow flow = DataFlow::kOutput;
  std::uint32_t request_id = 0;
  std::uint32_t generation = 0;
  std::uint16_t handle = 0;
};

class DeviceState final {
 public:
  bool beginSync(std::uint32_t generation, std::uint16_t output_count,
                 std::uint16_t input_count);
  bool addEndpoint(DataFlow flow, std::uint16_t handle, const char* name,
                   std::size_t name_length);
  bool setCurrent(DataFlow flow, CurrentHandles handles);
  bool commitSync(std::uint32_t generation);
  void cancelSync();
  void disconnectSession();
  void clear();

  bool cycleCandidate(DataFlow flow, int direction);
  bool beginSetRequest(DataFlow flow, SetRequest& request);
  bool finishSetRequest(std::uint32_t request_id);
  void cancelSetRequest();

  bool ready() const { return ready_; }
  bool syncInProgress() const { return sync_in_progress_; }
  bool setPending() const { return set_pending_; }
  bool awaitingSync() const { return awaiting_sync_; }
  std::uint32_t generation() const { return active_.generation; }
  StateError lastError() const { return last_error_; }
  std::uint16_t currentHandle(DataFlow flow) const;
  std::uint16_t candidateHandle(DataFlow flow) const;
  bool rolesSplit(DataFlow flow) const;
  const char* currentName(DataFlow flow) const;
  const char* candidateName(DataFlow flow) const;
  std::size_t endpointCount(DataFlow flow) const;

 private:
  struct FlowState {
    std::array<Endpoint, config::kMaximumEndpointsPerFlow> endpoints{};
    std::size_t count = 0;
    CurrentHandles current{};
    std::uint16_t current_handle = 0;
    std::uint16_t candidate_handle = 0;
  };

  struct Snapshot {
    std::uint32_t generation = 0;
    FlowState output{};
    FlowState input{};
  };

  FlowState& flow(Snapshot& snapshot, DataFlow data_flow);
  const FlowState& flow(const Snapshot& snapshot, DataFlow data_flow) const;
  const Endpoint* find(const FlowState& flow_state,
                       std::uint16_t handle) const;
  bool validateAndFinalizeFlow(FlowState& pending_flow,
                              const FlowState& previous_flow,
                              std::uint16_t expected_count);
  bool fail(StateError error);

  Snapshot active_{};
  Snapshot pending_{};
  std::uint16_t expected_output_count_ = 0;
  std::uint16_t expected_input_count_ = 0;
  bool ready_ = false;
  bool sync_in_progress_ = false;
  bool set_pending_ = false;
  bool awaiting_sync_ = false;
  std::uint32_t pending_request_id_ = 0;
  std::uint32_t next_request_id_ = 1;
  StateError last_error_ = StateError::kNone;
};

}  // namespace audio_selector::state
