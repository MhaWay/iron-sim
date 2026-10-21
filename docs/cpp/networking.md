# Networking C++ — lockstep e serializzazione (docs/cpp/networking.md)

Il kernel C++ (engine_cpp) mantiene la rete lockstep identica al kernel Rust:
**stato iniziale + sequenza di input = partita**. La replay riproduce identica.

## Modello dati (wire)

- `Tick` = `uint64_t` (identico a Rust `engine_core::Tick`).
- `Command`: `{uint64_t payload_hash;}` (core-trasporto — il payload è opaco;
  l'hash canonico lo calcola il layer game con `Hasher`/FNV-1a).
- `Event`: `{uint64_t tick; uint64_t id; uint64_t payload_hash;}`.
- `TickCommands` (batch relay): `{uint64_t tick; vector<pair<uint32_t, Command>>
  cmds;}` + `sort()` per (player, Command::sort_key) e `hash()` canonico.
- `Command` (game) ha discriminanti `kind_disc` congelati (Move=0, Attack=1,
  Stop=2, Build=3, Train=4, Gather=5, Sacrifice=6, Evolve=7, AgeUp=8) —
  mai ri-numerati, come design-factions §6 (rts).

## Regole di rete (identiche a determinism.md §5–6)

1. Relay smista comandi per tick; i client applicano in (tick, seq) e, dentro
   il tick, per (player, sort_key). Mai per ordine di arrivo.
2. `Scheduler` (C++) consuma `tick <= next` in ordine (tick, seq) identico a
   Rust `Scheduler::step`. `seq` = counter monotono globale (non thread-local).
3. `ReplayRecorder`: registra `record(tick, payload_hash)` per ogni comando.
   `digest()` piega in ordine di stream, sensibile all'ordine. `tick_digest`
   per il checksum per-tick del log input.
4. Hash di stato: piega `hash_words`/`hash_fold` (FNV-1a) su campi canonici
   little-endian. Nessun `std::hash`, nessun `unordered_map` iterato per hash.

## Serializzazione (M1/M2)

- Ogni campo numerico è little-endian (endian-agnostic wire). `uint32_t`,
  `uint16_t`, `int16_t`, `uint8_t`, `int64_t`, `uint64_t`.
- `Fixed1616` = `int64_t` raw (little-endian in wire, non float).
- Stringhe/enum: index canonici (come `kind_disc` e `Resource` = Food/Wood/
  Gold/Mana → 0/1/2/3).
- `TickCommands::hash()` = `Hasher`: `u64(tick)`, `u32(len)`, poi per ogni
  (player, Command): `u32(player)` + `kind_disc` + campi LE. Stessa struttura
  byte-per-byte del Rust `TickCommands::hash()`.

## Parità di rete

`parity_test` rilegge `testdata/command_hashes.json`: ogni stream
`TickCommands` produce `hash` identico a quello Rust. `scheduler_trace.json`
verifica che `enqueue`/`step` consumino comandi nella stessa (tick,seq,
payload_hash) del crate Rust.

## Restrizioni (CI-enforced)

- `<thread>`/`<mutex>`/`<atomic>` vietati nel path deterministico (sim,
  scheduler, recorder, hash). Solo in transport/packaging (non in M1).
- Nessun `std::random` nella rete.
- Nessun `std::unordered_map` per iterazioni determinanti; solo `std::map`.
