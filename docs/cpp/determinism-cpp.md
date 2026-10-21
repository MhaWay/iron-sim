# Determinismo C++ — regole per gli sviluppatori (docs/cpp/determinism-cpp.md)

Il kernel C++ replica il determinismo del kernel Rust (engine/crates/core +
rts-game/crates/shared). Le regole vincolanti (CI-enforced dove possibile)
sono identiche a docs/determinism.md, tradotte in restrizioni C++:

## Regole vincolanti

1. **VIETATO `float` / `double`** in ogni path di sim (src/include). Solo
   fixed-point `Fixed1616` (int64 Q16.16). Il gate CI è grep: nessun
   `float`/`double` in `engine_cpp/src/**` e `engine_cpp/include/**`.
2. **VIETATO `std::unordered_map` / `std::unordered_set` / `std::hash`** nei
   path deterministici. Solo `std::vector` (ordinati esplicitamente) o
   `std::map` (ordine deterministico per chiavi).
3. **VIETATO `<random>` / `<chrono>` / `std::thread` / `std::mutex` /`std::atomic`**
   nella sim. RNG = `engine::Rng` (SplitMix64, seed esplicito); tempo = tick.
4. **Ordinamento totale**: input ordinati per (tick, seq); comandi per
   (player, sort_key). `sort_key` replica `Command::sort_key()` di rts shared:
   (entity, kind_disc, params) con discriminanti congelati.
5. **Tick fisso, single-thread**, nessun `dt` variabile.
6. **Hash canonico**: FNV-1a. `hash_fold(acc,w)=((acc^w)*0x100001b3) rotl 11`,
   `hash_words` piega slice da `0xcbf29ce484222325`; `Hasher::byte` XOR poi
   `*0x100000001b3`; numeri via little-endian.
7. **Parità**: `engine_cpp/test/parity_test` confronta i valori con
   `engine_cpp/testdata/*.json` generati da `engine_cpp/tools/dump_vectors.rs`
   (crate Rust). Un fallimento indica divergenza Rust/C++: prima verificare
   `tools/dump_vectors.rs` (rigenerare), poi `engine_cpp/`.

## Semantica di overflow (esplicita)

- `wrapping_add/sub`: due's complement mod 2^64 (`__int128` per mul, shift
  `>> 16` aritmetico, poi cast `int64_t` = wrapping in C++ su clang/gcc x86-64).
  La moltiplicazione `wrapping_mul` usa `int128` e `>>16` poi `int64_t`.
- `div_floor`: euclideo (floor), identico a `div_floor` di Rust; in C++ con
  `std::div` + correzione segno.
- `wrapping_abs` su `INT64_MIN`: undefined in C++, in Rust overflow in debug
  panic e wrapping in release. Per la sim non capita mai (vettori e distanza
  sempre `>=0`), ma va documentato: C++ implementa `x==INT64_MIN ? INT64_MIN :
  (x<0?-x:x)` per evitare UB.

## Test di determinismo richiesti

- `rng_streams_reproducible`: due `Rng` uguali → stesse uscite (64+ chiamate).
- `rng_stream_parity`: `Rng(seed)` == `Rng` dello stream in `rng_seeds.json`.
- `hash_fold/hash_words` su stream deterministici == `hash_streams.json`.
- `scheduler_orders_and_times` e `scheduler_trace`: (tick, seq, payload)
  uguali alla trace generata da Rust.
- `replay_digest_stream`: `ReplayRecorder::digest` e `tick_digest` ==
  `replay_digests.json`.
- `isqrt_parity`: `isqrt_u128` e `fixed_sqrt` == `isqrt_vectors.json`
  (valori verificati contro Python math.isqrt nel crate Rust).
- `fnv1a_known_vectors` + `command_hash_parity`: `Hasher` e `TickCommands::hash`
  byte-identici a Rust per stream `command_hashes.json`.
- `fixed_math`: `from_rational`, `wrapping_mul`/`div`, `to_int_trunc` ==
  `fixed_values.json`.

## Cosa è permesso in vista / rete / UI

- `float`/`double` solo in code di rendering/interfaccia, non nella sim.
- `std::unordered_map` e `std::hash` solo fuori dal path deterministico.
