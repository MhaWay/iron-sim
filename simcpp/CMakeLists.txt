# simcpp — deterministic C++ games (M2/M3 of docs/cpp/migration.md).
# Ports rts-game + rimworld-game sims to C++20 with byte-parity against the Rust
# oracles in /engine and /rts-game (see simcpp/oracle/).
cmake_minimum_required(VERSION 3.22)
project(simcpp VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo)
endif()

set(ENGINE_CPP_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../engine_cpp)

# --- Deterministic sim lib. --------------------------------------------------
add_library(sim_cpp STATIC
  src/path.cpp
  src/rts_sim.cpp
  src/rim_sim.cpp
  src/demo_bridge.cpp
)
target_include_directories(sim_cpp PUBLIC include ${ENGINE_CPP_DIR}/include)
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
  target_compile_options(sim_cpp PRIVATE -Wall -Wextra)
endif()

enable_testing()

# --- Determinism grep gate (docs/cpp/determinism-cpp.md). --------------------
add_test(NAME grep_gate_no_float_no_banned
  COMMAND ${CMAKE_COMMAND} -P ${CMAKE_CURRENT_SOURCE_DIR}/tools/grep_gate.cmake)

# --- Test suites. ------------------------------------------------------------
set(ORACLE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/oracle_data)

function(simcpp_test name)
  add_executable(${name} test/${name}.cpp)
  target_include_directories(${name} PRIVATE test include ${ENGINE_CPP_DIR}/include)
  target_link_libraries(${name} PRIVATE sim_cpp)
  target_compile_options(${name} PRIVATE -Wall -Wextra)
  add_test(NAME ${name} COMMAND ${name} ${ORACLE_DIR})
endfunction()

simcpp_test(sim_selftest)
simcpp_test(determinism_cpp)
simcpp_test(rts_economy_determinism_cpp)
simcpp_test(rim_parity)

# --- Apps. -------------------------------------------------------------------
function(simcpp_app name)
  add_executable(${name} apps/${name}.cpp)
  target_include_directories(${name} PRIVATE include ${ENGINE_CPP_DIR}/include)
  target_link_libraries(${name} PRIVATE sim_cpp)
  target_compile_options(${name} PRIVATE -Wall -Wextra)
endfunction()

simcpp_app(demo)
simcpp_app(dedicated_server)
simcpp_app(rts_client)

# P2P test uses UDP loopback; runs two in-process clients.
add_executable(net_p2p_parity test/net_p2p_parity.cpp)
target_include_directories(net_p2p_parity PRIVATE test include ${ENGINE_CPP_DIR}/include)
target_link_libraries(net_p2p_parity PRIVATE sim_cpp)
add_test(NAME net_p2p_parity COMMAND net_p2p_parity)
