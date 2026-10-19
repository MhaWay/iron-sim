# Determinismo — regole per gli sviluppatori

Perché non Unreal: il suo pathfinding e la fisica non offrono garanzie
bit-esatte cross-platform. Il nostro engine è diverso per questo.

## Regole vincolanti (CI-enforced dove possibile)

1. **VIETATO f32 / f64** in tutte le crate di simulazione. Solo fixed-point.
2. **VIETATO** `HashMap` con hasher standard in sim code. Ordina sempre.
3. **VIETATO** `SystemTime` / RNG globali nel loop di simulazione.
4. Input ordinati per chiave totale (tick, player, entity, action_id).
5. Loop a tick fisso, niente dt variabile.
6. Parallelismo solo se la riduzione è bit-esatta (preferire single-thread ora).

## Permessi nella vista/rete/UI

- Il rendering può usare floating (solo display), purché non si legga dallo
  stato di render nella sim.
- Il networking può serializzare/deserializzare in floating purché poi converta
  in fixed prima della sim.
