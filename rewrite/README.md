# AutoCoder clean runtime

This workspace is the new runtime skeleton. It does not depend on the donor
application in `src/`, `backend/`, or `src-tauri/`.

Dependency direction for the first slice is:

```text
desktop UI -> application -> orchestration -> ledger contract <- persistence
                    |             |
                    +----------> contracts
```

`application` is the composition root and contains empty boundary modules for
Workspace, Provider Runtime, Process Supervisor, and Diagnostics. Those modules
cannot transition tasks. `orchestration` is the only owner of task transitions;
`persistence` only implements the append-only ledger contract.

`desktop` is a separate Tauri composition and serves the intentionally minimal
`ui/` shell; it shares no source or runtime state with the donor Tauri app.

Run the contract slice with `cargo test --manifest-path rewrite/Cargo.toml` and
the new desktop with `cargo run --manifest-path rewrite/Cargo.toml -p
autocoder-desktop`.

