# AutoCoder clean runtime

This workspace is the new runtime skeleton. It does not depend on the donor
application in `src/`, `backend/`, or `src-tauri/`.

Dependency direction is:

```text
desktop UI -> application -> orchestration -> ledger contract <- persistence
                    |             |
                    +----------> contracts
```

`application` is the composition root and contains empty boundary modules for
Workspace, Provider Runtime, Process Supervisor, and Diagnostics. Those modules
cannot transition tasks. `orchestration` is the only owner of task transitions;
`persistence` only implements the append-only ledger contract.

The lifecycle contract projects `created`, `ready`, `blocked`, and `completed`
only by replaying the versioned task stream. Semantic-verification evidence has
a stable identity, outcome, verifier provenance, and a versioned applicability
basis tied to the task-creation event and an opaque workspace input revision.
That input revision is an AutoCoder-owned reference boundary, not a fabricated
Workspace implementation; a later Workspace subsystem can bind it to revisions
or hashes.

Verification is appended as a durable fact before completion and does not
itself change lifecycle state. Only the orchestration-owned completion command
can append `TaskCompleted`, and only after replay finds the selected verified
evidence applicable to the current input basis. Generic lifecycle transitions
cannot complete tasks. Replay never reruns verification or consults current
time, filesystem, provider, or network state, and rejects missing, failed,
stale, conflicting, or version-incompatible evidence/history.

Version 1 create events and pending UI submissions written before
`input_revision` was introduced are compatibly upcast from their stable create
event identity. Both boundaries derive the same deterministic legacy input
reference, so an unknown-outcome pre-upgrade create remains an exact retry.

`desktop` is a separate Tauri composition and serves the intentionally minimal
`ui/` shell; it shares no source or runtime state with the donor Tauri app.

Validate the current contract slice with:

```text
cargo test --manifest-path rewrite/Cargo.toml --workspace
node --test rewrite/ui/main.test.mjs
```

The Rust workspace suite covers lifecycle, replay, and Ledger guarantees. The
focused Node test covers the UI's uncertain-outcome create/projection
reconciliation path. Run the new desktop with `cargo run --manifest-path
rewrite/Cargo.toml -p autocoder-desktop` when an interactive platform check is
needed.
