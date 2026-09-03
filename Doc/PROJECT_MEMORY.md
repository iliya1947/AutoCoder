# AutoCoder — PROJECT_MEMORY

> **Статус: FROZEN ARCHITECTURE CONTRACT v1.**
>
> Этот документ фиксирует долгоживущую архитектуру AutoCoder. После финального freeze-аудита обычная разработка, bugs, новые tools/providers/protocols, refactors, migrations и implementation choices должны укладываться в этот contract, а не становиться поводом переписывать его.

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
- способным объяснить, какие действия были выполнены и на основании каких фактов задача считается завершённой;
- не зависящим архитектурно от конкретного SDK, протокола, AI-провайдера, parser/indexer engine или другого заменяемого внешнего компонента.

AutoCoder является универсальным инструментом разработки. Архитектура проекта не должна вводить искусственные ограничения на то, какие обычные программные проекты пользователь может создавать. Permissions, confirmations, diagnostics, process ownership и backup существуют для управления действиями самого AutoCoder и целостности workspace, а не как content-based ограничения на назначение создаваемого ПО.

Автоматизация должна уменьшать ручную работу пользователя, а не переносить на него необходимость вручную восстанавливать внутреннее состояние системы.

### 2.1. Свобода AI-разработчика

**Полная техническая свобода AI-разработчика является фундаментальным свойством AutoCoder.**

После того как пользователь задаёт цель, выбранные им границы автономности и доступные системе ресурсы, AI должен иметь возможность самостоятельно выбирать наиболее подходящий путь разработки, а не работать внутри искусственного whitelist заранее предусмотренных действий.

Архитектура AutoCoder не должна без технической необходимости ограничивать AI фиксированным списком:

- инструментов и capabilities;
- языков и технологий;
- моделей и providers;
- способов поиска и получения контекста;
- файловых операций;
- shell/process действий;
- network-enabled development tools;
- dependencies;
- способов рефакторинга или архитектурной миграции;
- тестов, diagnostics и verification methods;
- способов реализации внутреннего модуля или внешней интеграции.

AI должен уметь самостоятельно использовать любой capability, который фактически доступен текущему runtime, разрешён выбранной пользователем policy и не блокируется реальными OS/platform/environment permissions.

Capability discovery отвечает на вопрос **«что система умеет и что сейчас доступно?»**. Authorization/approval policy отвечает на вопрос **«какие из доступных действий пользователь разрешил выполнять автоматически?»**. Эти механизмы не должны смешиваться в hardcoded whitelist возможностей AI.

Пользователь должен иметь возможность выбрать режим максимальной автономности, в котором AutoCoder самостоятельно использует доступные инструменты, сеть, процессы, зависимости и методы проверки без per-action подтверждения в пределах явно заданных пользователем и системных границ.

Безопасность должна обеспечиваться фактическими permissions, изоляцией, ownership, recoverability, backups, observability и выбранной пользователем policy, а не искусственным обеднением технических возможностей AI.

**Reliability-механизмы не должны становиться capability gates.** Execution Ledger, Durable Execution, Diagnostics, provenance, risk/effect metadata, Workspace Transaction, replay и другие механизмы достоверности отвечают за вопрос «что произошло, можно ли безопасно продолжать и подтверждён ли результат?», а не за создание скрытого whitelist действий AI.

Неполное знание AutoCoder о новом capability, его risk/effect classification, reversibility, idempotency, языке, технологии или способе проверки **само по себе не является основанием запретить AI использовать этот capability**. Неизвестность должна оставаться явным состоянием и управляться пользовательской policy, diagnostics, reconciliation и observability. `Unknown` не должно автоматически означать `forbidden`.

**Capability space является динамическим и не фиксируется на момент старта Task.** В пределах effective user policy и фактических OS/platform/environment permissions AI должен иметь возможность расширять собственный toolbox во время выполнения задачи, в том числе:

- устанавливать development dependencies, CLI и другие необходимые инструменты;
- создавать временные или постоянные scripts/helpers/utilities;
- запускать локальные services/processes, необходимые для разработки и проверки;
- обнаруживать и подключать новые providers, tools, language/debug services и другие capabilities;
- создавать или подключать новые adapters/integrations, если это является лучшим способом решения задачи;
- регистрировать reusable capabilities, когда структурированная интеграция действительно полезна;
- повторно выполнять capability discovery после изменения environment/runtime.

Использование нового инструмента через уже доступный general-purpose capability, например Terminal/shell/process execution, **не должно требовать предварительного создания first-class Tool Manifest только ради получения разрешения на использование**. Tool Manifest / registry нужен для structured discovery, reusable integration, schemas, diagnostics и richer execution semantics, а не как лицензия, без которой AI запрещено применять технически доступный инструмент.

### 2.2. Пользовательская настраиваемость и AI-managed settings

**Всё поведение AutoCoder, которое архитектурно предусмотрено как настраиваемое, выбираемое, ограничиваемое, профильное или включаемое/выключаемое, должно иметь явную пользовательскую настройку.**

Если функция или policy допускает несколько технически поддерживаемых режимов, AutoCoder не должен прятать выбор одного из них как необъяснимый hardcode. Пользователь должен иметь возможность увидеть и изменить соответствующее поведение через подходящий уровень настроек.

Это относится, в частности, к:

- enable/disable функций и экспериментальных возможностей;
- providers/models и их execution settings;
- autonomy/approval policies;
- network/research policies;
- tools/capabilities и их пользовательским ограничениям;
- Council profiles, topology, consensus/evaluation и rounds;
- scheduler/hardware/performance/cost budgets;
- privacy/external-data policies;
- diagnostics, retention/export и developer options;
- backup/recovery policies там, где существует реальный пользовательский выбор;
- любым будущим feature flags, режимам, лимитам и profiles, которые являются частью поведения продукта.

**Наличие настройки не должно превращать AutoCoder в систему ручной конфигурации.** Для operational/performance/execution settings, где это имеет смысл, Settings model должна позволять различать как минимум:

- `Auto / AI-managed` — AutoCoder сам выбирает effective value для текущей задачи/ситуации;
- явное пользовательское значение / override;
- пользовательский lock / hard constraint, который AutoCoder не меняет самостоятельно.

В Full Autonomy незакреплённые пользователем operational settings должны иметь возможность оставаться AI-managed. Пользователь не обязан заранее вручную выбирать модель, Council topology, количество rounds, tools, context strategy, verification strategy, parallelism, timeouts, retry parameters и другие поддерживаемые технические параметры, если он предпочитает поручить их AutoCoder.

Явная настройка означает **контроль пользователя**, а не обязательное ручное вмешательство. Пользователь может открыть настройку, задать значение и при необходимости закрепить его; в остальных случаях AutoCoder должен иметь возможность самостоятельно выбирать оптимальное значение в пределах пользовательской authority/policy.

Сложная или редко используемая настройка может находиться в `Advanced` / `Expert` / developer settings, но не должна существовать только как скрытая константа, если продукт реально поддерживает её изменение.

Presets вроде Fast / Balanced / Deep Review могут упрощать настройку, но не должны превращаться в закрытый список и скрывать underlying options, которые AutoCoder архитектурно поддерживает.

**Искусственные application-level hard ceilings не должны ограничивать capability space без технической причины.** Если limit/budget не является реальным пределом hardware, OS/platform, provider/API или другого внешнего contract, соответствующая настройка должна по возможности поддерживать `Auto`, пользовательское значение и режим без application-level limit / unlimited там, где это технически осмысленно. Реальные физические и внешние ограничения остаются фактическими constraints, а пользовательская policy может сознательно задавать более узкие limits/budgets.

Это правило не требует превращать каждую внутреннюю переменную реализации в UI setting. Оно относится к **фактически поддерживаемому изменяемому поведению продукта и policy**, а не к implementation detail, которое не имеет осмысленного пользовательского выбора.

---

## 3. Главный принцип владения архитектурой

**Архитектура AutoCoder принадлежит AutoCoder.**

Сторонняя библиотека, runtime, SDK или внешний протокол может использоваться как реализация отдельной commodity-функции, но не должен становиться владельцем внутренней архитектуры продукта.

Целевая модель:

AutoCoder-owned abstractions / contracts
→ replaceable adapters
→ сторонние реализации, системные сервисы, протоколы или provider APIs.

Примеры:

- Persistence architecture принадлежит AutoCoder; SQLite — текущая реализация локального store;
- Provider Runtime принадлежит AutoCoder; Ollama — один из provider/runtime adapters;
- Project Intelligence принадлежит AutoCoder; Tree-sitter, LSP, SCIP и другие источники могут подключаться через adapters;
- Diagnostics принадлежит AutoCoder; OpenTelemetry используется как совместимая модель/ориентир, а не как обязательный владелец diagnostics pipeline;
- Capability Runtime принадлежит AutoCoder; MCP может быть внешним interoperability adapter;
- Editor architecture принадлежит AutoCoder; Monaco — текущий editor engine;
- Durable Execution semantics принадлежит AutoCoder; Temporal, Restate и DBOS являются источниками проверенных идей, а не фундаментальными runtime dependencies.

### 3.1. Replacement readiness

Если сторонний компонент используется непосредственно, он должен по возможности находиться за AutoCoder-owned interface/adapter таким образом, чтобы его замена не требовала переписывания Orchestration Core или других несвязанных подсистем.

Заменяемость означает:

- contract AutoCoder не копирует внутреннюю модель сторонней библиотеки без необходимости;
- сторонние типы не растекаются по всему приложению;
- provider/parser/indexer/runtime-specific данные нормализуются на boundary;
- lifecycle и error semantics внешнего компонента переводятся в общие AutoCoder contracts;
- тесты AutoCoder проверяют собственный contract отдельно от конкретной реализации;
- новая реализация может быть подключена через тот же порт/adapter.

Не нужно создавать абстракцию ради абстракции. Граница нужна там, где компонент действительно является сменной реализацией или внешней системой.

---

## 4. Целевая архитектура

Целевая архитектура AutoCoder строится из отдельных слоёв с чёткой ответственностью:

1. UI.
2. Orchestration Core.
3. Durable Execution Engine + Execution Ledger.
4. Project Intelligence.
5. Provider Runtime.
6. Model Council / Multi-Model Deliberation.
7. Tool / Capability Runtime.
8. Workspace ChangeSet / Transaction.
9. State / Persistence.
10. Runtime Supervisor.
11. Diagnostics / Introspection Plane.
12. System Model / Self-Model.
13. Interoperability / Protocol Adapters.

Все слои опираются на общие основы:

- schema-first versioned internal protocol;
- stable IDs;
- явное process ownership;
- capability / permission model;
- AutoCoder-owned Settings / Policy model;
- structured errors;
- versioned persistence schema;
- restart-safe semantics;
- explicit durable-step semantics для недетерминированных и side-effect операций;
- AutoCoder-owned internal contracts, не завязанные на конкретный внешний SDK.

### 4.1. UI

