# AutoCoder — PROJECT_MEMORY

## 1. Назначение проекта

AutoCoder — AI-first desktop-среда разработки для Windows, которая должна автономно доводить программную задачу до проверенного рабочего результата.

Центральный объект AutoCoder — не редактор и не чат, а **задача пользователя**. Система должна понимать цель, анализировать проект, получать релевантный контекст, выбирать модели и инструменты, изменять workspace, запускать команды и проверки, наблюдать фактический результат, исправлять ошибки и завершать задачу только тогда, когда выполнение подтверждено фактами.

Целевой цикл:

Пользователь
→ задача / намерение
→ AutoCoder Orchestration Core
→ анализ проекта и получение контекста
→ выбор модели / совета моделей и инструментов
→ предложение / выполнение действий согласно политике автономности
→ изменение workspace
→ запуск команд, тестов и других инструментов
→ получение фактических результатов
→ проверка достижения цели
→ исправление ошибок при необходимости
→ подтверждённый рабочий результат.

AutoCoder не должен превращаться в IDE с прикрученным чат-ботом. Редактор, Terminal, Git, браузер, Docker, LSP, COMSOL и другие будущие возможности являются инструментами или подсистемами вокруг общего AI-оркестратора.

Проект личный, но архитектура должна допускать дальнейшее развитие в коммерческий продукт.

---

## 2. Базовые свойства конечного продукта

AutoCoder должен быть:

- AI-first;
- offline-first;
- Windows-first;
- автономным и пользовательски настраиваемым;
- расширяемым без переписывания ядра под каждый новый инструмент или модель;
- фактически наблюдаемым и диагностируемым;
- сохраняющим целостность пользовательского проекта и позволяющим откатить собственные изменения;
- устойчивым к перезапускам, сбоям, отмене задач и поздним результатам;
- способным объяснить, какие действия были выполнены и на основании каких фактов задача считается завершённой.

AutoCoder является универсальным инструментом разработки. Архитектура проекта не должна вводить искусственные ограничения на то, какие обычные программные проекты пользователь может создавать. Permissions, confirmations, diagnostics, process ownership и backup существуют для управления действиями самого AutoCoder и целостности workspace, а не как content-based ограничения на назначение создаваемого ПО.

Автоматизация должна уменьшать ручную работу пользователя, а не переносить на него необходимость вручную восстанавливать внутреннее состояние системы.

---

## 3. Целевая архитектура

Целевая архитектура AutoCoder строится из отдельных слоёв с чёткой ответственностью:

1. UI.
2. Orchestration Core.
3. Execution Ledger.
4. Project Intelligence.
5. Provider Runtime.
6. Model Council / Multi-Model Deliberation.
7. Tool / Capability Runtime.
8. Workspace ChangeSet / Transaction.
9. State / Persistence.
10. Runtime Supervisor.
11. Diagnostics / Introspection Plane.
12. System Model / Self-Model.

Все слои опираются на общие основы:

- schema-first versioned internal protocol;
- stable IDs;
- явное process ownership;
- capability / permission model;
- structured errors;
- versioned persistence schema;
- restart-safe semantics.

### 3.1. UI

UI отображает фактическое состояние системы и передаёт пользовательские intents.

UI не должен быть владельцем orchestration state machine.

Основные пользовательские intents:

- начать задачу;
- одобрить или отклонить действие;
- продолжить остановленное ожидание;
- остановить задачу;
- открыть / изменить проект;
- работать с редактором и интерактивными инструментами;
- просматривать изменения, историю, diagnostics и результаты;
- настраивать providers, council profiles, autonomy, hardware/scheduler policies и research policy.

UI может содержать локальное представление состояния для рендеринга, но источник истины для жизненного цикла автономной задачи должен находиться в Orchestration Core.

### 3.2. Orchestration Core

Orchestration Core — единственный логический владелец жизненного цикла AI-задачи.

Он отвечает за:

- immutable исходную цель пользователя;
- декомпозицию и семантику требований;
- текущее состояние задачи;
- допустимые переходы state machine;
- выбор следующего шага;
- связь model turn ↔ action ↔ factual result;
- уровни автономности и approval policy;
- execution budgets;
- completion / blocked / stopped semantics;
- restart-safe continuation;
- защиту от поздних или устаревших результатов;
- координацию single-model и multi-model execution.

