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

The current lifecycle contract projects `created`, `ready`, `blocked`, and
`completed` by replaying the versioned task stream. The orchestration core owns
the transition table and rejects invalid or incompatible history. The
application and desktop layers expose a read-only projection query; the UI only
renders that result. `completed` is reserved for a future orchestration-owned
path with durable semantic-verification evidence; until that contract exists,
the generic lifecycle transition cannot produce it and replay rejects an
unverified completion event rather than projecting success.

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