UI отображает фактическое состояние системы и передаёт пользовательские intents.

UI не должен быть владельцем orchestration state machine.

Основные пользовательские intents:

- начать задачу;
- одобрить или отклонить действие;
- предоставить требуемый input / возобновить blocked или waiting execution;
- остановить задачу;
- явно возобновить/повторить ранее остановленное или неуспешное выполнение, если соответствующий workflow это поддерживает;
- открыть / изменить проект;
- работать с редактором и интерактивными инструментами;
- просматривать изменения, историю, diagnostics и результаты;
- настраивать providers, council profiles, autonomy, hardware/scheduler policies, research policy и другие поддерживаемые settings.

UI может содержать локальное представление состояния для рендеринга, но источник истины для жизненного цикла автономной задачи должен находиться в Orchestration Core.

### 4.2. Orchestration Core

Orchestration Core — единственный логический владелец жизненного цикла AI-задачи.

Он отвечает за:

- immutable исходную цель пользователя;
- декомпозицию и семантику требований;
- текущее состояние задачи;
- допустимые переходы state machine;
- выбор следующего шага;
- стратегический выбор model/provider/council execution strategy на основании доступных capabilities и пользовательской policy;
- выбор AI-managed operational settings в пределах effective user policy/locks;
- связь model turn ↔ action ↔ factual result;
- уровни автономности и approval policy;
- execution budgets;
- completion / blocked / failed / stop / resume semantics;
- restart-safe continuation;
- защиту от поздних или устаревших результатов;
- координацию single-model и multi-model execution.

Frontend, provider, tools и persistence не должны независимо решать, какое состояние orchestration task является текущим или какой переход допустим.

Изменение пользователем уже выполняющейся задачи не является обязательной функцией ближайшего ядра. Архитектура при этом не должна делать такую возможность принципиально невозможной: в будущем пользовательские amendments могут сохраняться как durable факты, формирующие актуальную effective specification без переписывания исходного intent задним числом.

---

## 5. Durable Execution Engine и Execution Ledger

### 5.1. Execution Ledger

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
- AuthorizationDecisionRecorded / ActionApproved / ActionDeclined;
- EffectivePolicyChanged, если изменение влияет на активную task execution;
- DurableStepStarted;
- DurableAttemptStarted;
- DurableAttemptCompleted / DurableAttemptFailed / DurableAttemptInterrupted;
- DurableStepCompleted / DurableStepFailed / DurableStepInterrupted;
- ToolStarted;
- ToolCompleted / ToolFailed / ToolInterrupted;
- WorkspaceChanged;
- ReconciliationCompleted;
- RequirementSatisfied;
- TaskBlocked;
- TaskStopRequested;
- TaskStopped;
- TaskFailed;
- TaskResumed;
- TaskCompleted.

Текущее состояние задачи вычисляется из последовательности событий. Snapshot может использоваться как cache для ускорения загрузки, но не должен заменять фактическую историю событий.

Event sourcing не требуется распространять на всё приложение. Он нужен прежде всего там, где критична доказуемая причинность: orchestration, execution lifecycle, recovery и replay.

### 5.2. Ledger append и concurrency semantics

Append-only история должна иметь явный persistent concurrency contract. Один логический owner task state machine недостаточен, если несколько процессов, attempts или поздних результатов могут конкурентно пытаться записать события.

Каждый значимый Ledger event должен иметь stable `EventId`. Для task/event stream должна существовать монотонная revision/position или эквивалентный механизм причинного порядка.

State-changing append должен поддерживать optimistic concurrency / compare-and-swap semantics или функционально эквивалентную гарантию:

прочитана revision N
→ принято решение
→ append(event, expectedRevision=N).

Если stream уже изменился, stale append не должен бесшумно приниматься. Orchestration обязана перечитать факты и повторно определить допустимый transition.

Retry одного и того же append после неизвестного transport/database результата должен быть idempotent по `EventId` или эквивалентной identity, чтобы один логический факт не превращался в два события.

Late или superseded execution attempt не должен иметь возможность изменить актуальный task state только потому, что его результат физически пришёл позже. Attempt/epoch/fencing semantics должны позволять распознавать потерявший authority writer.

Точный storage-level механизм — SQLite transaction, expected revision, sequence, CAS, fencing token или другой вариант — является implementation detail при условии сохранения этих семантических гарантий.

### 5.3. Durable Step, attempts и retry semantics

Недетерминированная или выполняющая внешний side effect операция должна иметь явный durable execution contract.

К таким операциям относятся, например:

- model/provider call;
- Tool execution;
- network research;
- user approval / external signal;
- workspace mutation;
- запуск внешнего процесса;
- другие операции, которые нельзя безопасно повторять вслепую после crash/restart.

Для такого шага система должна уметь фиксировать как минимум:

intent
→ durable step identity
→ execution attempt identity
→ started
→ фактический result / failure / interruption
→ committed completion state.

Один durable step может иметь несколько execution attempts. Каждый фактический semantic attempt должен иметь отдельную identity и оставаться наблюдаемым в Execution Ledger/Diagnostics.

**Semantic retry/recovery принадлежит Durable Execution Engine.** Provider/transport/tool adapter может выполнять внутренний технический retry только когда повтор доказанно безопасен по семантике конкретной операции и не скрывает от Durable Execution отдельный смысловой execution attempt.

После restart/replay AutoCoder не должен автоматически повторять уже подтверждённый side effect только потому, что orchestration process был перезапущен.

Если исход операции неизвестен, система должна явно считать его неизвестным и применять специальную reconciliation/recovery логику, а не угадывать.

### 5.4. Версия orchestration semantics

Durable history должна интерпретироваться совместимой версией orchestration semantics.

Незавершённая задача должна сохранять достаточную identity версии state-machine/reducer/business logic, с которой была создана её durable history. После обновления AutoCoder система не должна слепо продолжать старую history изменившейся несовместимой логикой.

Для незавершённой задачи после обновления допустимы только явно определённые варианты:

- совместимое продолжение;
- контролируемая migration/upcast;
- reconciliation;
- перевод задачи в явное blocked/incompatible состояние.

Точный способ идентификации версии — номер схемы, implementation hash, compatibility generation или другой механизм — проектируется отдельно.

### 5.5. Recorded nondeterminism и deterministic recovery

Свобода AI во время **live execution** не должна ограничиваться требованием детерминированности. AI может использовать время, random, сеть, внешние сервисы, текущий filesystem/environment, модели, новые инструменты и любые другие доступные недетерминированные capabilities.

Но если недетерминированное наблюдение влияет на durable task transition или последующее recovery, существенный результат этого наблюдения должен стать durable fact/result или иметь эквивалентную replayable representation.

Recovery/replay одной и той же durable history должен быть детерминирован относительно:

- сохранённой history/facts/results;
- совместимой orchestration semantics version;
- явно сохранённых пользовательских/policy inputs.

Replay не должен заново читать современное состояние мира и выдавать новый результат за исторический факт старого run. Повторное обращение к текущему filesystem, wall clock, random, network/provider или другому nondeterministic источнику является новым execution observation и должно иметь собственную durable семантику.

Это правило ограничивает не свободу AI, а только способ безопасного восстановления уже произошедшей причинной цепочки.

### 5.6. Evidence validity и freshness

Факт успешной проверки имеет смысл только относительно состояния входов и окружения, на которых эта проверка была выполнена.

Evidence, используемое для `RequirementSatisfied` или `TaskCompleted`, должно по мере зрелости системы сохранять достаточный provenance, например:

- requirement/action/execution identity;
- релевантный WorkspaceRevision или набор входных revisions/hashes;
- environment/tool/provider version, если это влияет на достоверность;
- время и источник результата.

После изменения релевантных входов соответствующее evidence больше нельзя автоматически считать доказательством текущего состояния. Система должна либо доказать, что изменение не затронуло область применимости evidence, либо выполнить актуальную проверку.

На раннем этапе допустима более консервативная workspace-wide revision model; более точная dependency-aware freshness может быть добавлена позже без изменения общего принципа.

### 5.7. Termination, cancellation, resume и execution authority

Нужно различать **намерение прекратить выполнение** и факт физического прекращения уже начатой операции.

`TaskStopRequested` или эквивалентный durable intent означает: после принятия stop/revocation Orchestration не должна dispatch-ить новые действия под прежним execution authority. Для уже запущенных attempts система должна запросить cooperative/best-effort cancellation там, где capability это поддерживает, но **запрос cancellation не является доказательством, что side effect не успел произойти**.

Уже завершённый или физически неизбежный side effect не стирается из истории из-за stop/cancel. Его результат сохраняется как факт и, если он больше не соответствует желаемому состоянию, обрабатывается через compensation/reconciliation там, где это возможно.

Late result после `TaskStopped`, `TaskFailed` или другой закрытой execution authority может быть записан как исторический факт, но не должен самовольно возобновить orchestration, удовлетворить requirement или изменить terminal outcome текущего execution generation.

Фундаментальная семантика состояний:

- `Blocked` — выполнение сейчас не может продвинуться без внешнего условия/input/reconciliation; состояние не означает semantic success и может быть продолжено после снятия blocker;
- `Stopped` — пользователь или действующая policy явно прекратили текущую execution authority; новые actions не запускаются до явного resume/retry;
- `Failed` — текущая execution authority завершилась неуспешно из-за unrecoverable/aborted failure и не считается выполнением пользовательской цели;
- `Completed` — успешный terminal outcome, допустимый только при актуальном factual evidence достижения цели.

Явный resume/retry после `Stopped`/`Failed` должен создавать новую execution authority generation/epoch или функционально эквивалентную fencing identity. Старые attempts/results не получают authority только потому, что логическая Task снова стала активной.

Точные enum names и representation могут меняться, но различие stop request, фактического in-flight execution, terminal outcome, resumability и authority fencing является frozen invariant.

### 5.8. Собственная реализация

Durable Execution Engine должен быть AutoCoder-owned модулем поверх Execution Ledger и Persistence.

**Ориентиры / решения, которые нужно изучать, но не принимать как фундаментальную зависимость:** Temporal, Restate, DBOS и другие durable-workflow systems. Из них полезны идеи durable steps, journal/history, idempotency, recovery, replay, external signals, execution attempts, versioning, recorded nondeterminism, cancellation semantics и controlled retries.

Цель — перенести проверенные принципы в собственную архитектуру AutoCoder, не отдавая внешнему workflow engine владение orchestration state machine и не вводя обязательный отдельный server/runtime без необходимости.

---

## 6. Project Intelligence

Project Intelligence — AutoCoder-owned слой понимания пользовательского проекта.

Он должен со временем уметь:

- строить структуру проекта;
- искать файлы и текст;
- читать релевантные части проекта;
- понимать символы и зависимости;
- учитывать открытый файл и выделение;
- находить тесты и связанные файлы;
- учитывать фактические изменения workspace;
- использовать semantic retrieval и специализированные knowledge sources;
- получать точные language-intelligence facts из внешних источников;
- подготавливать ограниченный, релевантный контекст для модели.