Frontend, provider, tools и persistence не должны независимо решать, какое состояние orchestration task является текущим или какой переход допустим.

### 3.3. Execution Ledger

Жизненный цикл orchestration должен храниться как **append-only последовательность фактов**, а не только как постоянно перезаписываемый JSON snapshot.

Примеры событий:

- TaskStarted;
- RequirementCompiled;
- ModelTurnStarted;
- ModelDecisionReceived;
- CouncilRoundStarted;
- ProposalRecorded;
- CritiqueRecorded;
- ResearchRequested;
- EvidenceRecorded;
- PositionRevised;
- CouncilRoundCompleted;
- CaptainSelected;
- ActionProposed;
- ActionApproved / ActionDeclined;
- ToolStarted;
- ToolCompleted / ToolFailed / ToolInterrupted;
- WorkspaceChanged;
- ReconciliationCompleted;
- RequirementSatisfied;
- TaskBlocked;
- TaskStopped;
- TaskCompleted.

Текущее состояние задачи вычисляется из последовательности событий. Snapshot может использоваться как cache для ускорения загрузки, но не должен заменять фактическую историю событий.

Event sourcing не требуется распространять на всё приложение. Он нужен прежде всего там, где критична доказуемая причинность: orchestration, execution lifecycle, recovery и replay.

### 3.4. Project Intelligence

Project Intelligence — отдельный слой понимания пользовательского проекта.

Он должен со временем уметь:

- строить структуру проекта;
- искать файлы и текст;
- читать релевантные части проекта;
- понимать символы и зависимости;
- учитывать открытый файл и выделение;
- находить тесты и связанные файлы;
- учитывать фактические изменения workspace;
- использовать semantic retrieval и специализированные knowledge sources;
- подготавливать ограниченный, релевантный контекст для модели.

Контекст модели не должен бесконтрольно собираться внутри UI-компонента или одного prompt builder.

COMSOL Knowledge Engine в будущем должен использовать общий Project Intelligence / retrieval слой, а не создавать параллельную архитектуру.

### 3.5. Provider Runtime

Модели подключаются через provider abstraction.

Provider Runtime отвечает за:

- взаимодействие с локальными и облачными AI-провайдерами;
- model selection;
- capability discovery / negotiation;
- generation settings;
- structured outputs;
- native tool calling, если конкретный provider/model это поддерживает;
- streaming, reasoning/thinking, vision и другие возможности моделей, если доступны;
- model/provider metadata;
- timeout / retry policy;
- безопасную работу с credentials.

Provider не должен определять orchestration state machine.

Модель не является источником истины о фактическом выполнении инструментов. Она предлагает решения, а backend независимо проверяет их и сопоставляет с доступными capability contracts.

Provider Runtime должен поддерживать как локальные LLM, так и API-модели. Orchestration Core не должен быть архитектурно привязан к Ollama или любому конкретному API.

---

## 4. Model Council / Multi-Model Deliberation

AutoCoder должен поддерживать не только выбор одной модели, но и **совместную работу произвольного количества локальных LLM и облачных API-моделей над одной задачей**.

Это одна из ключевых возможностей конечного продукта.

Council Engine не должен иметь искусственного архитектурного лимита на количество участников. Практические пределы определяются только:

- доступной RAM / VRAM;
- скоростью локального железа;
- ограничениями providers;
- API rate limits;
- стоимостью API;
- размером контекста;
- выбранными пользователем настройками параллелизма и deliberation.

AutoCoder должен предупреждать пользователя о потенциальной нагрузке в месте настройки Council Profile, но не вводить искусственный hard limit на число моделей.

### 4.1. Участники совета

Каждый участник совета должен иметь собственную конфигурацию.

Как минимум пользователь должен иметь возможность независимо выбрать для каждого участника:

- provider;
- model;
- role;
- отдельный custom user prompt;
- enabled / disabled state;
- при необходимости дополнительные model/provider settings;
- необязательный weight / priority.

Одинаковая модель может участвовать в совете несколько раз с разными ролями и разными пользовательскими prompt-инструкциями.

Точная внутренняя семантика сущности `Participant` должна быть отдельно спроектирована после фиксации целевой архитектуры и повторного аудита проекта. Не следует преждевременно закреплять, что `Participant` навсегда равен одному provider/model instance.

### 4.2. Раунды обсуждения

Пользователь задаёт **максимальное число deliberation rounds**.

