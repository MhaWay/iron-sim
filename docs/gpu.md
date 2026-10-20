# GPU (future) — principi

## Motivazione
Colony sim + future AI: needs, pathfinding, job selection, e in futuro
neural-network inference. La CPU multicore può gestire molto, ma per
(1) molti agent e (2) modelli ML, GPU è il posto giusto.

## Regole di determinismo on-device
1. Nessun float. Usare fixed-point integer in GPU (shader hanno integer ops;
   in compute via i16/i32/i64). Se serve float, va accettato che può essere
   diverso su architetture diverse: quindi NON usarlo nella sim.
2. Ordine dei thread in un kernel può variare, quindi: kernel deve essere
   commutativo/associativo, oppure con merge deterministico a fine kernel.
3. Reduction su GPU: usare tree-reduce deterministico per struttura, non
   warp-shuffle che dipende da warp scheduling.
4. Atomi: evitare atomic non associativi. Per integer sum, atomic_add è
   associativo, quindi sicuro. Per hash, XOR è associativo, sicuro.
5. La GPU non deve mai leggere lo stato "live" della CPU senza un lockstep
   (snapshot), e deve restituire risultati in buffer ordinati.

## Stack
- wgpu (compute) o Vulkan via ash.
- Crate engine-gpu (nuova, più avanti): buffer ring, kernel registry,
  deterministic reduction.

## Regola pratica
La GPU può essere usata per: pathfinding (flow field su griglia),
need decay (per pawn, commutativo), job scoring, e in futuro inference.
La GPU NON può essere usata per: RNG (SplitMix64 resta CPU, seedato),
event order (resta CPU).