Контекст модели не должен бесконтрольно собираться внутри UI-компонента или одного prompt builder.

### 6.1. Источники Project Intelligence

Project Intelligence должен быть агрегатором нескольких источников, а не единым жёстко пришитым parser/indexer engine.

Возможные источники:

- собственный filesystem/text/search index;
- syntax/AST parser adapter;
- language-server adapter;
- persistent code-index adapter;
- Git/history adapter;
- debugger/runtime facts adapter;
- semantic/vector retrieval;
- специализированные knowledge sources.

**Актуальные ориентиры / возможные сменные реализации:**

- Tree-sitter — быстрый incremental syntax/AST parsing engine;
- LSP — стандартный источник language intelligence;
- SCIP — language-agnostic формат точного code index;
- DAP — стандартный интерфейс к debugger/runtime facts на более позднем этапе.

AutoCoder не должен строить внутреннюю архитектуру вокруг Tree-sitter, конкретного LSP server, SCIP indexer или debug adapter. Они должны подключаться через собственные interfaces/adapters.

Если Tree-sitter или другой parser используется как библиотека, parser-specific AST/types не должны бесконтрольно проникать в Orchestration Core или UI. Project Intelligence нормализует нужные факты в свои contracts.

LSP и DAP являются interoperability protocols. Их adapter может быть реализован самим AutoCoder по спецификации без обязательной зависимости от конкретного SDK.

Конкретные language servers, grammars, SCIP indexers и debug adapters являются отдельными сторонними компонентами и проверяются по версии, совместимости и лицензии перед включением в дистрибутив.

COMSOL Knowledge Engine в будущем должен использовать общий Project Intelligence / retrieval слой, а не создавать параллельную архитектуру.

### 6.2. Fact provenance, freshness и observation scope

Project Intelligence должен различать **факт/наблюдение и его область применимости**, а не выдавать любой когда-либо полученный index/retrieval result за вечную истину о текущем проекте.

Нормализованный project fact или существенный retrieval result должен по мере зрелости системы иметь достаточный provenance/freshness context, например:

- источник/adapter;
- WorkspaceRevision, file/input revision/hash или другой релевантный scope;
- время получения, если источник внешне изменяемый;
- tool/index/provider version, если это влияет на интерпретацию;
- признак derived/heuristic и confidence/provenance, если факт не является прямым наблюдением.

Parser/LSP/index/Git/debugger/vector retrieval и другие источники могут устаревать независимо друг от друга. Stale data может использоваться как historical/contextual information, но перед state-changing решением или semantic completion должна быть revalidated, если её актуальность материальна для решения.

Изменение workspace/environment не обязано глобально инвалидировать все знания: допускается dependency/scope-aware freshness. Но неизвестная актуальность не должна незаметно превращаться в утверждение о текущем состоянии.

Project Intelligence предоставляет Orchestration нормализованные observations/context; он не получает instruction authority пользователя и не объявляет semantic completion задачи.

---

## 7. Provider Runtime

Модели подключаются через AutoCoder-owned provider abstraction.

Provider Runtime отвечает за:

- взаимодействие с локальными и облачными AI-провайдерами;
- model discovery / enumeration / resolution;
- capability discovery / negotiation;
- generation settings;
- structured outputs;
- native tool calling, если конкретный provider/model это поддерживает;
- streaming, reasoning/thinking, vision и другие возможности моделей, если доступны;
- model/provider metadata;
- timeout policy;
- безопасные transport-level retries в пределах durable execution contract;
- безопасную работу с credentials.

**Стратегический выбор модели/provider/council принадлежит Orchestration Core / Council Engine / AI execution strategy, а не Provider Runtime.** Provider Runtime сообщает, какие модели доступны и как их выполнить. Если provider поддерживает собственный routing/fallback, это должно быть явной execution strategy/capability, а не скрытой заменой решения Orchestration.

Provider не должен определять orchestration state machine или самостоятельно владеть semantic retry/recovery.

Модель не является источником истины о фактическом выполнении инструментов. Она предлагает решения, а backend независимо проверяет их и сопоставляет с доступными capability contracts.

Provider Runtime должен поддерживать как локальные LLM, так и API-модели. Orchestration Core не должен быть архитектурно привязан к Ollama или любому конкретному API.

Ollama является текущим local model runtime/provider adapter, а не фундаментальной частью внутренней модели AutoCoder. Его замена на другой local runtime не должна требовать изменения Orchestration Core.

---

## 8. Model Council / Multi-Model Deliberation

AutoCoder должен поддерживать не только выбор одной модели, но и **совместную работу произвольного количества локальных LLM и облачных API-моделей над одной задачей**.

Это одна из ключевых возможностей конечного продукта.

Council Engine является AutoCoder-owned subsystem. Он не должен зависеть от стороннего multi-agent framework как от владельца deliberation semantics.

Council Engine не должен иметь искусственного архитектурного лимита на количество участников. Практические пределы определяются только:

- доступной RAM / VRAM;
- скоростью локального железа;
- ограничениями providers;
- API rate limits;
- стоимостью API;
- размером контекста;
- выбранными пользователем или AI-managed настройками параллелизма и deliberation.

AutoCoder должен предупреждать пользователя о потенциальной нагрузке в месте настройки Council Profile, но не вводить искусственный hard limit на число моделей.

Явная configurability Council не означает обязательную ручную конфигурацию. В AI-managed режиме AutoCoder должен иметь возможность самостоятельно выбирать участников, models/providers, roles, topology, rounds, consensus/evaluation strategy, research policy и scheduler parameters в пределах пользовательских overrides/locks и фактических ресурсов.

### 8.1. Участники совета

Каждый участник совета должен иметь собственную конфигурацию.

Пользователь должен иметь возможность независимо задать для каждого участника:

- provider;
- model;
- role;
- отдельный custom user prompt;
- enabled / disabled state;
- при необходимости дополнительные model/provider settings;
- необязательный weight / priority.

Если соответствующие поля находятся в `Auto / AI-managed`, Council Engine может формировать и изменять конфигурацию участников самостоятельно согласно задаче и effective policy.

Одинаковая модель может участвовать в совете несколько раз с разными ролями и разными пользовательскими prompt-инструкциями.

`Participant` является логической deliberation identity/configuration, а не архитектурно зафиксированным конкретным process или provider instance. Точная schema/binding к execution instance является implementation-design вопросом внутри этого invariant.

### 8.2. Раунды обсуждения

Максимальное число deliberation rounds должно быть настраиваемым. Пользователь может задать/закрепить конкретное значение, а в AI-managed режиме AutoCoder может выбрать подходящий лимит самостоятельно.

Это верхняя граница, а не обязательное количество раундов. Если участники достигли выбранного критерия консенсуса раньше, обсуждение заканчивается раньше.

Критерий консенсуса должен быть настраиваемым и также может быть AI-managed, если пользователь его не закрепил. Система должна позволять использовать:

- полное совпадение решения;
- совпадение ключевого плана;
- заданный процент согласия;
- отсутствие существенных возражений;
- комбинации этих критериев.

Если используется пороговый, а не полный консенсус, AutoCoder обязан явно показать пользователю оставшиеся различия между позициями. Нельзя представлять частичное согласие как полное единодушие.

### 8.3. Базовый deliberation cycle

Базовый цикл должен поддерживать:

proposal
→ critique
→ factual verification / research при необходимости
→ revised position
→ comparison
→ consensus or next round.

Модели должны иметь возможность критиковать предложения друг друга, пересматривать собственную позицию после новых аргументов и фактов и продолжать обсуждение до достижения критерия консенсуса либо лимита раундов.

Deliberation не является отдельным чат-шоу. Его результат должен быть связан с фактической задачей, Project Intelligence, tools, tests, execution results и другими проверяемыми источниками системы.

### 8.4. Internet Research внутри раунда

В deliberation должна быть возможность фактической проверки утверждений через интернет.

Интернет-поиск не должен быть скрытой привилегией конкретной облачной модели. Он должен быть доступен Council Engine как отдельная research capability / tool, чтобы локальные и облачные модели могли опираться на общие проверяемые evidence.

Пользовательская политика должна позволять настраивать, когда интернет-проверка разрешена или требуется, например:

- полностью отключена;
- по запросу участников;
- разрешена в каждом раунде;
- обязательна для проверяемых внешних утверждений перед финальным решением;
- AI-managed в пределах разрешённой пользователем network policy.

Research results должны сохранять фактические источники и время получения, чтобы участники могли ссылаться на одни и те же evidence, а Diagnostics могла восстановить причинную цепочку решения.

### 8.5. Команды и масштабирование совета

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

В AI-managed режиме AutoCoder может выбирать размер/количество команд и topology самостоятельно, если пользователь не закрепил эти значения.

Количество логических участников не должно означать одновременную загрузку всех локальных моделей. Runtime Scheduler должен позволять выполнять участников последовательно или ограниченными параллельными группами согласно настройкам железа.

**Логический Council не должен сужаться из-за physical scheduling.** Например, профиль из 100 логических участников может выполняться на железе, где одновременно запускаются только 1–2 модели. Scheduler управляет способом физического выполнения выбранной конфигурации, а не определяет, какие Council configurations AI или пользователь в принципе имеют право задавать.

Council Engine отвечает за логические deliberation requirements/topology; runtime resource scheduling исполняет их в пределах фактически доступного железа и пользовательской hardware/performance policy. Точное component placement Runtime/Resource Scheduler является implementation-design решением и не меняет этот ownership contract.

Настройки scheduler/hardware должны позволять адаптировать одну и ту же логическую конфигурацию совета под различное железо. Архитектура должна допускать настройки вроде:

- maximum parallel local model executions;
- maximum simultaneously loaded models;
- RAM / VRAM budget;
- maximum concurrent API calls;
- context budget per participant;
- team size / team count;
- пользовательские performance profiles;
- `Auto / AI-managed` и отсутствие application-level limit там, где нет реального технического hard ceiling.

Нагрузка должна быть объяснена пользователю предупреждением в месте конфигурации Council Profile, а не искусственным запретом.

### 8.6. Капитаны и капитанские раунды

В Team / Hierarchical Council после завершения командного этапа должен определяться победитель/капитан команды.

Основная предполагаемая стратегия отбора, которую нужно доработать и проверить экспериментально: после общей критики, проверок и пересмотра позиций предпочтение получает участник, **чья смысловая позиция сохранила наибольшую устойчивость и к чьему итоговому выводу независимо приблизились остальные**, при обязательном учёте factual evidence, результатов tools/tests, существенных нерешённых возражений и diversity сигналов.

Position Stability и convergence не являются доказательством правильности сами по себе. Они должны использоваться как сигналы внутри заменяемой/расширяемой evaluation policy, а не как навсегда зашитый алгоритм Council Engine. Если реальные эксперименты покажут более надёжную стратегию выбора, архитектура должна позволять заменить или скомбинировать её без переписывания deliberation core и frozen architecture.