Это верхняя граница, а не обязательное количество раундов. Если участники достигли выбранного критерия консенсуса раньше, обсуждение заканчивается раньше.

Критерий консенсуса должен быть настраиваемым. Система должна позволять использовать:

- полное совпадение решения;
- совпадение ключевого плана;
- заданный процент согласия;
- отсутствие существенных возражений;
- комбинации этих критериев.

Если используется пороговый, а не полный консенсус, AutoCoder обязан явно показать пользователю оставшиеся различия между позициями. Нельзя представлять частичное согласие как полное единодушие.

### 4.3. Базовый deliberation cycle

Базовый цикл должен поддерживать:

proposal
→ critique
→ factual verification / research при необходимости
→ revised position
→ comparison
→ consensus or next round.

Модели должны иметь возможность критиковать предложения друг друга, пересматривать собственную позицию после новых аргументов и фактов и продолжать обсуждение до достижения критерия консенсуса либо лимита раундов.

Deliberation не является отдельным чат-шоу. Его результат должен быть связан с фактической задачей, Project Intelligence, tools, tests, execution results и другими проверяемыми источниками системы.

### 4.4. Internet Research внутри раунда

В deliberation должна быть возможность фактической проверки утверждений через интернет.

Интернет-поиск не должен быть скрытой привилегией конкретной облачной модели. Он должен быть доступен Council Engine как отдельная research capability / tool, чтобы локальные и облачные модели могли опираться на общие проверяемые evidence.

Пользовательская политика должна позволять настраивать, когда интернет-проверка разрешена или требуется, например:

- полностью отключена;
- по запросу участников;
- разрешена в каждом раунде;
- обязательна для проверяемых внешних утверждений перед финальным решением.

Research results должны сохранять фактические источники и время получения, чтобы участники могли ссылаться на одни и те же evidence, а Diagnostics могла восстановить причинную цепочку решения.

### 4.5. Команды и масштабирование совета

Для больших Council Profiles должна поддерживаться настраиваемая topology.

Минимально архитектура должна позволять:

- Flat Council — все участники обсуждают в одном совете;
- Team Council — участники разбиваются на несколько команд;
- Hierarchical Council — команды выбирают победителей/капитанов, которые переходят на следующий уровень;
- будущие экспериментальные topologies без переписывания Council Engine.

Пользователь должен иметь возможность задавать размер и количество команд, например:

- 2 команды × 25 моделей;
- 10 команд × 5 моделей;
- другие конфигурации в пределах доступных ресурсов.

Количество логических участников не должно означать одновременную загрузку всех локальных моделей. Runtime Scheduler должен позволять выполнять участников последовательно или ограниченными параллельными группами согласно настройкам железа.

Настройки scheduler/hardware должны позволять адаптировать одну и ту же логическую конфигурацию совета под различное железо. Архитектура должна допускать настройки вроде:

- maximum parallel local model executions;
- maximum simultaneously loaded models;
- RAM / VRAM budget;
- maximum concurrent API calls;
- context budget per participant;
- team size / team count;
- пользовательские performance profiles.

Нагрузка должна быть объяснена пользователю предупреждением в месте конфигурации Council Profile, а не искусственным запретом.

### 4.6. Капитаны и капитанские раунды

В Team / Hierarchical Council после завершения командного этапа должен определяться победитель/капитан команды.

Базовый принцип отбора: после общей критики, проверок и пересмотра позиций предпочтение получает участник, **чья смысловая позиция изменилась меньше всего и к чьему итоговому выводу в результате пришли остальные**.

Это не должно сводиться к простой текстовой похожести. Точный алгоритм Position Stability Analysis проектируется отдельно после фиксации PROJECT_MEMORY и повторного аудита проекта.

Капитаны переходят на следующий уровень и проходят **тот же общий принцип deliberation**, а не отдельную непрозрачную judge-логику.

Пользователь должен отдельно настраивать **максимальное количество captain rounds**. Как и обычные rounds, это верхняя граница: если капитаны достигли выбранного критерия консенсуса раньше, капитанский этап завершается раньше.

Финальный победитель капитанского уровня становится итоговым победителем совета для соответствующего решения.

Отдельный обязательный Judge-модуль не является фундаментальным требованием архитектуры.

### 4.7. Position Stability Analysis

AutoCoder должен уметь анализировать, насколько позиция участника изменилась между раундами после критики, evidence и ответов других участников.

