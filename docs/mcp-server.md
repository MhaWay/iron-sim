# MCP server (Model Context Protocol)

Obiettivo: pilotare l'engine da un agent, non solo via CLI. Il progetto
deve essere integrabile in un agent come te (DSH) via MCP tool calls.

## Cosa deve esporre il MCP server (nuova crate / `engine-mcp`)
- `tick_step{count}`: far avanzare la simulazione di N tick, ritorna hash + delta.
- `spawn_command{cmd}`: iniettare una command, ritorna ack.
- `world_query{query}`: leggere stato (entities, componenti) in JSON canonico
  deterministico.
- `world_diff{from, to}`: diff tra hash di tick.
- `save_state/load_state{path}`: snapshot.
- `run_replay{file}`: replay.
- `lint_world`: controllare violazioni determinismo (float, hashmap non ordinate).
- `bench_parallel{chunks}`: test multicore deterministico.

## Come sarà strutturato (roadmap)
```
engine/crates/mcp/
   src/lib.rs       MCP handler (types, dispatch)
   src/main.rs      stdio binary (cargo run -p engine-mcp)
```
Il server espone MCP tools via `rmcp` crate o `mcp-server`.

## Regola d'oro
Non importa chi chiama: agent, CLI, GUI. La sim è deterministica.