Это не должно сводиться к простой текстовой похожести. Точный алгоритм Position Stability Analysis и weighting сигналов является implementation-design вопросом.

Капитаны переходят на следующий уровень и проходят **тот же общий принцип deliberation**, а не отдельную непрозрачную judge-логику.

Максимальное количество captain rounds должно быть отдельно настраиваемым. Пользователь может задать/закрепить значение, а в AI-managed режиме AutoCoder может выбрать его самостоятельно. Как и обычные rounds, это верхняя граница: если капитаны достигли выбранного критерия консенсуса раньше, капитанский этап завершается раньше.

Финальный победитель капитанского уровня становится итоговым победителем совета для соответствующего решения.

Отдельный обязательный Judge-модуль не является фундаментальным требованием архитектуры.

### 8.7. Position Stability, diversity и calibration

AutoCoder должен уметь анализировать, насколько позиция участника изменилась между раундами после критики, evidence и ответов других участников.

Это нужно для:

- выбора устойчивых предложений;
- определения победителей/капитанов;
- анализа convergence;
- объяснения пользователю, как совет пришёл к результату.

Стабильность позиции не должна автоматически означать правильность. Она является одним из сигналов совместно с:

- factual evidence;
- результатами инструментов и тестов;
- convergence других участников;
- разнообразием исходных позиций;
- confidence/calibration signal, если он доступен и имеет смысл для конкретной модели/метода оценки;
- отсутствием или наличием существенных нерешённых возражений.

Council Engine должен сохранять **diversity of viewpoints** там, где преждевременное усреднение может скрыть ошибку. Разные роли и custom prompts могут использоваться как один из механизмов создания независимых исходных подходов.

Self-reported confidence модели не является доказательством правильности. Любой confidence signal должен рассматриваться как дополнительный сигнал и по возможности калиброваться/проверяться фактическими результатами.

Точная модель `Position`, алгоритм смыслового сравнения и weighting сигналов являются implementation-design вопросами внутри заменяемой evaluation policy.

### 8.8. Передача информации между командами и уровнями

Схема передачи между командами/уровнями должна сохранять:

- proposals;
- critiques;
- factual evidence;
- unresolved disagreements;
- team results;
- positions;
- captain-level context;
- ссылки на исходные ответы;
- diversity/confidence metadata, если они используются.

Цель — сохранять фактические аргументы и причинность, не заставляя большие советы бесконтрольно дублировать полный сырой контекст всех участников.

Точные packing/summarization/storage algorithms проектируются как implementation detail внутри этих требований и не требуют пересмотра frozen architecture.

### 8.9. Экспериментальная topology: Rotating / Overlapping Teams

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

### 8.10. Council Profiles

Пользователь должен иметь возможность сохранять reusable Council Profiles.

Профиль может включать:

- список участников;
- provider/model каждого участника;
- role;
- custom prompt каждого участника;
- max deliberation rounds;
- max captain rounds;
- consensus policy;
- captain/evaluation policy;
- topology;
- teams configuration;
- internet research policy;
- scheduler / hardware policy;
- cost/token/API budget;
- дополнительные экспериментальные параметры.

Каждый поддерживаемый профильный параметр должен иметь возможность быть явно задан пользователем или оставлен `Auto / AI-managed`, если самостоятельный выбор этого параметра AutoCoder технически поддерживается.

AutoCoder может поставлять предустановленные профили вроде Fast / Balanced / Deep Review, но пользовательские профили не должны быть ограничены ими, а все реально поддерживаемые profile options должны оставаться доступными для явной настройки.

---

## 9. Tool / Capability Runtime

Инструменты должны регистрироваться через AutoCoder-owned Capability Registry / Tool Manifest, а не через набор разрозненных условных конструкций.

Tool Manifest должен со временем описывать статические или относительно устойчивые свойства capability:

- stable id;
- operations;
- input schema;
- result schema;
- execution backend;
- approval / risk policy hooks;
- diagnostics category;
- capability metadata;
- version;
- known resource scope;
- известные read-only / mutating semantics;
- известные idempotency semantics;
- destructive/reversible/compensatable semantics, если они могут быть достоверно описаны на уровне capability;
- external/open-world side effects, если они возможны.

Эти свойства нужны для planning, Full Autonomy, Durable Execution, recovery и diagnostics. Для внешних tools metadata от недоверенного сервера не считается доказательством фактических свойств capability; adapter/host обязан сохранять границу доверия.

Capability Registry является механизмом discovery и нормализации доступных возможностей, **а не whitelist того, что AI в принципе разрешено уметь**.

Orchestration Core спрашивает registry, какие capabilities реально доступны в текущем runtime, вместо hardcoded предположений. Capability discovery должна обновляться динамически, когда AI устанавливает/создаёт/подключает новые технические возможности во время задачи.

Новый reusable/structured capability должен иметь возможность подключаться через manifest/adapter/registry без переписывания Orchestration Core. При этом одноразовое или general-purpose использование технически доступного CLI/script/process через Terminal/shell не обязано сначала становиться first-class registry entry.

Authorization и approval применяются к фактически доступным capabilities согласно пользовательской policy; они не должны скрывать capability из архитектуры только потому, что конкретный режим требует подтверждения.

File и Terminal являются первыми реализациями Tool Runtime, но не определяют архитектуру всех будущих инструментов и не образуют закрытый список допустимых действий.

### 9.1. Action Effect Profile

Статический Tool Manifest не может надёжно описать все эффекты конкретного вызова универсального capability.

Например один и тот же Terminal capability может выполнить `git status` или destructive filesystem command. Поэтому конкретный Action/Invocation должен иметь возможность получать отдельный runtime `Action Effect Profile` или эквивалентное описание фактических/ожидаемых эффектов.

Такой профиль может включать, когда это возможно определить:

- фактический resource scope;
- read-only / mutating;
- destructive/non-destructive;
- idempotent/non-idempotent/unknown;
- reversible/compensatable/irreversible/unknown;
- local/external/open-world effect;
- confidence/provenance классификации.

Effect classification может строиться из manifest metadata, аргументов конкретного вызова, adapter knowledge, runtime observation и других источников.

**Отсутствие полного Action Effect Profile или значение `unknown` не должно автоматически запрещать выполнение capability.** Оно является входом для пользовательской policy, planning, diagnostics и recovery. В Full Autonomy неизвестный эффект остаётся допустимым, если capability фактически доступен и effective user policy не требует иного.

Риск может зависеть от композиции нескольких действий, а не только от одного вызова. Архитектура должна позволять учитывать chain/session-level effects без превращения статического Tool Manifest в закрытый список разрешённых workflows.

### 9.2. MCP как внешний compatibility target

Внутренний Tool Runtime **не должен строиться на MCP как на своём core protocol**.

При необходимости AutoCoder должен иметь собственный MCP-compatible adapter, который переводит внешние MCP tools/resources/capabilities в AutoCoder Tool Manifest / capability contracts и обратно там, где это имеет смысл.

**Ориентир:** Model Context Protocol (MCP) — внешний стандарт для связи agent ↔ tools/data. Перед реализацией всегда проверяется текущая официальная спецификация: отдельные MCP features могут эволюционировать, переноситься в extensions или deprecated без влияния на frozen internal architecture AutoCoder.

Реализация может быть собственной, SDK-based или hybrid. Конкретный выбор делается по сложности, качеству, совместимости, поддерживаемости, лицензии и стоимости; замена реализации или версии протокола не должна требовать изменения Orchestration Core.

---

## 10. Workspace ChangeSet / Transaction

AI должна изменять **workspace**, а редактор должен отображать workspace.

Целевая модель:

AI / Orchestration
→ Workspace ChangeSet
→ Diff / Preview
→ Approval policy
→ Workspace Transaction
→ backup
→ precondition validation
→ apply
→ verify
→ rollback/reconciliation при необходимости
→ editor/project reconciliation.

ChangeSet должен поддерживать как минимум create / modify / delete и в будущем multi-file **logically atomic / crash-recoverable bounded transactions**.

ChangeSet должен строиться относительно известного состояния workspace: WorkspaceRevision, per-file revisions/hashes или других проверяемых preconditions. Перед применением система обязана проверять, что релевантные входы не были неожиданно изменены пользователем, другим Tool, formatter, Git operation, другой автономной задачей или внешним процессом.

При несовпадении preconditions запрещён blind overwrite. Требуется conflict/reconciliation/re-plan logic.

Если несколько автономных задач могут изменять один workspace, mutation semantics должны быть явно определены: serialized writes, optimistic concurrency control или другой проверяемый механизм. Конкретная стратегия является implementation detail при сохранении conflict/precondition guarantees.

Нельзя считать Monaco/editor buffer конечным execution backend для фактических AI-изменений. Terminal, compiler и другие инструменты должны видеть тот же фактический workspace, который AutoCoder считает изменённым.

Git может использоваться как дополнительный механизм diff/history, но не как обязательная зависимость backup/rollback.

Workspace Transaction является AutoCoder-owned subsystem. Реализация filesystem/backup primitives может меняться без изменения orchestration semantics.

### 10.1. Transaction boundary и внешние side effects

**Workspace Transaction не означает глобальную атомарность всей пользовательской Task.** Его atomic/recovery guarantees относятся только к тем workspace mutations, которыми AutoCoder реально владеет и которые входят в конкретный bounded transaction.

Одна Task может одновременно включать эффекты вне этого transaction boundary, например:

- package manager / system environment changes;
- запуск или конфигурацию внешних processes/services;
- Git remote operations;
- cloud/API calls;
- публикацию artifacts;
- external databases/services;
- другие local/open-world side effects.

Для таких действий нельзя обещать общий «rollback всего Task» как физически атомарную операцию. Они должны использовать Durable Execution identity, idempotency/reconciliation и, если действие реально обратимо, explicit compensation. Для irreversible или unknown-outcome действий система сохраняет факт и reconciles текущее состояние вместо выдуманного rollback.

UI/Diagnostics должны различать **workspace rollback** и **external compensation/reconciliation**. Откат файлов не означает, что уже совершённый внешний effect исчез.

Установка инструмента или изменение host environment вне workspace также является фактическим side effect и должно быть наблюдаемым/управляться effective policy; это не запрещает AI выполнять такие действия в Full Autonomy.

---

## 11. Workspace identity

Нужен first-class stable `WorkspaceId` для identity фактического workspace/project.

Display name проекта не является identity.

Если системе нужен отдельный идентификатор конкретного открытия/активной сессии workspace, он должен иметь отдельную семантику и lifetime, например `ProjectSessionId`, и не подменять стабильный `WorkspaceId`.

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

Workspace root/identity определяет project context и transaction scope, но **не является автоматически абсолютным filesystem sandbox для всех general-purpose capabilities**. Доступ за пределы workspace определяется реальными OS/platform permissions и effective user policy. Внешние protocol concepts вроде roots/scopes не должны подменять фактический authorization boundary AutoCoder.

