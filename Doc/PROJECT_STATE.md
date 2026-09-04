# AutoCoder — PROJECT_STATE

## Дата состояния

4 сентября 2026.

## Текущий этап

**Третий vertical slice clean rewrite реализован: durable semantic-verification contract и
orchestration-owned completion path.**

`Doc/PROJECT_MEMORY.md` остаётся неизменённым **FROZEN ARCHITECTURE CONTRACT v1**. Старый React,
Python и Rust/Tauri runtime сохранён только как donor/reference и не является dependency нового
runtime.

## Реализованный clean-runtime skeleton

Новый workspace `rewrite/` существует рядом с donor-реализацией и задаёт направленные зависимости:

`desktop UI -> application -> orchestration -> ledger contract <- persistence`.

Фактически реализовано:

- AutoCoder-owned versioned contracts `WorkspaceId`, `TaskId`, `EventId`, `IdempotencyKey`,
  `CreateTaskIntent`, `LedgerEvent` и первый `TaskCreated` payload;
- отдельный `OrchestrationCore` как единственный владелец перехода create-task;
- versioned lifecycle с рабочими переходами `created -> ready` и `ready <-> blocked`; terminal
  `completed` производится только отдельным orchestration-owned completion path после replay
  подтверждённого durable semantic-verification evidence; generic transition не может завершить
  task;
- first-class versioned `SemanticVerificationEvidence` со stable `EvidenceId`, outcome,
  verifier provenance и applicability basis, связывающим task, `TaskCreated` event, workspace и
  opaque `InputRevision`; это минимальная AutoCoder-owned reference boundary, а не фиктивная
  реализация Workspace subsystem;
- verification result сначала добавляется в Ledger отдельным immutable fact; completion выбирает
  evidence по identity и принимает решение только из replay сохранённого stream prefix, без
  повторной verification и обращений к clock/filesystem/provider/network;
- абстракция append-only `ExecutionLedger` с expected stream revision и idempotency key;
- SQLite-реализация Ledger: транзакционный optimistic append, уникальные event/idempotency keys,
  строгая проверка envelope, различение exact retry и conflicting identity reuse, упорядоченный
  replay и сохранение versioned event body;
- `ApplicationShell` как composition boundary без собственной task state machine и read-only
  query durable projection, полностью восстанавливаемой replay-ем task stream;
- отдельная минимальная Tauri desktop composition и статический UI, отправляющий только
  `create_task` intent и read-only `get_task` query через IPC; Ledger path выводится из Tauri
  `app_data_dir`, UI сохраняет pending logical-append identity до подтверждения create и успешной
  reconciliation projection (включая случай successful create + failed query) и отображает
  полученную durable projection, не вычисляя transitions;
- зарезервированные независимые boundaries Workspace, Provider Runtime, Process Supervisor и
  Diagnostics без фиктивной реализации или присвоения ими orchestration ownership.

Новый workspace не импортирует `src/`, `backend/` или `src-tauri/`, не содержит File/Terminal tools,
Monaco/Explorer, Ollama и legacy JSON orchestration snapshots.

## Проверенное поведение

`cargo test --manifest-path rewrite/Cargo.toml --workspace` подтверждает durable verified
completion и его projection после повторного открытия SQLite store; отказ при отсутствующем,
failed, mismatched/stale/inapplicable, conflicting или version-incompatible evidence/history;
запрет generic completion; exact retry без дублирования verification/completion; optimistic
fencing stale writer. Ранее реализованные Ledger guarantees для conflicting append identity reuse,
envelope validation и durable concurrency сохраняются.

Отдельный `node --test rewrite/ui/main.test.mjs` подтверждает UI regression-сценарий: successful
durable create, ошибка последующего projection query и безопасный retry с той же logical identity
без создания новой task identity.

`cargo check --manifest-path rewrite/Cargo.toml -p autocoder-desktop` также успешен после установки
доступных Linux WebKitGTK development libraries. Реальный интерактивный запуск webview в headless
контейнере и Windows packaged runtime по-прежнему не подтверждены.

## Существенные ограничения текущего slice

- Lifecycle пока намеренно не содержит attempts, execution authority и stop/resume. Текущий
  `InputRevision` является opaque stable reference: полноценные Workspace revisions/hashes и их
  смена появятся только с отдельным Workspace slice.
- Ledger пока хранит versioned JSON event body в SQLite, но ещё не реализует timestamps,
  orchestration-version migration и crash reconciliation.
- Workspace/Provider/Supervisor/Diagnostics пока являются только ownership boundaries.
- Windows packaged запуск новой desktop composition и durable запись из реального WebView не
  проверялись; это platform-specific acceptance risk, а не доказанная неисправность.
- Donor-компоненты не приняты в clean runtime.

## Следующий точный технический шаг

Добавить минимальный durable attempt/execution-authority contract в Orchestration Core, не
подключая пока provider/tool execution или physical process lifecycle. Stop/resume, capability
registry, Workspace Transaction и provider adapters должны по-прежнему появляться отдельными
последующими slices через clean boundaries, а не через legacy orchestration.

## Правило обновления этого файла

`PROJECT_STATE.md` хранит только текущее подтверждённое состояние, blockers и ближайший шаг.
Frozen target architecture хранится только в `PROJECT_MEMORY.md`; исторические donor-детали доступны
в Git history.