Это нужно для:

- выбора устойчивых предложений;
- определения победителей/капитанов;
- анализа convergence;
- объяснения пользователю, как совет пришёл к результату.

Стабильность позиции не должна автоматически означать правильность. Она является одним из сигналов совместно с evidence, factual verification, результатами инструментов и convergence других участников.

Точная модель `Position`, алгоритм смыслового сравнения и weighting этих сигналов остаются отдельной архитектурной задачей после повторного аудита проекта.

### 4.8. Передача информации между командами и уровнями

Это отдельная важная архитектурная задача.

Нужно отдельно спроектировать сложную настраиваемую схему передачи:

- proposals;
- critiques;
- factual evidence;
- unresolved disagreements;
- team results;
- positions;
- captain-level context;
- ссылок на исходные ответы.

Цель — сохранять фактические аргументы и причинность, не заставляя большие советы бесконтрольно дублировать полный сырой контекст всех участников.

Эта схема должна проектироваться **после PROJECT_MEMORY и повторного аудита фактического проекта**, а не фиксироваться преждевременно.

### 4.9. Экспериментальная topology: Rotating / Overlapping Teams

Сохранить как экспериментальную идею, не как обязательный конечный алгоритм.

Пример конфигурации:

`10 команд × 5 моделей + N сдвигов состава`

После командного раунда составы смещаются, создавая перекрывающиеся группы. Участники переносят аргументы и evidence между разными группами, а устойчивость их позиций можно оценивать после взаимодействия с различными наборами оппонентов.

Пример:

Shift 0:
[A B C D E] [F G H I J] ...

Shift 1:
[B C D E F] [G H I J K] ...

Shift 2:
[C D E F G] [H I J K L] ...

Количество сдвигов должно быть настраиваемым, если эта topology будет подтверждена практическими экспериментами.

До исследований нельзя считать её обязательной частью MVP или основной topology.

### 4.10. Council Profiles

Пользователь должен иметь возможность сохранять reusable Council Profiles.

Профиль может включать:

- список участников;
- provider/model каждого участника;
- role;
- custom prompt каждого участника;
- max deliberation rounds;
- max captain rounds;
- consensus policy;
- topology;
- teams configuration;
- internet research policy;
- scheduler / hardware policy;
- cost/token/API budget;
- дополнительные экспериментальные параметры.

AutoCoder может поставлять предустановленные профили вроде Fast / Balanced / Deep Review, но пользовательские профили не должны быть ограничены ими.

---

## 5. Tool / Capability Runtime

Инструменты должны регистрироваться через общую Capability Registry / Tool Manifest, а не через набор разрозненных условных конструкций.

Tool Manifest должен со временем описывать:

- stable id;
- operations;
- input schema;
- result schema;
- execution backend;
- approval / risk policy;
- diagnostics category;
- capability metadata;
- version.

Orchestration Core спрашивает registry, какие capabilities реально доступны в текущем runtime, вместо hardcoded предположений.

File и Terminal являются первыми реализациями Tool Runtime, но не определяют архитектуру всех будущих инструментов.

---

## 6. Workspace ChangeSet / Transaction

AI должна изменять **workspace**, а редактор должен отображать workspace.

Целевая модель:

AI / Orchestration
→ Workspace ChangeSet
→ Diff / Preview
→ Approval policy
→ Workspace Transaction
→ backup
→ apply
→ verify
→ rollback при необходимости
→ editor/project reconciliation.

ChangeSet должен поддерживать как минимум create / modify / delete и в будущем multi-file atomic/bounded transactions.

Нельзя считать Monaco/editor buffer конечным execution backend для фактических AI-изменений. Terminal, compiler и другие инструменты должны видеть тот же фактический workspace, который AutoCoder считает изменённым.

Git может использоваться как дополнительный механизм diff/history, но не как обязательная зависимость backup/rollback.

---

## 7. Workspace identity

Нужен first-class stable WorkspaceId / ProjectSessionId.

Display name проекта не является identity.

Workspace identity должен проходить через:

- UI;
- Orchestration;
- Tool Runtime;
- Workspace Transaction;
- Persistence;
- Diagnostics;
- Backend Runtime;
- Execution Ledger.

Канонический filesystem root и stable id должны защищать от логических коллизий между проектами с одинаковыми именами или похожей структурой.