---

## 12. Runtime Supervisor и backend process model

Целевой runtime — Rust-supervised long-lived AutoCoder Backend Runtime.

Rust/Tauri владеет физическим lifecycle AutoCoder-owned child processes:

- запуск;
- health supervision;
- restart infrastructure/service process после crash;
- task-scoped cancellation, где это технически возможно;
- shutdown;
- process-tree cleanup.

Нужно различать infrastructure/service process и process, который является фактическим Task Action. Rust supervisor может физически наблюдать и завершать оба типа, но **semantic restart/retry Task Action не должен происходить автоматически на уровне process supervisor**: решение о повторе, reconciliation или неизвестном исходе принадлежит Durable Execution Engine.

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

### 12.1. Ollama lifecycle

Нужно сохранять принцип **один ресурс — один физический lifecycle owner**.

Для desktop runtime:

- Rust supervisor владеет запуском/завершением AutoCoder-owned Ollama process;
- Provider Runtime отвечает за provider semantics: endpoint, readiness, models, capabilities, request/response metadata и ошибки.

Текущая Python-side логика запуска Ollama может существовать как compatibility/standalone fallback во время миграции, но после перехода на long-lived supervised backend не должна случайно превращаться во второго независимого владельца одного и того же desktop process lifecycle.

Это не отменяет поддержку разных local/API providers. Наоборот, Ollama должен стать одним provider adapter среди многих и должен быть заменяемым.

---

## 13. Schema-first internal protocol

Ключевые cross-layer contracts должны быть versioned и schema-first.

В частности:

- task state/events и event-stream revisions;
- durable steps/attempts и execution authority/fencing identity;
- actions/results и action effect profiles;
- authorization decisions / effective policy references;
- tool manifests;
- provider capabilities;
- provider responses;
- council positions/results;
- workspace revisions/changesets/results;
- Project Intelligence facts/observation provenance;
- evidence/provenance references;
- settings/policies;
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

Protocol/schema compatibility не заменяет orchestration-semantics compatibility незавершённой durable task: эти уровни должны проверяться отдельно.

### 13.1. Внутренний протокол не равен внешним стандартам

AutoCoder internal protocol должен оставаться собственным и отражать фактические потребности AutoCoder.

MCP, ACP, A2A, LSP, DAP и другие внешние protocols не должны диктовать внутреннюю модель задачи, tool lifecycle, workspace transaction, Council или persistence.

Внешняя совместимость обеспечивается adapters на границе системы.

---

## 14. Interoperability / Protocol Adapters

AutoCoder должен уметь взаимодействовать с внешней экосистемой через сменные protocol adapters без превращения этих протоколов в архитектурное ядро.

Целевая модель:

AutoCoder internal contracts
→ protocol adapter
→ внешний стандарт / external agent / tool / editor / language service.

### 14.1. MCP adapter

Назначение: external tools/data/capabilities ↔ AutoCoder Capability Runtime.

**Ориентир:** Model Context Protocol.

Adapter должен переводить внешние schemas/capabilities/results в AutoCoder-owned Tool Manifest / action/result contracts.

### 14.2. ACP adapter

Назначение: совместимость coding agent ↔ editor/IDE.

**Ориентир:** Agent Client Protocol (ACP).

В перспективе AutoCoder может:

- подключать внешний ACP-compatible coding agent к своему UI;
- при необходимости предоставлять часть собственного agent runtime другому ACP-compatible editor.

ACP не должен становиться внутренним orchestration protocol AutoCoder.

### 14.3. A2A adapter

Назначение: взаимодействие с независимыми внешними agent systems.

**Ориентир:** Agent2Agent Protocol (A2A).

Внутренний Model Council не должен реализовываться через A2A только ради стандарта. Но внешний agent/system потенциально может быть представлен как отдельный `ExternalAgentParticipant` через A2A adapter.

### 14.4. LSP / DAP adapters

LSP и DAP используются как внешние compatibility boundaries для language intelligence и debugging/runtime facts.

AutoCoder может реализовать protocol clients/adapters самостоятельно по спецификациям. Конкретные language servers/debug adapters остаются сменными external implementations.

### 14.5. Реализация adapters

Нет автоматического приоритета собственной реализации перед SDK или наоборот.

Для каждого adapter нужно рассматривать как минимум:

- собственную реализацию нужного protocol subset по официальной спецификации;
- изолированный SDK;
- hybrid-вариант;
- другие технически подходящие реализации.

Выбор делается по фактическим требованиям: корректность, сложность, performance, compatibility, maintainability, лицензия, стоимость поддержки и replacement readiness.

В любом варианте внутренние AutoCoder contracts важнее структуры конкретной implementation. Замена SDK, собственной реализации или версии протокола не должна требовать перестройки несвязанных подсистем.

Перед реализацией проверяются актуальные официальные спецификации, версии, compatibility и лицензии. Изменение внешнего стандарта в нормальном случае приводит к изменению adapter/implementation, а не frozen `PROJECT_MEMORY.md`.

---

## 15. Provider capabilities и Model Execution Profile

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
- transport retry policy в пределах Durable Execution semantics;
- required provider capabilities.

Все параметры Model Execution Profile, которые AutoCoder фактически поддерживает как изменяемые, должны иметь соответствующие пользовательские настройки или профильные overrides согласно общей Settings model. Там, где параметр может безопасно выбираться автоматически, он должен поддерживать AI-managed effective value и пользовательский override/lock.

Diagnostics должна фиксировать фактически применённый execution profile и реальные execution attempts там, где retry влияет на причинную цепочку.

Provider envelope должен по возможности сохранять полезные metadata ответа: модель, finish/done reason, timings, token/eval counts и другие доступные provider metrics.

---

## 16. State / Persistence

SQLite остаётся базовым локальным persistent store на текущем этапе.

Persistence architecture должна принадлежать AutoCoder и быть отделена от SQLite-specific деталей настолько, насколько это оправдано. Замена storage engine в будущем не должна требовать переписывания Orchestration Core.

Нужна versioned schema / migrations. Изменения структуры БД не должны зависеть от неявного совпадения версии приложения и старой базы.

Persistence отвечает за надёжное хранение данных, но не принимает orchestration business decisions.

Необходимо различать как минимум:

- durable execution facts;
- Ledger EventId / stream revision / idempotency data;
- orchestration semantics/version identity для незавершённых задач;
- execution authority/fencing generation для stop/resume/retry;
- snapshots/cache;
- chat/history;
- settings/profiles и их revisions/effective values там, где они влияют на durable decisions;
- authorization-decision provenance;
- workspace metadata/revisions;
- Project Intelligence fact/provenance metadata там, где оно persistent;
- evidence/provenance references;
- diagnostics retention;
- schema version.

---

## 17. Structured errors

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

## 18. Diagnostics / Introspection Plane

Diagnostics — AutoCoder-owned fundamental platform capability, а не локальная отладка отдельного бага.

Цель: по одному run можно восстановить причинную цепочку от пользовательского intent до фактического результата.

Пример цепочки:

Run
→ UI operation
→ orchestration task/version/authority generation
→ effective settings/policy revision
→ model/council turn
→ provider request
→ durable attempt
→ provider response
→ decision
→ authorization decision
→ validator
→ approval
→ durable step
→ tool execution / action effect
→ OS operation
→ filesystem/external mutation
→ reconciliation/compensation
→ evidence
→ persisted state.

### 18.1. OpenTelemetry-compatible conceptual model

Diagnostics должна использовать trace/span/context модель причинности и быть **семантически совместимой с актуальными OpenTelemetry concepts/conventions там, где это полезно**, но не зависеть архитектурно от OpenTelemetry Collector или конкретного OTel SDK.

Предпочтительная модель:

Trace
→ Span
→ correlated structured logs/events
→ metrics
→ artifact/payload references.

Каждое существенное событие должно иметь stable correlation identifiers, чтобы можно было связать cross-process и cross-language цепочку.

**Ориентир / решение, которое частично заменяется своим модулем:** OpenTelemetry. AutoCoder берёт проверенные concepts, naming/semantic conventions и возможность будущего compatible export, но хранение, replay, coverage и local diagnostics UI остаются собственными.

Если OpenTelemetry SDK используется технически, он должен находиться за diagnostics adapter/export boundary и быть заменяемым.

### 18.2. Architecture Inventory

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
- protocols/versions;
- protocol adapters.

Нельзя полагаться только на ручное правило «не забудь зарегистрировать новый модуль в diagnostics».

### 18.3. Runtime Discovery

Runtime traces должны автоматически обнаруживать реально участвующие:

- processes;
- commands;
- providers;
- tools;
- IPC calls;
- model turns;
- council rounds;
- database operations;
- workspace operations;
- external side effects/compensations, насколько они наблюдаемы;
- task transitions;
- external protocol boundaries.

### 18.4. Coverage Audit

Coverage Auditor сравнивает:

**что существует**
с
**что реально наблюдается diagnostics**.

Он должен находить blind spots и непокрытые boundaries.

Новый Tauri command, Tool, Provider, spawned process, protocol adapter или другой значимый boundary не должен бесшумно появляться вне наблюдаемой архитектуры.

В CI/build должен существовать Diagnostics Coverage Gate, который способен обнаруживать новые непокрытые компоненты/границы и заставлять разработчика осознанно решить, как они наблюдаются.

### 18.5. Replay

**Replay semantics принадлежат Durable Execution Engine + Orchestration logic совместимой версии.** Diagnostics не должна становиться вторым владельцем state transitions или durable recovery.

Diagnostics предоставляет captured history, results, correlations, artifacts и интерфейс исследования replay, а Durable Execution/Orchestration выполняют фактическую интерпретацию history по своим contracts.

Особенно важно уметь повторно прогнать сохранённый provider/model response через:

- parsing;
- validation;
- orchestration transition logic совместимой версии;
- council evaluation logic;
- reconciliation logic.

Replay использует сохранённые durable-step/attempt results там, где side effect уже был выполнен.

Replay не должен автоматически повторять destructive tool side effects или заново получать nondeterministic исторические observations.

### 18.6. Diagnostics UI / Bundle

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

### 18.7. AI Diagnostics Review

После появления надёжного System Model, coverage audit и runtime traces AutoCoder должен получить отдельный AI Diagnostics / Architecture Review.

LLM должна анализировать **достоверные диагностические данные**, а не угадывать архитектуру.

Ей передаются по необходимости:

- architecture/system map;
- component inventory;
- coverage gaps;
- relevant traces;
- structured errors;
- protocol/capability information.

Цель — находить blind spots, неправильные implementation boundaries, новые непокрытые компоненты и подозрительные causal chains внутри frozen architecture.

Сначала строится рабочая diagnostics infrastructure; AI review является слоем поверх неё.

### 18.8. Diagnostics invariants

Diagnostics должна быть пассивным наблюдателем:

