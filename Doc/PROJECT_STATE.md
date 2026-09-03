# AutoCoder — PROJECT_STATE

## Дата состояния

3 сентября 2026.

## Текущий этап

**Первый vertical slice clean rewrite реализован; следующий этап — durable task projection и
явный lifecycle новых задач.**

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
- абстракция append-only `ExecutionLedger` с expected stream revision и idempotency key;
- SQLite-реализация Ledger: транзакционный optimistic append, уникальные event/idempotency keys,
  строгая проверка envelope, различение exact retry и conflicting identity reuse, упорядоченный
  replay и сохранение versioned event body;
- `ApplicationShell` как composition boundary без собственной task state machine;
- отдельная минимальная Tauri desktop composition и статический UI, отправляющий только
  `create_task` intent через IPC; Ledger path выводится из Tauri `app_data_dir`, а UI сохраняет
  pending logical-append identity до подтверждения результата;
- зарезервированные независимые boundaries Workspace, Provider Runtime, Process Supervisor и
  Diagnostics без фиктивной реализации или присвоения ими orchestration ownership.

Новый workspace не импортирует `src/`, `backend/` или `src-tauri/`, не содержит File/Terminal tools,
Monaco/Explorer, Ollama и legacy JSON orchestration snapshots.

## Проверенное поведение

Команда `cargo test --manifest-path rewrite/Cargo.toml --workspace` успешна: одиннадцать tests
подтверждают durable SQLite replay после повторного открытия, Tauri app-data path composition, exact
idempotent retry, отказ при conflicting reuse event/idempotency identity, stale append,
несогласованном envelope, невалидном десериализованном identifier и повторном create transition.

`cargo check --manifest-path rewrite/Cargo.toml -p autocoder-desktop` также успешен после установки
доступных Linux WebKitGTK development libraries. Реальный интерактивный запуск webview в headless
контейнере и Windows packaged runtime по-прежнему не подтверждены.

## Существенные ограничения текущего slice

- Реализован только переход создания task; полноценная task state machine, attempts, execution
  authority, stop/resume/reconciliation и semantic completion отсутствуют.
- Ledger пока хранит versioned JSON event body в SQLite, но ещё не реализует timestamps,
  orchestration-version migration и crash reconciliation.
- Workspace/Provider/Supervisor/Diagnostics пока являются только ownership boundaries.
- Windows packaged запуск новой desktop composition и durable запись из реального WebView не
  проверялись; это platform-specific acceptance risk, а не доказанная неисправность.
- Donor-компоненты не приняты в clean runtime.

## Следующий точный технический шаг

Добавить внутри clean workspace минимальный **durable task lifecycle projection**:

1. определить versioned состояния и события как минимум для created/ready/blocked/completed без
   tool/provider-specific semantics;
2. восстанавливать task projection только replay-ем Ledger и проверять допустимость перехода в
   Orchestration Core;
3. добавить read-only query через Application Shell/desktop IPC, чтобы UI отображал durable state,
   не вычисляя business transitions;
4. покрыть restart/replay, invalid transition, optimistic concurrency и idempotent retry contract
   tests.

Physical process lifecycle, capability registry, Workspace Transaction и provider adapter должны
подключаться последующими slices через новые boundaries, а не через legacy orchestration.

## Правило обновления этого файла

`PROJECT_STATE.md` хранит только текущее подтверждённое состояние, blockers и ближайший шаг.
Frozen target architecture хранится только в `PROJECT_MEMORY.md`; исторические donor-детали доступны
в Git history.
