# Roadmap (visione futura)

## Fase 1 (oggi, fatto)
- Engine deterministico single-thread: core, ecs, path, sim.
- RTS + colony sim scaffolati, 38 test green.
- Lockstep, A* riproducibile, hash di stato deterministici.
- MCP server: API per pilotare l'engine da un agent.

## Fase 2 — multicore (breve-medio termine)
- Crate `engine-parallel`: chunk deterministico, phase barrier, monoidal reduce.
- Systems che dichiarano `fn is_parallel_safe(world) -> bool`: se true, il
  runtime distribuisce su worker in chunk deterministici.
- Benchmark: same sim, N core diversi, stato hash identico.
- Target: colony sim con ≥10k pawn, FPS stabile, stesso hash.

## Fase 3 — GPU compute (medio termine)
- Crate `engine-gpu` (wgpu): ring buffer, kernel registry, deterministic reduce.
- Kernels: flow field / BFS, need decay, job scoring, collision broad-phase.
- CPU resta owner assoluto di: RNG, ordine eventi, tick loop, lockstep.
- GPU restituisce risultati in buffer ordinati; CPU li consuma in ordine di
  chunk-id deterministico.
- Regola: nessun float in kernel per dati sim.

## Fase 4 — AI su scheda video (futuro, dopo Fase 3)
- Inference on-device (CPU o GPU) per modelli ML leggeri: comportamento di
  pawn, stima utilità, planner.
- Obiettivo: un modello che carica权重 sul GPU (wgpu storage buffer) e
  produce output deterministici.
- Note: l'inferenza deve essere deterministica per replay. Se useremo
  modelli tipo candle/ort, servono test che verifichino bit-exact across runs.
- Possibile path: modello piccolo (MLP quantizzato, fixed-point per layer)
  in Rust, inference via wgpu compute.
- **NON iniziare ora**: Fase 1 deve essere solida.
