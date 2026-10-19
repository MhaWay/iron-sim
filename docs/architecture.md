# Iron Sim Engine — Architettura

Motivazione primaria: **determinismo bit-esatto**. Unreal non lo garantisce nel
pathfinding né nella fisica. Serve lockstep netcode per RTS e una sim colony
riproducibile per il gioco stile RimWorld.

## Principi fondanti

1. **Fixed-point, non floating-point.** Tutte le coordinate, velocità, costi e
   euristiche sono interi ```I64F64``` (o simile). Questo elimina divergenze
   tra x86/ARM e tra build diverse. Regola dura nei CI: lint che blocca
   ```f32/f64``` in crate di simulazione.
2. **Nessun RNG globale / nessuna mappa non deterministica.** SplitMix64 per RNG
   (seed esplicito per partita e per entità), ```BTreeMap``` o ```Vec``` ordinati
   per le collezioni. Vietato l'hasher standard di ```HashMap``` in sim code.
3. **Tick deterministico.** Loop a tick fisso. Ogni tick accetta una lista di
   comandi ordinati (per tick, per player, per entity) e produce eventi deterministici.
4. **Pathfinding riproducibile.** A* su griglia con tie-break totale (per costo,
   poi per indice y-maggior), oppure flow field costruito con BFS deterministico.
   Verifica: a parità di input, lo stesso path (identico bit per bit).
5. **Lockstep.** Lo stato iniziale + la sequenza di input è la partita. La
   riproduzione è identica. Registriamo gli input (comandi) più hash di stato
   per tick, non lo snapshot.

## Componenti (crate)

- crates/core    fixed-point math, RNG, scheduler, comandi/eventi, replay recorder
- crates/ecs     ECS deterministica (sparse-set/archetipi senza HashMap non ordinate)
- crates/path    A*, flow field, tie-breaking, verifica path
- crates/sim     (shared types per RTS + colony, se utile)

## Loop di simulazione (concettuale)

tick = N
inputs = comandi_ordinati[N]          # ordinati per (player, entity, action_id)
state = tick_function(state, inputs)  # ECS update deterministici
state_hash = hash(state)              # BTreeMap -> hash stabile
record = (N, state_hash, inputs)

## Determinismo e verifica

- ```crates/core/tests/determinism.rs```: run A e run B con stesso seed + log input
  -> stesso hash finale.
- ```crates/path/tests/replay.rs```: due ricerche A* -> stesse coordinate.
- (later) property tests per il loop completo.
