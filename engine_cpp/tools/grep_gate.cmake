# Determinism gate (docs/cpp/determinism-cpp.md): no float / banned headers
# in the sim path of engine_cpp. Fails with nonzero exit on any violation.
get_filename_component(GATE_ROOT "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
file(GLOB_RECURSE SOURCES
  "${GATE_ROOT}/include/*.hpp"
  "${GATE_ROOT}/src/*.cpp"
  "${GATE_ROOT}/test/*.cpp"
)
list(LENGTH SOURCES N_SOURCES)
if(NOT N_SOURCES GREATER 0)
  message(FATAL_ERROR "grep_gate: no sources found under ${GATE_ROOT}")
endif()
set(FORBIDDEN "float|double|std::unordered_map|std::hash|<random>|<chrono>|<thread>|<mutex>|<atomic>")
foreach(src ${SOURCES})
  file(READ "${src}" text)
  string(REGEX REPLACE "/\\*([^*]|\\*[^*])*\\*/" " " text "${text}")
  string(REGEX REPLACE "[^\n]*//[^\n]*" " " text "${text}")
  string(REGEX REPLACE "[^\n]*#[^\n]*" " " text "${text}")
  if(text MATCHES "${FORBIDDEN}")
    string(REGEX MATCH "${FORBIDDEN}" hit "${text}")
    message(FATAL_ERROR "grep_gate: forbidden token \"${hit}\" in ${src}")
  endif()
endforeach()
message(STATUS "grep_gate: OK (${N_SOURCES} files)")