- её failure не ломает основную работу приложения;
- она не должна менять business semantics;
- не должна скрывать race conditions изменением порядка выполнения;
- должна быть concurrency/process safe;
- payloads должны быть bounded;
- нужен retention/rotation/cleanup;
- нужен centralized redaction/sanitization согласно effective data policy;
- предыдущие diagnostic runs должны сохраняться достаточно долго для расследования restart/startup проблем;
- никаких автоматических внешних telemetry uploads без соответствующей пользовательской настройки/policy;
- работа локальная/offline по умолчанию.

---

## 19. System Model / Self-Model

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
- protocol adapters;
- replaceable implementations;
- available settings/policies и scopes;
- diagnostics coverage.

System Model должен строиться из фактических registries, runtime discovery, protocols и других проверяемых источников, а не вручную поддерживаемого списка, который легко забыть обновить.

В будущем AI Diagnostics Review сможет использовать System Model для анализа самого AutoCoder.

---

## 20. Permissions, credentials, trust и технические границы

Технические permissions должны управлять тем, какие реальные ресурсы доступны конкретному runtime/action. Они не должны превращаться в искусственный whitelist функциональности AI или ограничение на назначение создаваемого пользователем программного обеспечения.

Нужны разные уровни:

1. фактическая доступность capability в текущем runtime;
2. application / OS / platform permissions;
3. AutoCoder AI execution authorization / approval policy.

Эти уровни не должны подменять друг друга. Capability может быть известен системе и оставаться доступным архитектурно, даже если текущая пользовательская policy требует подтверждения его выполнения.

Approval policy должна быть пользовательски настраиваемой вплоть до **Full Autonomy**, где AutoCoder может самостоятельно выполнять доступные действия без per-action approval в пределах фактических OS/platform permissions и явно заданных пользователем ограничений.

Факт, что UI технически может вызвать command, сам по себе не определяет AI authorization. И наоборот, отсутствие заранее прописанного command/capability в статическом списке не должно запрещать AI использовать новый технически доступный capability в пределах effective policy только потому, что он ещё не оформлен отдельным first-class manifest/registry entry.

### 20.1. Content provenance и instruction authority

Project files, comments, README, документация, web content, tool output, MCP/resources, external-agent responses и другие полученные системой материалы могут содержать полезные факты и инструкции предметной области, но **не получают автоматически право изменять пользовательский intent, autonomy/authorization policy, system invariants или другие более высокие уровни authority**.

AutoCoder должен сохранять provenance существенного внешнего контента там, где это нужно для принятия решений, diagnostics и trust evaluation.

Архитектура должна различать как минимум:

- откуда пришла информация;
- насколько источник доверен;
- является ли содержимое данными/контекстом или авторизованной инструкцией;
- какие действия система вправе выводить из этого содержимого.

Эта граница не должна превращаться в запрет AI читать, анализировать или использовать внешние данные. Её задача — не позволить недоверенному контенту незаметно получить полномочия пользователя или системы.

### 20.2. External data / provider policy

Если пользователь подключил и разрешил облачный provider или внешний service, AutoCoder в обычном автономном режиме может самостоятельно передавать ему необходимый для задачи контекст без per-request подтверждений.

Data handling должна быть policy-driven, а не зашитой навсегда стеной. Базовая конфигурация должна защищать credentials, tokens и другие обнаруженные секреты от случайной внешней передачи, но это **настраиваемая default policy**, а не фундаментальный запрет capability space.

Secret detection/classification является общей cross-cutting функцией. Конкретное действие — redact, block, warn, allow, scope-specific allow или другое поддерживаемое поведение — определяется effective user policy и реальными platform constraints.

Пользователь должен иметь возможность изменять поддерживаемые политики раскрытия данных, включая более строгие privacy/enterprise режимы и более свободные режимы, когда это необходимо для легитимной задачи. Full Autonomy работает внутри выбранной policy без обязательных per-request подтверждений.

Архитектура не должна вводить обязательный детальный whitelist типов данных для каждого provider. Более строгие privacy/data-egress режимы добавляются как настраиваемые policies без изменения Orchestration Core и без искусственного ограничения пользователей, которые их не включили.

Для credentials нужен Secret Store abstraction.

`.env` допустим как development override, но не должен считаться конечным production-хранилищем API keys/tokens.

Diagnostics, persisted artifacts, provider/council payloads и export должны использовать общую secret/data classification и effective policy, а не иметь несогласованные hardcoded правила redaction в каждом subsystem.

### 20.3. Settings / Policy model

Settings architecture принадлежит AutoCoder и должна быть общей для UI, Orchestration, Council, Provider Runtime, Tool Runtime, Diagnostics и других подсистем, а не набором независимых hardcoded переключателей.

Для пользовательски изменяемой настройки по мере зрелости системы должны быть определимы как минимум:

- stable setting/policy id;
- type/schema и допустимые значения;
- default value;
- scope;
- control mode, если применимо: `Auto / AI-managed`, explicit user value/override, user lock/hard constraint;
- current/effective value;
- источник effective value / override;
- version/revision, если значение влияет на durable execution, authorization или последующее объяснение причинности.

Архитектура должна поддерживать подходящие scopes, например:

- global/application;
- workspace/project;
- profile;
- Council Profile;
- provider/model profile;
- task/run override;
- другие обоснованные будущие scopes.

Точная precedence model является implementation-design вопросом, но effective value должна быть однозначно вычисляема и диагностируема.

Если AutoCoder поддерживает `on/off`, выбор режима, лимит, policy, profile или иной пользовательский параметр — у него должна быть явная настройка. Новая экспериментальная функция, которую можно технически включить/отключить, должна иметь соответствующий feature setting/flag, даже если он находится только в Advanced/Experimental settings.

Для operational/performance/execution settings, которые AutoCoder способен выбирать самостоятельно, `Auto / AI-managed` должен быть first-class режимом, а не скрытым preset. Пользовательский override меняет effective value, а user lock/hard constraint запрещает AI самостоятельно его переопределять в соответствующем scope.

В Full Autonomy AutoCoder должен иметь возможность самостоятельно выбирать все незаблокированные operational settings, необходимые для выполнения задачи. Наличие UI-настройки не является требованием участия пользователя в каждом решении.

Настройки не должны превращаться в искусственный capability whitelist. Например отсутствие отдельной risk classification настройки для нового capability не означает автоматический запрет этого capability. Settings управляют поддерживаемым поведением и пользовательской policy; capability discovery остаётся отдельной системой.

Пользователь может разрешить AutoCoder автоматически подбирать или временно изменять operational/performance settings для выполнения задачи. Такая AI self-tuning возможность сама должна быть настраиваемой. Изменение authority/autonomy/privacy policies самим AI допускается только если пользовательская policy явно предоставляет такое право.

Limits/budgets должны отличать реальные external/physical constraints от application-level policy limits. Там, где продукт технически способен работать без искусственного потолка, пользовательская настройка должна по возможности позволять `Auto` или отсутствие application-level limit, а не скрытый hard maximum.

Presets и automatic modes являются удобными слоями над settings, а не заменой underlying configurability.

### 20.4. Authorization decision provenance

Для фактически выполняемого Action система должна иметь возможность восстановить, почему действие считалось разрешённым в момент исполнения.

Authorization decision должен по мере необходимости ссылаться на:

- action/execution identity;
- effective autonomy/authorization policy revision или эквивалентный immutable reference;
- relevant scope/inputs;
- результат решения;
- время/causal context.

В Full Autonomy такой provenance **не означает дополнительный approval step**. Действие может выполняться автоматически; система просто сохраняет факт, какая effective policy разрешала его в тот момент.

Изменение настроек после действия не должно переписывать историческую причину, по которой прошлое действие было разрешено.

### 20.5. Изменение policy во время активной Task

Пользовательский control должен сохраняться и после старта автономной задачи. Изменение authority/autonomy/privacy/network/resource policy не переписывает историю, но влияет на будущую execution authority согласно новой effective revision.

Для нового state-changing Action authorization должна быть актуальна на момент dispatch/commit boundary. Если между решением и фактическим запуском изменилась authority-relevant policy revision, старое разрешение не должно бесшумно использоваться как вечный permit: действие re-evaluates против текущей effective policy или явно подтверждённой immutable authorization scope.

Ужесточение/отзыв permission после начала Task должно прекращать dispatch новых несовместимых действий, как только изменение наблюдено Orchestration. Для уже запущенного attempt AutoCoder должен запросить cancellation, если это технически возможно, но не может считать revocation доказательством отсутствия уже совершённого side effect.

Result уже начатого действия после revocation сохраняется как факт и проходит reconciliation; он не предоставляет автоматическую authority для следующих действий.

Ослабление policy или новый пользовательский override может применяться к будущим actions без изменения исторических authorization decisions.

Если изменение policy влияет на активную durable task, соответствующая effective revision/change должна быть наблюдаемой в Ledger/Diagnostics. AI может менять authority-level policy только если пользователь заранее явно дал ему такое право.

---

## 21. Offline-first

AutoCoder после установки должен полноценно запускаться и выполнять локальные функции без интернета.

Offline-first означает **способность ядра работать без сети**, а не запрет AI использовать сетевые development capabilities, когда пользователь их включил.

Локально поставляются:

- UI;
- Monaco Editor и workers;
- JavaScript/CSS;
- шрифты и иконки;
- локализации;
- Python backend runtime;
- Tauri runtime resources;
- обязательные вспомогательные библиотеки.

Runtime-загрузки обязательных локальных компонентов из интернета запрещены как скрытая обязательная зависимость.

При этом пользователь может явно разрешить сетевые capabilities, включая:

- AI/API providers;
- internet research;
- Git/GitHub и другие remote repositories;
- package registries и dependency downloads;
- remote documentation;
- external APIs/services;
- cloud build/test/development services;
- любые другие сетевые development tools, которые пользователь подключил к AutoCoder или которые AutoCoder обнаружил/подключил в пределах разрешённой network policy.

Локальный Ollama должен работать без интернета после установки требуемой модели.

Build-time установки npm/cargo/pip не являются runtime dependency установленного приложения.

Никакой interoperability adapter не должен превращать интернет в скрытую обязательную runtime dependency, но архитектура не должна искусственно запрещать сеть там, где она является явно доступной и выбранной пользователем capability.

---

## 22. AI autonomy semantics

AutoCoder должен поддерживать диапазон от пошаговых подтверждений до **Full Autonomy** без перестройки ядра.

Уровень автономности является policy, а не отдельной архитектурой.

В режиме Full Autonomy AI самостоятельно выбирает и применяет доступные ему:

- модели и Council configuration;
- project/context retrieval;
- tools/capabilities;
- создание, установку, подключение и повторное обнаружение новых development capabilities;
- shell/process actions;
- network research и external development services;
- dependencies и implementation strategy;
- workspace/file operations;
- способы анализа, debugging и diagnostics;
- тесты и verification methods;
- AI-managed operational/performance settings, если пользователь их не закрепил;
- порядок итераций и исправлений;
- объём refactor/migration, необходимый для достижения пользовательской цели.