---

## 8. Runtime Supervisor и backend process model

Целевой runtime — Rust-supervised long-lived AutoCoder Backend Runtime.

Rust/Tauri владеет физическим lifecycle AutoCoder-owned child processes:

- запуск;
- health supervision;
- restart после crash;
- task-scoped cancellation, где это технически возможно;
- shutdown;
- process-tree cleanup.

Backend Runtime должен быть долгоживущим сервисом, а не обязательно новым Python interpreter на каждый model turn.

Это позволит естественно поддерживать:

- Project Intelligence indexes;
- embeddings/retrieval;
- provider pools;
- tool registry;
- diagnostics context;
- caches;
- background analysis;
- LSP/service integrations;
- multi-agent / Model Council contexts.

Переход на long-lived backend не должен уничтожать существующую fault isolation. Она должна обеспечиваться supervision/restart и protocol boundaries.

### 8.1. Ollama lifecycle

Нужно сохранять принцип **один ресурс — один физический lifecycle owner**.

Для desktop runtime:

- Rust supervisor владеет запуском/завершением AutoCoder-owned Ollama process;
- Provider Runtime отвечает за provider semantics: endpoint, readiness, models, capabilities, request/response metadata и ошибки.

Текущая Python-side логика запуска Ollama может существовать как compatibility/standalone fallback во время миграции, но после перехода на long-lived supervised backend не должна случайно превращаться во второго независимого владельца одного и того же desktop process lifecycle.

Это не отменяет поддержку разных local/API providers. Наоборот, Ollama должен стать одним provider adapter среди многих.

---

## 9. Schema-first internal protocol

Ключевые cross-layer contracts должны быть versioned и schema-first.

В частности:

- task state/events;
- actions/results;
- tool manifests;
- provider capabilities;
- provider responses;
- workspace changesets/results;
- structured errors;
- diagnostics events;
- backend requests/responses.

TypeScript, Rust и Python representations должны либо генерироваться из общего контракта, либо валидироваться против одной канонической схемы.

До выполнения первой задачи long-lived backend и frontend/Tauri должны выполнять protocol handshake с данными вроде:

- protocolVersion;
- backendVersion;
- supportedCapabilities;
- toolContractVersion;
- diagnosticsProtocolVersion.

Несовместимые версии должны обнаруживаться до начала автономного выполнения, а не через случайную ошибку отсутствующего поля в середине задачи.

---

## 10. Provider capabilities и Model Execution Profile

Provider/model capabilities должны быть first-class data.

Примеры capabilities:

- structured outputs;
- native tools;
- streaming;
- reasoning/thinking;
- vision;
- context window;
- parallel tool calls;
- usage telemetry.

Orchestration Core / Council Engine выбирает стратегию на основании фактических capabilities, а не предполагает одинаковое поведение всех моделей.

Нужен явный Model Execution Profile, в который могут входить:

- temperature;
- context/token limits;
- timeouts;
- structured-output strategy;
- retry policy;
- required provider capabilities.

Diagnostics должна фиксировать фактически применённый execution profile.

Provider envelope должен по возможности сохранять полезные metadata ответа: модель, finish/done reason, timings, token/eval counts и другие доступные provider metrics.

---

## 11. State / Persistence

SQLite остаётся базовым локальным persistent store.

Нужна versioned schema / migrations. Изменения структуры БД не должны зависеть от неявного совпадения версии приложения и старой базы.

Persistence отвечает за надёжное хранение данных, но не принимает orchestration business decisions.

Необходимо различать как минимум:

- durable execution facts;
- snapshots/cache;
- chat/history;
- settings/profiles;
- workspace metadata;
- diagnostics retention;
- schema version.

---

## 12. Structured errors

Ошибки между подсистемами должны эволюционировать от плоских строк к structured error contract.

Минимально полезная модель должна позволять передавать:

- error id / code;
- component;
- operation;
- cause/category;
- technical details;
- user-facing message;
- correlation/task/action context;
- severity;
- recoverability.

Diagnostics должна различать как минимум:

- EXPECTED_ABSENCE;
- FAILED_BUT_RECOVERED;
- FAILED_FATAL.

Graceful degradation в UI не должна означать потерю факта внутренней ошибки.

---

## 13. Diagnostics / Introspection Plane

Diagnostics — фундаментальная platform capability, а не локальная отладка отдельного бага.

