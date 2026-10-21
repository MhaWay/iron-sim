// engine_cpp library TU. All primitives are header-only constexpr/inline; this
// file anchors the library target and is the home for future out-of-line code.
// SIM PATH RULE (docs/cpp/determinism-cpp.md): NO float/double, NO
// unordered_map/hash, NO <random>/<chrono>/<thread>/<mutex>/<atomic>.
#include "engine/fixed1616.hpp"
#include "engine/hash.hpp"
#include "engine/isqrt.hpp"
#include "engine/rng.hpp"
#include "engine/scheduler.hpp"
#include "engine/replay.hpp"
#include "engine/command.hpp"
#include "engine/event.hpp"

namespace engine {
const char* version() noexcept { return "engine_cpp M1"; }
} // namespace engine