AutoCoder не должен требовать per-action approval в Full Autonomy, если действие находится в пределах фактических permissions и явно выбранных пользователем ограничений. Другие autonomy policies могут добавлять confirmations или более узкие execution boundaries без изменения самих capabilities ядра.

Full Autonomy не означает обязанность пользователя предварительно настроить каждый технический параметр. Незакреплённые settings могут определяться AutoCoder динамически по задаче, доступным ресурсам, evidence и текущему execution context.

При любой policy должны сохраняться базовые инварианты:

- model proposal не является фактическим результатом;
- success tool execution не обязательно означает semantic completion;
- factual result должен быть связан с конкретным action/execution id;
- user constraints остаются частью исходной задачи;
- completion требует актуальных фактических доказательств;
- `Blocked`, `Stopped`, `Failed`, `Completed` и stop/cancel request имеют различимую непротиворечивую семантику;
- stop/cancel request не является доказательством, что in-flight side effect физически не завершился;
- resume/retry после закрытой execution authority использует новую generation/epoch или эквивалентное fencing;
- restart не должен незаметно повторять действие с неизвестным результатом;
- late result не может незаконно изменить уже закрытую execution authority или terminal outcome;
- durable step с подтверждённым side effect не повторяется автоматически при replay/recovery;
- несовместимая новая orchestration logic не продолжает старую durable history вслепую;
- replay/recovery не переоценивает старые nondeterministic observations как будто они являются прежними фактами;
- reliability/risk metadata не создаёт скрытый capability whitelist;
- неизвестная классификация действия не равна автоматическому запрету;
- capability set может расширяться во время Task и не замораживается при старте;
- отсутствие first-class Tool Manifest не запрещает использование нового инструмента через доступный general-purpose capability;
- workspace rollback не выдаётся за глобальный rollback внешних side effects;
- irreversible/unknown external effects используют factual reconciliation/compensation semantics, а не выдуманную атомарность;
- Project Intelligence observations имеют provenance/freshness scope и не считаются текущей истиной после материального устаревания без revalidation;
- настройки управляют policy и режимами, но не подменяют capability discovery;
- явная configurability не означает обязательную ручную конфигурацию: незакреплённые operational settings могут быть AI-managed;
- authority-relevant policy change re-evaluates будущие actions и не переписывает исторические authorization decisions;
- application-level limits не должны превращаться в скрытые hard ceilings без фактической технической причины.

### 22.1. Свобода verification

AutoCoder AI самостоятельно выбирает достаточные способы проверки результата согласно задаче и риску.

Архитектура не должна иметь закрытого списка допустимых tests/checks. AI может использовать любые доступные verification capabilities: unit/integration/e2e tests, compilers, type checkers, linters, browser automation, shell scripts, debuggers, profilers, static/dynamic analyzers, fuzz/property tests, benchmarks, external services, custom temporary tooling и другие методы.

AI может создавать новые временные или постоянные test/diagnostic tools, если существующих механизмов недостаточно для получения фактической уверенности.

---

## 23. Мультиязычность

Мультиязычность учитывается архитектурно с начала проекта.

UI-строки не должны жёстко зашиваться в компоненты.

Базовые локали:

- en;
- ru;
- he.

Добавление нового языка должно требовать в основном нового translation resource, а не изменения бизнес-логики.

---

## 24. Git и backup

Git не является обязательной зависимостью AutoCoder.

AutoCoder может использовать Git/GitHub как инструменты разработки, но пользовательская безопасность не должна зависеть от наличия Git repository.

Собственная backup/rollback система для AutoCoder-owned workspace mutations остаётся обязательной.

В будущем Workspace Transaction может использовать Git как дополнительный источник diff/history, если он доступен, но не как единственную защиту.

Backup/rollback workspace не является обещанием отменить external side effects; для них действуют compensation/reconciliation semantics из раздела 10.1.

Те параметры backup/retention/recovery поведения, которые AutoCoder поддерживает как пользовательски изменяемые, должны быть представлены соответствующими settings согласно общей Settings model.

---

## 25. Первая специализация — COMSOL только после универсального ядра

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

## 26. Предварительный стек и сменные реализации

Текущий базовый стек:

- Desktop/UI: Tauri 2 + React + TypeScript;
- Editor engine: Monaco Editor;
- OS/process/file layer: Rust/Tauri;
- AI/backend orchestration services: Python;
- Local database: SQLite;
- Local LLM runtime: Ollama.

Стек не является самоцелью или whitelist. Конкретный компонент, язык или технология могут быть заменены, если фактические требования проекта или технический анализ показывают лучший вариант, при сохранении frozen архитектурных границ и инвариантов.

Особенно сменными должны считаться:

- local/cloud model providers;
- local model runtime;
- parser engines;
- language servers;
- code indexers;
- debug adapters;
- protocol SDKs;
- diagnostics exporters;
- search/retrieval backends;
- storage engine там, где AutoCoder-owned persistence contract позволяет замену.

### 26.1. Ориентиры для отдельных функций

Рядом с собственными модулями сохраняются наводки на проверенные внешние решения, чтобы при проектировании не изобретать уже известные механизмы вслепую:

- Durable Execution Engine → изучать Temporal / Restate / DBOS;
- Diagnostics / tracing semantics → изучать OpenTelemetry;
- syntax parsing → рассматривать Tree-sitter как сменный engine;
- language intelligence → LSP и конкретные language servers как сменные sources;
- persistent code intelligence → SCIP как формат/источник и сменные indexers;
- debugger integration → DAP и сменные debug adapters;
- external tools/data → MCP-compatible adapter;
- editor/coding-agent interoperability → ACP-compatible adapter;
- external agent systems → A2A-compatible adapter.

Эти наводки не означают обязательное включение соответствующей библиотеки в продукт и не ограничивают AI только перечисленными альтернативами.

Перед использованием быстро меняющихся API, библиотек, протоколов и инструментов необходимо проверять актуальную официальную документацию, версии, совместимость и лицензии. Изменившийся внешний API/standard адаптируется на implementation boundary и сам по себе не меняет frozen architecture.

---

## 27. Лицензии и dependency policy

Проект потенциально коммерческий.

На стадии исследования, сравнения и прототипирования AI может рассматривать любые технические решения, если их изучение допустимо соответствующими условиями. Лицензионная политика ограничивает прежде всего включение компонентов в production architecture и дистрибутив, а не технический поиск альтернатив.

Для распространяемых компонентов предпочтительны permissive licenses:

- MIT;
- BSD;
- Apache-2.0;
- другие совместимые permissive licenses.

Лицензии значимых зависимостей проверяются фактически перед включением в production architecture и дистрибутив.

### 27.1. Protocol ≠ implementation

Разрешение использовать протокол архитектурно не означает автоматического разрешения включать любой SDK/server/indexer/adapter, реализующий этот протокол.

Отдельно проверяются:

- protocol/specification terms;
- конкретный SDK;
- Tree-sitter grammar;
- language server;
- SCIP indexer;
- debug adapter;
- provider client/runtime;
- transitive dependencies;
- обязательные NOTICE/attribution requirements.

AutoCoder может реализовать protocol adapter самостоятельно, использовать SDK или hybrid-подход. Решение выбирается по технической выгоде и стоимости поддержки; при этом нельзя без необходимости копировать чужой исходный код или большие фрагменты лицензированной документации.

### 27.2. Непермиссивные фундаментальные зависимости

Компоненты с restrictive/source-available/business-source/copyleft условиями не должны становиться фундаментом распространяемого AutoCoder без отдельного сознательного архитектурного и лицензионного решения.

Это не запрещает AI изучать такие компоненты, использовать их как reference или технически сравнивать с другими вариантами.

---

## 28. Принцип развития архитектуры

Главный принцип:

> Не реализовывать все будущие функции заранее, но не строить ближайший этап способом, который заведомо ломает путь к frozen architecture.

Нужно различать:

- преждевременную реализацию функции;
- необходимую архитектурную границу.

Например, Docker Tool не нужно реализовывать заранее, но Capability Runtime должен позволять добавить Docker Tool без переписывания Orchestration Core.

COMSOL retrieval не нужно реализовывать заранее, но Project Intelligence должен быть общим, а не COMSOL-specific.

AI Diagnostics Review не нужно строить раньше рабочей diagnostics infrastructure, но System Model и coverage architecture должны позволять добавить его естественно.

MCP/ACP/A2A support не нужно реализовывать до реальной необходимости, но internal contracts не должны мешать добавить соответствующие adapters без перестройки ядра.

Не нужно заранее заменять все сторонние библиотеки собственными аналогами и не нужно заранее отдавать предпочтение внешним реализациям. Сначала сохраняется правильная ownership boundary, затем AI/разработчик выбирает собственную реализацию, сменную внешнюю implementation или hybrid по фактическим техническим критериям.

Будущие функции, которые не являются приоритетом текущего ядра — например изменение пользователем уже выполняющейся задачи или более строгие privacy/data-egress режимы — не нужно реализовывать заранее. Но они должны добавляться через существующие extensibility/policy/durable boundaries, а не требовать переписывать frozen contract.

То же относится к configurability: не нужно заранее строить UI для всех будущих настроек, но если текущая функция уже имеет поддерживаемый toggle/mode/policy/limit/profile, архитектура не должна оставлять этот выбор скрытым hardcode. Настройка может появляться одновременно с самой функцией и находиться в Advanced/Experimental UI.

AI-managed режим не нужно заменять десятками обязательных ручных настроек. По мере появления новой configurable функции нужно сохранять возможность явного пользовательского override/lock и, где это технически уместно, автоматического выбора AutoCoder.

---

## 29. Правила архитектурной ответственности

Устойчивые инварианты проекта:

