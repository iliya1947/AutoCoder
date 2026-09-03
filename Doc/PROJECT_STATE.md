# AutoCoder — PROJECT_STATE

## Дата состояния

3 сентября 2026.

## Текущий этап

**Подготовка clean rewrite завершена; следующий этап — минимальный новый application/runtime
skeleton по `PROJECT_MEMORY.md`.**

`Doc/PROJECT_MEMORY.md` завершён и заморожен как **FROZEN ARCHITECTURE CONTRACT v1**. Он описывает
целевую архитектуру, но не состояние текущего runtime. В рамках обычной разработки и clean rewrite
этот файл не изменяется.

Краткий аудит фактического repository state подтвердил, что более 50% архитектурно значимой
реализации требует замены. Поэтому принято решение не мигрировать существующее приложение
постепенными архитектурными refactor-ами: новый AutoCoder будет создан как clean rewrite внутри
frozen boundaries. Текущая реализация остаётся на месте только как donor/reference implementation.
Перенос любого donor-механизма требует отдельной проверки его contract, ownership, platform
поведения и пригодности для новой архитектуры.

Rollback на старую orchestration architecture не планируется. Исторические acceptance-результаты
доказывают поведение старой реализации только в проверенных сценариях и не делают её фундаментом
нового runtime.

## Фактически проверенная текущая реализация

Инвентаризация выполнена по текущим исходникам React/TypeScript, Python и Rust/Tauri, а не по
предыдущему описанию состояния:

- UI собран вокруг `App` и компонентов `ProjectExplorer`, `Editor`, `ChatPanel`, `TerminalPanel`,
  `BackupDialog` и `WorkspaceHeader`;
- `ChatPanel` хранит и переводит lifecycle orchestration task, продолжает model turns, связывает
  tool results и сохраняет task через Tauri;
- TypeScript-модель orchestration ограничивает `ToolKind` вариантами `file | terminal`, содержит
  режимы `supervised | step_by_step`, фиксированные пределы model turns/actions и формирует JSON
  snapshot для backend;
- Python backend валидирует тот же closed-world набор File/Terminal actions, строит structured-output
  schema и принимает frontend orchestration snapshot как execution context;
- SQLite-слой Rust хранит orchestration task целиком как JSON и ключует workspace/history по строке
  filesystem project path; `workspace_state` представляет один `project_root`;
- Rust/Tauri содержит низкоуровневые операции с файлами, проверку путей, backup/restore,
  process supervision и отдельную Windows Job Object реализацию;
- frontend содержит Monaco integration, Project Explorer и локализацию на русском, английском и
  иврите;
- Python provider содержит Ollama HTTP adapter, structured output, model discovery и readiness/start
  logic.

Это описание фиксирует наличие кода, а не подтверждает его пригодность для переноса без изменений.

## Инвентаризация для clean rewrite

### SALVAGE — потенциальные donors после отдельной проверки

Ни один пункт пока не переносится и не считается частью нового AutoCoder:

1. **Windows process lifecycle / Job Objects** (`src-tauri/src/process_lifecycle.rs`) — полезны
   suspended-start, assignment в Job, kill-on-close и termination primitives. Перед переносом нужны
   отдельные Windows tests и привязка к единственному владельцу physical child-process lifecycle —
   новому Rust supervisor.
2. **Низкоуровневые filesystem и backup primitives** (`src-tauri/src/lib.rs`) — path validation,
   checked/atomic replacement, backup metadata, safe directory copy и restore concurrency checks.
   Их следует извлекать только за новым Workspace Transaction contract, не перенося старую
   project-path identity и Tauri-command ownership.
3. **Editor / Monaco integration** (`src/components/Editor.tsx`, `src/monaco.ts`) — model lifecycle,
   selection и dirty-buffer handling могут быть переиспользованы после проверки нового UI contract.
4. **Project Explorer UI** (`src/components/ProjectExplorer.tsx`, `src/utils/projectTree.ts`) —
   rendering/tree utilities являются возможным UI donor, но старое single-root/project-path state
   не переносится.
5. **Localization** (`src/hooks/useTranslation.ts`, `src/locales/*.json`) — translation mechanism и
   существующие строки можно проверить и адаптировать к новому application shell.
6. **Части Ollama/provider HTTP и readiness logic** (`backend/provider.py`) — HTTP encoding/error
   handling, local readiness и model presence checks могут стать деталями нового сменного provider
   adapter. Provider не получает ownership общей task state machine или process lifecycle.

### REWRITE — несовместимо с frozen architecture

1. **Frontend-owned orchestration state machine** (`src/types/orchestration.ts`) и управление ею из
   `ChatPanel`: целевой владелец task transitions — Orchestration Core, UI отправляет intents и
   отображает projection.