Цель: по одному run можно восстановить причинную цепочку от пользовательского intent до фактического результата.

Пример цепочки:

Run
→ UI operation
→ orchestration task
→ model/council turn
→ provider request
→ provider response
→ decision
→ validator
→ approval
→ tool execution
→ OS operation
→ filesystem mutation
→ reconciliation
→ persisted state.

### 13.1. Runtime Trace

Diagnostics должна использовать trace/span/context-like модель причинности, а не только текстовый log.

Каждое существенное событие должно иметь stable correlation identifiers, чтобы можно было связать cross-process и cross-language цепочку.

### 13.2. Architecture Inventory

Diagnostics должна автоматически строить фактический inventory компонентов и boundaries насколько это возможно из реальной архитектуры:

- frontend modules / operations;
- Tauri commands;
- Rust modules/services;
- Python services/modules;
- providers;
- tools/capabilities;
- stores;
- spawned processes;
- IPC boundaries;
- protocols/versions.

Нельзя полагаться только на ручное правило «не забудь зарегистрировать новый модуль в diagnostics».

### 13.3. Runtime Discovery

Runtime traces должны автоматически обнаруживать реально участвующие:

- processes;
- commands;
- providers;
- tools;
- IPC calls;
- model turns;
- database operations;
- workspace operations;
- task transitions.

### 13.4. Coverage Audit

Coverage Auditor сравнивает:

**что существует**
с
**что реально наблюдается diagnostics**.

Он должен находить blind spots и непокрытые boundaries.

Новый Tauri command, Tool, Provider, spawned process или другой значимый boundary не должен бесшумно появляться вне наблюдаемой архитектуры.

В CI/build должен существовать Diagnostics Coverage Gate, который способен обнаруживать новые непокрытые компоненты/границы и заставлять разработчика осознанно решить, как они наблюдаются.

### 13.5. Replay

Diagnostics должна позволять воспроизводить captured causal chain без повторного недетерминированного model call.

Особенно важно уметь повторно прогнать сохранённый provider/model response через:

- parsing;
- validation;
- orchestration transition logic;
- reconciliation logic.

Replay не должен автоматически повторять destructive tool side effects.

### 13.6. Diagnostics UI / Bundle

Development diagnostics UI должна позволять:

- видеть chronological timeline;
- фильтровать по subsystem/category/severity;
- искать события;
- раскрывать structured event details;
- переходить по parent/child correlation chain;
- находить errors/rejections;
- видеть предшествующие и последующие связанные события;
- копировать событие;
- экспортировать local diagnostic bundle.

Diagnostic bundle должен содержать достаточно безопасной информации, чтобы другой разработчик или AI мог восстановить проблему без ручного сбора десятков скриншотов и сообщений.

### 13.7. AI Diagnostics Review

После появления надёжного System Model, coverage audit и runtime traces AutoCoder должен получить отдельный AI Diagnostics / Architecture Review.

LLM должна анализировать **достоверные диагностические данные**, а не угадывать архитектуру.

Ей передаются по необходимости:

- architecture/system map;
- component inventory;
- coverage gaps;
- relevant traces;
- structured errors;
- protocol/capability information.

Цель — находить blind spots, неправильные boundaries, новые непокрытые компоненты, архитектурные расхождения и подозрительные causal chains.

Сначала строится рабочая diagnostics infrastructure; AI review является слоем поверх неё.

### 13.8. Diagnostics invariants

Diagnostics должна быть пассивным наблюдателем:

- её failure не ломает основную работу приложения;
- она не должна менять business semantics;
- не должна скрывать race conditions изменением порядка выполнения;
- должна быть concurrency/process safe;
- payloads должны быть bounded;
- нужен retention/rotation/cleanup;
- нужен centralized redaction/sanitization;
- предыдущие diagnostic runs должны сохраняться достаточно долго для расследования restart/startup проблем;
- никаких автоматических внешних telemetry uploads;
- работа локальная/offline.

---

## 14. System Model / Self-Model

AutoCoder должен иметь machine-readable factual System Model собственного устройства.

Он должен описывать по мере развития:

- subsystems;
- tools;
- providers;
- boundaries;
- stores;
- processes;
- capabilities;
- protocol versions;
- diagnostics coverage.

System Model должен строиться из фактических registries, runtime discovery, protocols и других проверяемых источников, а не вручную поддерживаемого списка, который легко забыть обновить.

