# AutoCoder — PROJECT_STATE

## Дата состояния

4 сентября 2026.

## Текущий этап

**Второй vertical slice clean rewrite реализован: durable task lifecycle projection и явный
lifecycle новых задач.**

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
  `completed` сохранён как versioned state/event, но его production и успешная projection
  зарезервированы до появления orchestration-owned durable semantic-verification evidence, а
  generic transition и replay history без evidence явно отклоняются;
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

Автоматические Rust tests подтверждают replay lifecycle после повторного открытия SQLite store,
реализованные replay states, отказ для invalid transitions и неподтверждённого semantic completion
как при production, так и при replay, несовместимой версии, неполной/некорректной history,
read-only application/desktop query, exact idempotent retry, безопасный UI retry после successful
create + failed projection и
optimistic fencing конкурирующих lifecycle append. Ранее реализованные Ledger guarantees для
conflicting identity reuse, envelope validation и durable concurrency сохраняются.

`cargo check --manifest-path rewrite/Cargo.toml -p autocoder-desktop` также успешен после установки
доступных Linux WebKitGTK development libraries. Реальный интерактивный запуск webview в headless
контейнере и Windows packaged runtime по-прежнему не подтверждены.

## Существенные ограничения текущего slice

- Lifecycle пока намеренно ограничен состояниями `created`/`ready`/`blocked` и replay-контрактом
  `completed`; production `TaskCompleted`, attempts, execution authority, stop/resume и durable
  доказательство semantic completion отсутствуют.
- Ledger пока хранит versioned JSON event body в SQLite, но ещё не реализует timestamps,
  orchestration-version migration и crash reconciliation.
- Workspace/Provider/Supervisor/Diagnostics пока являются только ownership boundaries.
- Windows packaged запуск новой desktop composition и durable запись из реального WebView не
  проверялись; это platform-specific acceptance risk, а не доказанная неисправность.
- Donor-компоненты не приняты в clean runtime.

## Следующий точный технический шаг

Добавить минимальный durable semantic-verification contract и orchestration-owned completion path,
который только после подтверждённого evidence сможет производить зарезервированный
`TaskCompleted`. Provider/tool execution, stop/resume, physical process lifecycle, capability
registry, Workspace Transaction и provider adapter должны по-прежнему подключаться последующими
slices через новые boundaries, а не через legacy orchestration.

## Правило обновления этого файла

`PROJECT_STATE.md` хранит только текущее подтверждённое состояние, blockers и ближайший шаг.
Frozen target architecture хранится только в `PROJECT_MEMORY.md`; исторические donor-детали доступны
в Git history.
