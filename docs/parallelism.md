# Multicore deterministico — strategia (roadmap)

## Principio
Il parallelismo DEVE essere una funzione deterministica del (tick, stato)
indipendente dall'ordine di esecuzione reale. L'ordine effettivo delle
completioni è irrilevante purché il risultato sia identico in tutti i
processori e piattaforme.

## Pattern accettati
1. **Work-stealing a phase barrier con riduzione commutativa/associativa.**
   Esempio: sistema "needs decay" per pawn. Ogni pawn ha un update indipendente;
   l'ordine con cui vengono applicati NON conta se la funzione di aggregazione
   è commutativa. Ma attenzione: se ci sono effetti tra pawn (social), servono
   chunk deterministici con merge in ordine di entity-id.
2. **Chunk deterministico.** Partizioniamo le entità per id / regione / bucket.
   Ogni chunk è elaborato in un task separato; merge in ordine di chunk-id.
   La partizione deve essere funzione dello stato (non dell'hardware).
3. **Parallel reduction deterministica.** Per hash di stato su interi: XOR è
   associativo/commutativo, quindi basta che ogni thread XOR i suoi elementi e
   gli hash parziale si XOR-ano. Per sum: usare integer sum con ordine di
   chunk fisso, o usare tree-reduction bilanciato.
4. **Job queues per tick.** Ogni tick pianifica i job, li esegue, fa barrier.
   L'input del job plan è generato in modo deterministico (sort per id), non
   dalla queue OS.

## Pattern VIETATI
- Dipendenza dal numero di core (n_workers influenza il partizionamento).
- Ordine dei risultati dalla queue work-stealing (dipende dallo scheduler OS).
- Ordine dei risultati della CPU SIMD (non sempre bit-identico in floating; qui
  evitiamo float, quindi va bene su integer SIMD).
- Mutex/atomic per stato di sim.

## API proposta (future crate `engine-parallel`)
```rust
pub trait ParallelTask { type Item: Send + 'static; }
pub fn deterministically_chunk<I>(items: &[I], chunks: usize, by: impl KeyFn) -> Vec<Vec<I>>
pub fn par_fold<A, T>(seed: A, chunks: &[T], map: impl Fn(&T)->A, reduce: impl Fn(A,A)->A) -> A
    where A: Monoid + Send;
pub fn phase_barrier<F>(phases: &[&str], f: impl FnMut(&str)) -> Result<(), PhaseError>
```

Regola: `deterministically_chunk` riceve `chunks` fisso (dalla configurazione di
partita, non da core count). Se la partita dice "8 chunk" sempre, la simulazione
ha lo stesso risultato con 1 core o con 128 core.