В будущем AI Diagnostics Review сможет использовать System Model для анализа самого AutoCoder.

---

## 15. Permissions, credentials и технические границы

Технические permissions должны управлять тем, какие реальные ресурсы и capabilities доступны конкретному runtime/action. Они не должны превращаться в искусственные ограничения на назначение создаваемого пользователем программного обеспечения.

Нужны два разных уровня:

1. application / OS capability permissions;
2. AutoCoder AI execution authorization / approval policy.

Факт, что UI технически может вызвать command, не означает, что автономная AI-задача автоматически имеет право выполнить это действие.

Для credentials нужен Secret Store abstraction.

`.env` допустим как development override, но не должен считаться конечным production-хранилищем API keys/tokens.

Diagnostics обязана централизованно редактировать credentials, tokens и другие secrets до записи/export.

---

## 16. Offline-first

AutoCoder после установки должен полноценно запускаться и выполнять локальные функции без интернета.

Локально поставляются:

- UI;
- Monaco Editor и workers;
- JavaScript/CSS;
- шрифты и иконки;
- локализации;
- Python backend runtime;
- Tauri runtime resources;
- обязательные вспомогательные библиотеки.

Runtime-загрузки этих компонентов из интернета запрещены.

Исключение — явно выбранные пользователем сетевые AI/API providers и другие внешние capabilities, для которых сеть является их явной функцией.

Локальный Ollama должен работать без интернета после установки требуемой модели.

Build-time установки npm/cargo/pip не являются runtime dependency установленного приложения.

---

## 17. AI autonomy semantics

AutoCoder должен эволюционировать от режима подтверждений к более высокой автономности без перестройки ядра.

Уровень автономности является policy, а не отдельной архитектурой.

При любой policy должны сохраняться базовые инварианты:

- model proposal не является фактическим результатом;
- success tool execution не обязательно означает semantic completion;
- factual result должен быть связан с конкретным action/execution id;
- user constraints остаются частью исходной задачи;
- completion требует фактических доказательств;
- cancelled/stopped/blocked/completed — разные состояния;
- restart не должен незаметно повторять действие с неизвестным результатом;
- late result не может незаконно изменить уже терминальное состояние задачи.

---

## 18. Мультиязычность

Мультиязычность учитывается архитектурно с начала проекта.

UI-строки не должны жёстко зашиваться в компоненты.

Базовые локали:

- en;
- ru;
- he.

Добавление нового языка должно требовать в основном нового translation resource, а не изменения бизнес-логики.

---

## 19. Git и backup

Git не является обязательной зависимостью AutoCoder.

AutoCoder может в будущем использовать Git/GitHub как инструменты разработки, но пользовательская безопасность не должна зависеть от наличия Git repository.

Собственная backup/rollback система остаётся обязательной.

В будущем Workspace Transaction может использовать Git как дополнительный источник diff/history, если он доступен, но не как единственную защиту.

---

## 20. Первая специализация — COMSOL только после универсального ядра

COMSOL не является текущим этапом развития архитектуры.

Первой практической специализацией планируется:

**COMSOL Multiphysics 6.4.429**

Особенно:

- Java Shell;
- Methods;
- Java API.

COMSOL-specific corpus, retrieval, UI, backend logic и knowledge engine добавляются только после того, как универсальное ядро AutoCoder практически завершено и стабильно.

COMSOL Knowledge Engine должен опираться на общий Project Intelligence / retrieval architecture.

Источники знаний:

- официальная документация;
- API / Programming Reference;
- Java API;
- Learning Center;
- официальные примеры;
- рабочие пользовательские проекты.

Приоритет фактов:

1. локальная официальная документация;
2. подтверждённые примеры;
3. рабочий код пользователя;
4. генерация LLM.

Если достоверной информации недостаточно, AutoCoder должен показывать неопределённость, а не выдумывать API.

---

## 21. Предварительный стек

Текущий базовый стек:

- Desktop/UI: Tauri 2 + React + TypeScript;
- Editor: Monaco Editor;
- OS/process/file layer: Rust/Tauri;
- AI/backend orchestration services: Python;
- Local database: SQLite;
- Local LLM runtime: Ollama.

Стек не является самоцелью. Конкретный компонент можно заменить, если фактические требования проекта этого потребуют, но архитектурные границы и инварианты должны сохраняться.