- один subsystem / resource lifecycle — один логический владелец;
- Orchestration Core — единственный владелец task state machine и стратегического выбора execution path/models;
- Orchestration Core / AI execution strategy может выбирать незакреплённые AI-managed operational settings в пределах effective user policy;
- Durable Execution Engine + Execution Ledger — владелец durable-step/history/attempt/retry/replay/cancellation authority semantics, но не бизнес-цели задачи;
- Execution Ledger append имеет idempotent/concurrency-safe semantics и не принимает stale state transitions бесшумно;
- stop/cancel request не равен доказанному прекращению уже начатого side effect;
- resume/retry после закрытой execution authority использует новую generation/epoch или эквивалентное fencing;
- Rust supervisor — владелец физических AutoCoder-owned child processes, но не semantic retry Task Actions;
- Provider Runtime — владелец AI-provider semantics, discovery/resolution и execution, но не стратегического model selection, OS process ownership desktop runtime или orchestration retry/recovery;
- Council Engine — владелец deliberation/topology/position semantics, но не фактической истины о выполнении tools;
- Project Intelligence — владелец нормализованного project knowledge/context и его provenance/freshness semantics, но не конкретный parser/LSP/indexer;
- Project Intelligence observation не считается вечной текущей истиной вне своего freshness/scope;
- Workspace Transaction — владелец фактического применения AI-изменений и conflict/precondition handling;
- Workspace Transaction гарантирует только свой bounded workspace transaction и не обещает глобальную атомарность внешних side effects;
- external/irreversible effects используют idempotency, compensation/reconciliation и factual history, а не выдуманный общий rollback;
- Editor отображает workspace, но не заменяет его;
- Tool Runtime исполняет capability, но не объявляет semantic completion задачи;
- Tool Manifest описывает structured/reusable capability, а конкретный Action Effect может зависеть от invocation/runtime context;
- использование технически доступного нового инструмента через general-purpose capability не обязано предварительно получать first-class Tool Manifest;
- capability space динамически расширяется во время Task и не замораживается в момент старта;
- неизвестный/неполный risk/effect metadata не означает автоматический запрет capability;
- Persistence хранит факты/state, но не решает business transitions;
- UI отправляет intents и отображает state, но не является главным orchestration engine;
- Diagnostics наблюдает систему, предоставляет replay evidence/UI, но не владеет replay/state-transition semantics;
- interoperability adapters переводят внешние protocols в AutoCoder contracts, но не определяют внутреннюю архитектуру;
- сторонняя библиотека не должна становиться скрытым владельцем доменной семантики AutoCoder;
- capability registry описывает доступные возможности, но не задаёт искусственный whitelist возможностей AI;
- workspace root/context не является автоматически absolute security sandbox; authorization определяется effective policy + фактическими platform permissions;
- autonomy/approval policy управляет разрешением выполнения, но не должна обеднять архитектурный toolbox ядра;
- authority-relevant policy change применяется к будущей execution authority и требует re-evaluation stale authorization перед dispatch;
- историческое authorization решение связано с effective policy, которая действовала в момент действия, и не переписывается задним числом;
- reliability mechanisms не должны превращаться в capability gates;
- недоверенный внешний контент не получает автоматически instruction authority пользователя или системы;
- evidence должно оставаться связано с тем состоянием и входами, которые оно фактически проверяло;
- live AI execution может быть недетерминированным, но recovery старой durable history использует recorded observations/facts;
- всё поддерживаемое пользовательски изменяемое поведение должно иметь явную setting/policy boundary вместо скрытого hardcode;
- configurable operational settings должны позволять AI-managed выбор и пользовательские overrides/locks там, где это технически осмысленно;
- наличие настройки не означает обязательное участие пользователя: Full Autonomy использует AI-managed значения для незакреплённых параметров;
- presets/automatic modes не должны закрывать underlying configurability, которую продукт реально поддерживает;
- application-level limits/budgets не должны создавать произвольные hard ceilings там, где реального технического предела нет.

Если новая функция или implementation path не укладывается в эти frozen boundaries, **сначала пересматривается дизайн реализации**. Изменение `PROJECT_MEMORY.md` не является обычным способом приспособить архитектуру к локальному bug, framework или удобному short-term решению.

---

## 30. Проектная память и состояние

`PROJECT_MEMORY.md` содержит frozen устойчивые сведения:

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

Нельзя помещать временную текущую проблему в `PROJECT_MEMORY`. После freeze даже устойчивый implementation finding должен оформляться в коде, tests, `PROJECT_STATE.md` или другой implementation documentation, если он не является явным product-level изменением frozen contract, принятым пользователем.

---

## 31. Принципы работы над AutoCoder

- Фактический repository state важнее памяти модели.
- Подтверждённое поведение Windows build важнее предположений.
- Технические API и возможности быстро меняющихся инструментов проверяются по актуальной официальной документации.
- Нельзя выдумывать API, параметры или элементы интерфейса.
- AI-разработчик имеет свободу использовать любые доступные инструменты, методы, сеть, dependencies, tests и архитектурные подходы, если они помогают лучше выполнить задачу и находятся в пределах фактической среды и пользовательских границ.
- Capability space может расширяться самим AutoCoder во время задачи через установку, создание и подключение новых development tools/capabilities в пределах effective user policy.
- Reliability, security metadata и settings проектируются так, чтобы сохранять свободу AI, а не создавать новый whitelist через боковую дверь.
- Наличие подробных настроек не должно переносить orchestration decisions обратно на пользователя: незакреплённые технические параметры могут оставаться AI-managed.
- Нельзя исправлять неизвестную причину по последовательности догадок, если можно сначала получить фактическую диагностику.
- При повторяющемся классе багов нужно искать отсутствующий механизм реализации frozen contract, а не бесконечно добавлять локальные исключения и не переписывать MEMORY.
- Поэтапная миграция предпочтительна, когда она снижает риск и улучшает проверяемость; большой связный rewrite/refactor допустим, если фактический анализ показывает, что он является лучшим или необходимым решением внутри frozen architecture.
- Существующий рабочий механизм не является неприкосновенным: его нужно сохранить, перенести, переработать или заменить исходя из фактической пользы и frozen architecture.
- Один законченный технический пакет должен решать связанный архитектурный результат, а не только ближайший симптом; его размер не ограничивается искусственно.
- При проектировании новой внутренней функции изучаются зрелые внешние реализации/стандарты, после чего свободно выбирается лучший вариант: собственный модуль, сменная внешняя implementation через adapter, direct isolated dependency или hybrid.
- Если функция имеет пользовательски значимый изменяемый режим/toggle/policy/profile, соответствующая настройка является частью завершённой функции, а не необязательной будущей косметикой.
- Изменение внешней технологии/стандарта обычно меняет adapter/implementation, а не frozen `PROJECT_MEMORY.md`.
- COMSOL остаётся финишной специализацией после универсального ядра.

---

## 32. Implementation-design вопросы внутри frozen architecture

Следующие пункты **не являются незакрытыми фундаментальными архитектурными решениями**. Это очередь точного проектирования реализации. Любой выбранный ответ обязан сохранять frozen invariants выше и не должен требовать изменения `PROJECT_MEMORY.md` как обычной части разработки:

1. точная schema/binding `Participant` в Model Council при сохранении его logical deliberation identity;
2. packing/summarization/storage схема передачи информации между командами, уровнями и раундами с сохранением evidence и unresolved disagreements;
3. формальная модель `Position`, Position Stability Analysis, diversity/calibration signals и конкретные algorithms/weights заменяемой captain/evaluation policy;
4. физическое хранение deliberation data между Execution Ledger, Diagnostics и persistent stores;
5. точная schema durable-step/attempt contract, idempotency/reconciliation fields, граница semantic/transport retry и связь с replay;
6. storage-level реализация Ledger append: EventId, stream revision, expected-revision/CAS, idempotent append и attempt/execution-authority fencing;
7. конкретный механизм orchestration-semantics versioning/compatibility и migration незавершённых durable tasks между версиями AutoCoder;
8. перечень nondeterministic observations, которые становятся durable facts, и конкретная replay representation;
9. точная Evidence + Project Intelligence provenance/freshness schema: WorkspaceRevision, input hashes/dependency scope, source versions и invalidation algorithms;
10. конкретная concurrency strategy Workspace Transaction: serialized writes/optimistic concurrency, conflict UX и crash recovery;
11. migration order от текущей фактической реализации к frozen target architecture без искусственного запрета на связный rewrite, если он окажется лучшим решением;
12. механизмы автоматического architecture/diagnostics coverage discovery и CI integration;
13. минимальная normalized Project Intelligence fact schema и adapter contracts;
14. приоритет реализации interoperability adapters и поддерживаемый subset конкретной актуальной версии каждого внешнего протокола;
15. dependency/replacement map конкретных third-party implementations внутри frozen ownership boundaries;
16. точный capability discovery + authorization contract, включая dynamic install/create/connect/discover/register lifecycle и Full Autonomy без hardcoded whitelist;
17. representation authorization-decision provenance, active-policy re-evaluation и execution-authority generation/epoch;
18. Action Effect model, chain/session-level composition, compensation/reconciliation metadata при сохранении `unknown != forbidden`;
19. Settings / Policy precedence/effective-value algorithm, scopes, schema/versioning, `Auto / AI-managed`, overrides/locks, Advanced/Experimental UI и AI self-tuning;
20. representation limits/budgets: user policy limits, AI-managed limits, unlimited/no application-level limit и реальные hardware/OS/provider constraints;
21. механизм свободного выбора/создания verification/test capabilities и их evidence binding;
22. concrete provenance/trust/instruction-authority implementation и настраиваемая external-data policy внутри зафиксированных authority boundaries;
23. component placement logical Council scheduler/resource scheduler при сохранении разделения logical configuration и physical execution;
24. точные enum/event/API names для `Blocked`/`Stopped`/`Failed`/`Completed`, stop request, resume/retry и execution-authority fencing при сохранении фиксированной семантики раздела 5.7;
25. возможная экспериментальная поддержка durable amendments к уже выполняющейся пользовательской задаче без переписывания исходного intent.

Если ответ на один из этих implementation-design вопросов кажется требующим нарушения frozen invariant, это означает, что выбранный implementation design нужно пересмотреть или заменить.

---

## 33. Architectural freeze

После финального архитектурного аудита этот `PROJECT_MEMORY.md` считается **FROZEN ARCHITECTURE CONTRACT v1**.

Обычная разработка **не должна изменять этот файл** по следующим причинам:

- bug или regression;
- текущий PR/milestone;
- новый provider/model/tool/capability;
- новый или изменившийся внешний protocol/API;
- замена framework/library/runtime;
- schema/storage/IPC implementation choice;
- migration stage;
- performance optimization;
- security/reliability hardening, которое укладывается в существующие policy/recovery boundaries;
- новый Council algorithm/topology/evaluation strategy;
- refactor или полный rewrite реализации;
- более удобный способ решить один из вопросов раздела 32.

Все такие изменения должны реализовываться **под** frozen contract через существующие ownership, adapter, policy, capability, durability, transaction и diagnostics boundaries.

Если implementation не помещается в frozen architecture, default-действие — изменить implementation/design, а не MEMORY.

Изменение `PROJECT_MEMORY.md` допустимо только при одном из двух исключительных условий:

1. пользователь **явно принимает product-level изменение** конечной цели или фундаментального требования AutoCoder;
2. доказано внутреннее противоречие самого frozen contract, которое невозможно устранить корректной реализацией внутри уже зафиксированных invariants.

Новый framework, opinion модели, очередной аудит реализации или локальный technical debt сами по себе не являются таким основанием.

Изменение внешних стандартов не должно заставлять менять MEMORY: меняются adapters, supported protocol versions и implementation documentation. Изменение фактического состояния проекта фиксируется в `PROJECT_STATE.md`.

Таким образом дальнейшая работа над AutoCoder должна переходить от аудита архитектурной памяти к **реализации, verification и migration фактического repository state относительно этого frozen contract**.