2. **`ChatPanel` как владелец task lifecycle** (`src/components/ChatPanel.tsx`), включая model-loop,
   approval/result transitions, recovery и persistence sequencing.
3. **Closed-world `file | terminal` tool contract** в TypeScript/Python (`src/types/orchestration.ts`,
   `backend/tool_contracts.py`, связанные ветви `backend/main.py`): новый capability space должен
   динамически расширяться и оставаться за AutoCoder-owned contracts/registry.
4. **Durable orchestration как JSON snapshots** (`src-tauri/src/history.rs` и frontend/backend
   snapshot exchange): новый runtime требует durable Execution Ledger, versioned transitions,
   attempts, facts, idempotency/fencing и replay/reconciliation semantics.
5. **Filesystem path как Workspace identity** и single-root `workspace_state` (`src-tauri/src/history.rs`,
   связанное состояние `App`): новый Workspace имеет стабильную identity и поддерживает несколько
   roots/resources без сведения identity к path.
6. **Смешанные ownership boundaries** текущего `App`/`ChatPanel`/Python/Tauri пути: orchestration,
   provider semantics, process supervision, persistence и workspace mutation должны быть разведены
   по владельцам frozen architecture.
7. **Текущую backend policy/requirement compilation** и semantic completion поверх File/Terminal
   actions: она привязана к старой модели snapshot и closed-world tools, а не к новому durable core,
   capability/effect/policy contracts и evidence model.

### OBSOLETE — не переносить в новую реализацию

1. Autonomy semantics `supervised | step_by_step` и соответствующий UI selector. Новая policy model
   должна выражать authority/approval/autonomy без сохранения этих исторических режимов.
2. Hardcoded execution ceilings `maxModelTurns = 12` и `maxActions = 8`, включая блокировку task при
   их достижении. Limits/budgets должны быть policy/settings values, AI-managed или unlimited там,
   где нет реального технического ограничения.
3. Compatibility с orchestration task JSON старого runtime как целевая recovery/migration model.
   Старые snapshots остаются только историческими данными donor-приложения; новый skeleton не должен
   строить durable semantics вокруг них.
4. Имена fenced File/Terminal control contracts и assumption, что executable capability заранее
   обязана входить в этот конечный список.
5. Старый staged-MVP/acceptance roadmap как план развития новой реализации. Его подтверждённые
   результаты остаются historical evidence для donor-механизмов, но не задают порядок clean rewrite.

## Решение и границы подготовительного шага

- Существующий runtime-код не удалён и не перемещён.
- Новый application skeleton на этом шаге не создан.
- Dependencies и lockfiles не изменены.
- Массовый refactor не выполнялся.
- `Doc/PROJECT_MEMORY.md` не изменён.
- Изменение является только фиксацией нового фактического project state и инвентаризации donors.

## Что не подтверждено

- Ни один `SALVAGE`-компонент ещё не принят в новую реализацию.
- Новый Orchestration Core, Execution Ledger, Workspace identity/transactions, contracts, IPC и
  application/runtime composition ещё не реализованы.
- Windows-specific donor behavior в контексте будущего skeleton на этом шаге не перепроверялось.
- Clean rewrite пока не имеет runnable application milestone.

## Следующий точный технический шаг

Создать **минимальный новый application/runtime skeleton** рядом со старой donor-реализацией, не
подключая к нему legacy orchestration. В первом vertical slice нужно:

1. зафиксировать AutoCoder-owned границы модулей и направленные зависимости для Application Shell,
   Orchestration Core, Execution Ledger, Workspace, Provider Runtime, Rust Process Supervisor,
   Persistence и Diagnostics;
2. определить минимальные versioned contracts для `WorkspaceId`, `TaskId`, intent/command и
   append-only ledger event с optimistic append/idempotency полями;
3. запустить пустой desktop/runtime composition и доказать одним contract test, что UI может создать
   task intent, а durable transition выполняет Orchestration Core и записывает Ledger — без
   File/Terminal tool implementation, Monaco/Explorer donors и Ollama integration;
4. держать legacy runtime изолированным как reference, без adapter-а, который возвращает ownership
   lifecycle в `ChatPanel` или filesystem path.

После этого donors следует подключать отдельными проверяемыми slices только через новые boundaries.

## Правило обновления этого файла

`PROJECT_STATE.md` хранит текущее подтверждённое состояние, проверки, blockers и ближайший шаг.
Исторические подробности старой реализации доступны в Git history и не должны затенять актуальный
clean-rewrite milestone. Frozen target architecture хранится только в `PROJECT_MEMORY.md`.