Перед использованием быстро меняющихся API, библиотек и инструментов необходимо проверять актуальную официальную документацию, версии, совместимость и лицензии.

---

## 22. Лицензии

Проект потенциально коммерческий.

Предпочтение сторонним компонентам с permissive licenses:

- MIT;
- BSD;
- Apache-2.0;
- другим совместимым permissive licenses.

Лицензии значимых зависимостей проверяются фактически перед включением в production architecture.

---

## 23. Принцип развития архитектуры

Главный принцип:

> Не реализовывать все будущие функции заранее, но не строить ближайший этап способом, который заведомо ломает путь к конечной архитектуре.

Нужно различать:

- преждевременную реализацию функции;
- необходимую архитектурную границу.

Например, Docker Tool не нужно реализовывать заранее, но Capability Runtime должен позволять добавить Docker Tool без переписывания Orchestration Core.

COMSOL retrieval не нужно реализовывать заранее, но Project Intelligence должен быть общим, а не COMSOL-specific.

AI Diagnostics Review не нужно строить раньше рабочей diagnostics infrastructure, но System Model и coverage architecture должны позволять добавить его естественно.

---

## 24. Правила архитектурной ответственности

Устойчивые инварианты проекта:

- один subsystem / resource lifecycle — один логический владелец;
- Orchestration Core — единственный владелец task state machine;
- Rust supervisor — владелец физических AutoCoder-owned child processes;
- Provider Runtime — владелец AI-provider semantics, но не OS process ownership desktop runtime;
- Workspace Transaction — владелец фактического применения AI-изменений;
- Editor отображает workspace, но не заменяет его;
- Tool Runtime исполняет capability, но не объявляет semantic completion задачи;
- Persistence хранит факты/state, но не решает business transitions;
- UI отправляет intents и отображает state, но не является главным orchestration engine;
- Diagnostics наблюдает систему, но не участвует в принятии business decisions.

Если новая функция нарушает эти границы, сначала пересматривается архитектура, а не добавляется ещё одна локальная защита.

---

## 25. Проектная память и состояние

`PROJECT_MEMORY.md` содержит только устойчивые сведения:

- конечную цель;
- целевую архитектуру;
- постоянные ограничения;
- фундаментальные инварианты;
- долгосрочные решения.

`PROJECT_STATE.md` содержит изменяемое состояние:

- фактически реализованные части;
- PR/commit history;
- текущие проверки;
- известные баги;
- milestone status;
- незавершённые миграции;
- ближайший технический шаг.

Нельзя помещать временную текущую проблему в `PROJECT_MEMORY`, если она не раскрыла постоянный архитектурный принцип.

---

## 26. Принципы работы над AutoCoder

- Фактический repository state важнее памяти модели.
- Подтверждённое поведение Windows build важнее предположений.
- Технические API и возможности быстро меняющихся инструментов проверяются по актуальной официальной документации.
- Нельзя выдумывать API, параметры или элементы интерфейса.
- Нельзя исправлять неизвестную причину по последовательности догадок, если можно сначала получить фактическую диагностику.
- При повторяющемся классе багов нужно искать отсутствующий архитектурный механизм, а не бесконечно добавлять локальные исключения.
- Существенные архитектурные изменения должны оставлять приложение в рабочем состоянии после каждого этапа миграции.
- Не переписывать рабочие механизмы без причины: существующий код нужно либо перенести под новую границу ответственности, либо заменить только когда он нарушает целевую архитектуру.
- Один законченный технический пакет должен решать связанный архитектурный результат, а не только ближайший симптом.
- COMSOL остаётся финишной специализацией после универсального ядра.

---

## 27. Открытые архитектурные вопросы после фиксации памяти

После принятия этого PROJECT_MEMORY отдельно спроектировать и проверить на фактическом repository state:

1. точную семантику `Participant` в Model Council;
2. сложную настраиваемую схему передачи информации между командами, уровнями и раундами;
3. формальную модель `Position` и Position Stability Analysis;
4. хранение deliberation data между Execution Ledger, Diagnostics и persistent stores;
5. точный migration order от текущей архитектуры к целевой без переписывания проекта с нуля;
6. критерии и механизмы автоматического architecture/diagnostics coverage discovery.

Эти вопросы нельзя закрывать предположениями до повторного аудита фактического проекта после обновления PROJECT_MEMORY.
