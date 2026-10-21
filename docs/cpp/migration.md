# C++ Migration — piano (docs/cpp/migration.md)

Motivazione: portare il kernel deterministico dall Rust (engine/crates/core,
rts-game/crates/shared) a C++20 mantenendo **parità bit-esatta e di sequenza**
con la implementazione Rust esistente. I crate Rust restano il riferimento
(oracle) e NON vengono toccati: green = engine 17/17, rts 17/17, rim 9/9.

## Regole prese in eredità (da docs/architecture.md, docs/determinism.md)

1. Fixed-point, non floating-point: Q16.16 `int64` (alias C++ `Fixed1616` =
   `struct{int64_t 0;}`). Vietato `float`/`double` in tutti i path di sim.
2. Nessun RNG globale: SplitMix64 con seed esplicito; stessa seed + stessa
   sequenza di chiamate = stessa stream, ovunque.
3. `std::unordered_map` vietato nella sim. `std::vector` ordinati / `std::map`.
   Vietato `std::hash` sui path deterministici.
4. Loop a tick fisso. Input sempre ordinati per (tick, seq) e (player, entity,
   kind_disc, params). Nessun `std::chrono` / time-to-live nel loop.
5. Sim single-thread. Nessun `std::thread`, `std::mutex`, `thread_local`,
   `volatile`, o atomics nelle path deterministici.
6. Parallelismo solo se reduce bit-esatta (M1: NO — single-thread esplicito).

## Fasi

- **M1 (task-4, questo):** `engine_cpp/` — core deterministico:
  Fixed1616, SplitMix64 (`Rng`), hash_fold/hash_words (FNV-1a),
  Command/Event/Tick, Scheduler, ReplayRecorder, isqrt.
  CMake + C++20 + ctest. Test vettoriali **generati dai binari Rust**:
  `engine_cpp/testdata/*.json` dumpati eseguendo binari Rust che leggono le
  API esatte dei crate (`engine/crates/core`, `rts-game/crates/shared`).
- **M2 (task-5):** port `rts-game` → `rts_game_cpp/`; parity golden (stesso
  input log → stesso hash finale) vs `engine/crates/*`.
- **M3 (task-5):** port `rimworld-game` → `rim_game_cpp/`; golden parity;
  demo zip.

## Struttura prevista di engine_cpp (M1)

```
engine_cpp/
├── CMakeLists.txt          # C++20, CTest, targets
├── include/engine/         # fixed1616.hpp rng.hpp hash.hpp command.hpp
│                           # event.hpp tick.hpp scheduler.hpp replay.hpp isqrt.hpp
├── src/                    # .cpp dove serve (isqrt, scheduler)
├── tools/dump_vectors.rs   # binari Rust → JSON vettori (oracle)
└── test/                   # test exe per unità + test_parity (legge testdata/)
```

## Contratto di parità (M1)

- `Rng::new(seed)` = `state=seed` poi scarta un `next_u64()` (stessa identica
  semantica Rust — `Rng::new in engine/crates/core/src/lib.rs).
- `hash_fold(acc,word) = ((acc ^ word) * 0x100001b3) rotl 11` (`rotate_left`
  x86/ARM = C++ `std::rotl` / modulo). `hash_words` piega la slice da
  `0xcbf29ce484222325`.
- Hasher FNV-1a canonico (rts shared): XOR-byte, moltiplica per
  `0x100000001b3`, init `0xcbf29ce484222325`; `u32/u16/i16/u64` come LE bytes.
- `div_floor` euclideo (floor, non trunc) per `from_rational` e `wrapping_div`.
- `isqrt_u128` bit-pair radix-4; `fixed_sqrt(v)=isqrt(v<<16)` per `v>0`, else 0.
- `Scheduler::step`: sort stable by (tick, seq); consumo di tutto
  `tick<=next` in ordine `cmd = (tick, seq, command)`. `seq` monotonico.
- `ReplayRecorder::record`: piega digest di (tick, payload_hash) in ordine di
  stream. `tick_digest(tick, hashes)` piega `hashes` partendo da `tick`.
- `Command::sort_key` e hash canonico (rts shared) byte-identici in C++ — vedi
  networking.md.

## Generazione dei vettori (fonte di verità: Rust)

`engine_cpp/tools/dump_vectors.rs` è un crate eseguibile che:
1. Duplica `engine/crates/core` (o `use engine_core as`) per generare:
   stream SplitMix64 per seed {0, 1, 0xDEADBEEF, 0x123456789ABCDEF}
   × N=256 `next_u64`;
2. stream `hash_fold`/`hash_words` per input deterministici;
3. `Hasher` rts-shared per stream `Command` e `TickCommands`;
4. `isqrt`/`fixed_sqrt` per input fissa;
5. `Scheduler` sequence (enq/step) con transcript (tick, seq, hash) per replay.
Output: `testdata/rng_seeds.json`, `hash_streams.json`, `command_hashes.json`,
`isqrt_vectors.json`, `scheduler_trace.json`.

## Regole d'oro (CI-enforced dove possibile)

1. NO `float`/`double` in path sim. Grep gate: nessun token float/double in
   `engine_cpp/src` e `engine_cpp/include` (commentari esclusi).
2. NO `std::unordered_map`/`std::hash` in sim.
3. NO `std::random`/`std::chrono` nella sim.
4. Input sempre ordinati (tick, seq) / (player, entity, kind_disc).
5. Single-thread (nessun `<thread>`, `<mutex>`, `<atomic>` in sim).

## Cosa NON è ora (M4+)

- Multicore, GPU, MCP, AI. Vedi docs/parallelism.md, docs/gpu.md,
  docs/mcp-server.md, docs/roadmap.md.
