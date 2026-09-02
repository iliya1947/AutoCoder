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
- продолжить остановленное ожидание;
- остановить задачу;
- открыть / изменить проект;
- работать с редактором и интерактивными инструментами;
- просматривать изменения, историю, diagnostics и результаты;
- настраивать providers, council profiles, autonomy, hardware/scheduler policies и research policy.

UI может содержать локальное представление состояния для рендеринга, но источник истины для жизненного цикла автономной задачи должен находиться в Orchestration Core.

### 4.2. Orchestration Core

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
- ActionApproved / ActionDeclined;
- DurableStepStarted;
- DurableStepCompleted / DurableStepFailed / DurableStepInterrupted;
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

### 5.2. Durable Step semantics

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
→ step identity
→ started
→ фактический result / failure / interruption
→ committed completion state.

После restart/replay AutoCoder не должен автоматически повторять уже подтверждённый side effect только потому, что orchestration process был перезапущен.

Если исход операции неизвестен, система должна явно считать его неизвестным и применять специальную reconciliation/recovery логику, а не угадывать.

### 5.3. Собственная реализация

Durable Execution Engine должен быть AutoCoder-owned модулем поверх Execution Ledger и Persistence.

**Ориентиры / решения, которые нужно изучать, но не принимать как фундаментальную зависимость:** Temporal, Restate, DBOS и другие durable-workflow systems. Из них полезны идеи durable steps, journal/history, idempotency, recovery, replay, external signals и controlled retries.

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

---

## 7. Provider Runtime

Модели подключаются через AutoCoder-owned provider abstraction.

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
- выбранными пользователем настройками параллелизма и deliberation.

AutoCoder должен предупреждать пользователя о потенциальной нагрузке в месте настройки Council Profile, но не вводить искусственный hard limit на число моделей.

### 8.1. Участники совета

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

### 8.2. Раунды обсуждения

Пользователь задаёт **максимальное число deliberation rounds**.

Это верхняя граница, а не обязательное количество раундов. Если участники достигли выбранного критерия консенсуса раньше, обсуждение заканчивается раньше.

Критерий консенсуса должен быть настраиваемым. Система должна позволять использовать:

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
- обязательна для проверяемых внешних утверждений перед финальным решением.

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

### 8.6. Капитаны и капитанские раунды

В Team / Hierarchical Council после завершения командного этапа должен определяться победитель/капитан команды.

Базовый принцип отбора: после общей критики, проверок и пересмотра позиций предпочтение получает участник, **чья смысловая позиция изменилась меньше всего и к чьему итоговому выводу в результате пришли остальные**.

Это не должно сводиться к простой текстовой похожести. Точный алгоритм Position Stability Analysis проектируется отдельно после фиксации PROJECT_MEMORY и повторного аудита проекта.

Капитаны переходят на следующий уровень и проходят **тот же общий принцип deliberation**, а не отдельную непрозрачную judge-логику.

Пользователь должен отдельно настраивать **максимальное количество captain rounds**. Как и обычные rounds, это верхняя граница: если капитаны достигли выбранного критерия консенсуса раньше, капитанский этап завершается раньше.

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

Точная модель `Position`, алгоритм смыслового сравнения и weighting сигналов остаются отдельной архитектурной задачей после повторного аудита проекта.

### 8.8. Передача информации между командами и уровнями

Это отдельная важная архитектурная задача.

Нужно отдельно спроектировать сложную настраиваемую схему передачи:

- proposals;
- critiques;
- factual evidence;
- unresolved disagreements;
- team results;
- positions;
- captain-level context;
- ссылок на исходные ответы;
- diversity/confidence metadata, если они используются.

Цель — сохранять фактические аргументы и причинность, не заставляя большие советы бесконтрольно дублировать полный сырой контекст всех участников.

Эта схема должна проектироваться **после PROJECT_MEMORY и повторного аудита фактического проекта**, а не фиксироваться преждевременно.

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
- topology;
- teams configuration;
- internet research policy;
- scheduler / hardware policy;
- cost/token/API budget;
- дополнительные экспериментальные параметры.

AutoCoder может поставлять предустановленные профили вроде Fast / Balanced / Deep Review, но пользовательские профили не должны быть ограничены ими.

---

## 9. Tool / Capability Runtime

Инструменты должны регистрироваться через AutoCoder-owned Capability Registry / Tool Manifest, а не через набор разрозненных условных конструкций.

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

Capability Registry является механизмом discovery и нормализации доступных возможностей, **а не whitelist того, что AI в принципе разрешено уметь**.

Orchestration Core спрашивает registry, какие capabilities реально доступны в текущем runtime, вместо hardcoded предположений. Новый capability должен иметь возможность подключаться через manifest/adapter/registry без переписывания Orchestration Core.

Authorization и approval применяются к фактически доступным capabilities согласно пользовательской policy; они не должны скрывать capability из архитектуры только потому, что конкретный режим требует подтверждения.

File и Terminal являются первыми реализациями Tool Runtime, но не определяют архитектуру всех будущих инструментов и не образуют закрытый список допустимых действий.

### 9.1. MCP как внешний compatibility target

Внутренний Tool Runtime **не должен строиться на MCP как на своём core protocol**.

При необходимости AutoCoder должен иметь собственный MCP-compatible adapter, который переводит внешние MCP tools/resources/capabilities в AutoCoder Tool Manifest / capability contracts и обратно там, где это имеет смысл.

**Ориентир:** Model Context Protocol (MCP) — актуальный внешний стандарт для связи agent ↔ tools/data. Перед реализацией проверяется текущая официальная спецификация.

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
→ apply
→ verify
→ rollback при необходимости
→ editor/project reconciliation.

ChangeSet должен поддерживать как минимум create / modify / delete и в будущем multi-file atomic/bounded transactions.

Нельзя считать Monaco/editor buffer конечным execution backend для фактических AI-изменений. Terminal, compiler и другие инструменты должны видеть тот же фактический workspace, который AutoCoder считает изменённым.

Git может использоваться как дополнительный механизм diff/history, но не как обязательная зависимость backup/rollback.

Workspace Transaction является AutoCoder-owned subsystem. Реализация filesystem/backup primitives может меняться без изменения orchestration semantics.

---

## 11. Workspace identity

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

## 12. Runtime Supervisor и backend process model

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

- task state/events;
- durable steps;
- actions/results;
- tool manifests;
- provider capabilities;
- provider responses;
- council positions/results;
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

Перед реализацией проверяются актуальные официальные спецификации, версии, compatibility и лицензии.

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
- retry policy;
- required provider capabilities.

Diagnostics должна фиксировать фактически применённый execution profile.

Provider envelope должен по возможности сохранять полезные metadata ответа: модель, finish/done reason, timings, token/eval counts и другие доступные provider metrics.

---

## 16. State / Persistence

SQLite остаётся базовым локальным persistent store на текущем этапе.

Persistence architecture должна принадлежать AutoCoder и быть отделена от SQLite-specific деталей настолько, насколько это оправдано. Замена storage engine в будущем не должна требовать переписывания Orchestration Core.

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
→ orchestration task
→ model/council turn
→ provider request
→ provider response
→ decision
→ validator
→ approval
→ durable step
→ tool execution
→ OS operation
→ filesystem mutation
→ reconciliation
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

Diagnostics должна позволять воспроизводить captured causal chain без повторного недетерминированного model call.

Особенно важно уметь повторно прогнать сохранённый provider/model response через:

- parsing;
- validation;
- orchestration transition logic;
- council evaluation logic;
- reconciliation logic.

Replay использует сохранённые durable-step results там, где side effect уже был выполнен.

Replay не должен автоматически повторять destructive tool side effects.

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

Цель — находить blind spots, неправильные boundaries, новые непокрытые компоненты, архитектурные расхождения и подозрительные causal chains.

Сначала строится рабочая diagnostics infrastructure; AI review является слоем поверх неё.

### 18.8. Diagnostics invariants

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
- diagnostics coverage.

System Model должен строиться из фактических registries, runtime discovery, protocols и других проверяемых источников, а не вручную поддерживаемого списка, который легко забыть обновить.

В будущем AI Diagnostics Review сможет использовать System Model для анализа самого AutoCoder.

---

## 20. Permissions, credentials и технические границы

Технические permissions должны управлять тем, какие реальные ресурсы доступны конкретному runtime/action. Они не должны превращаться в искусственный whitelist функциональности AI или ограничение на назначение создаваемого пользователем программного обеспечения.

Нужны разные уровни:

1. фактическая доступность capability в текущем runtime;
2. application / OS / platform permissions;
3. AutoCoder AI execution authorization / approval policy.

Эти уровни не должны подменять друг друга. Capability может быть известен системе и оставаться доступным архитектурно, даже если текущая пользовательская policy требует подтверждения его выполнения.

Approval policy должна быть пользовательски настраиваемой вплоть до **Full Autonomy**, где AutoCoder может самостоятельно выполнять доступные действия без per-action approval в пределах фактических OS/platform permissions и явно заданных пользователем ограничений.

Факт, что UI технически может вызвать command, сам по себе не определяет AI authorization. И наоборот, отсутствие заранее прописанного command/capability в статическом списке не должно запрещать AI использовать новый корректно зарегистрированный capability.

Для credentials нужен Secret Store abstraction.

`.env` допустим как development override, но не должен считаться конечным production-хранилищем API keys/tokens.

Diagnostics обязана централизованно редактировать credentials, tokens и другие secrets до записи/export.

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
- любые другие сетевые development tools, которые пользователь подключил к AutoCoder.

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
- shell/process actions;
- network research и external development services;
- dependencies и implementation strategy;
- workspace/file operations;
- способы анализа, debugging и diagnostics;
- тесты и verification methods;
- порядок итераций и исправлений;
- объём refactor/migration, необходимый для достижения пользовательской цели.

AutoCoder не должен требовать per-action approval в Full Autonomy, если действие находится в пределах фактических permissions и явно выбранных пользователем ограничений. Другие autonomy policies могут добавлять confirmations или более узкие execution boundaries без изменения самих capabilities ядра.

При любой policy должны сохраняться базовые инварианты:

- model proposal не является фактическим результатом;
- success tool execution не обязательно означает semantic completion;
- factual result должен быть связан с конкретным action/execution id;
- user constraints остаются частью исходной задачи;
- completion требует фактических доказательств;
- cancelled/stopped/blocked/completed — разные состояния;
- restart не должен незаметно повторять действие с неизвестным результатом;
- late result не может незаконно изменить уже терминальное состояние задачи;
- durable step с подтверждённым side effect не повторяется автоматически при replay/recovery.

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

Собственная backup/rollback система остаётся обязательной.

В будущем Workspace Transaction может использовать Git как дополнительный источник diff/history, если он доступен, но не как единственную защиту.

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

Стек не является самоцелью или whitelist. Конкретный компонент, язык или технология могут быть заменены, если фактические требования проекта или технический анализ показывают лучший вариант, при сохранении или осознанном пересмотре архитектурных границ и инвариантов.

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

Перед использованием быстро меняющихся API, библиотек, протоколов и инструментов необходимо проверять актуальную официальную документацию, версии, совместимость и лицензии.

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

> Не реализовывать все будущие функции заранее, но не строить ближайший этап способом, который заведомо ломает путь к конечной архитектуре.

Нужно различать:

- преждевременную реализацию функции;
- необходимую архитектурную границу.

Например, Docker Tool не нужно реализовывать заранее, но Capability Runtime должен позволять добавить Docker Tool без переписывания Orchestration Core.

COMSOL retrieval не нужно реализовывать заранее, но Project Intelligence должен быть общим, а не COMSOL-specific.

AI Diagnostics Review не нужно строить раньше рабочей diagnostics infrastructure, но System Model и coverage architecture должны позволять добавить его естественно.

MCP/ACP/A2A support не нужно реализовывать до реальной необходимости, но internal contracts не должны мешать добавить соответствующие adapters без перестройки ядра.

Не нужно заранее заменять все сторонние библиотеки собственными аналогами и не нужно заранее отдавать предпочтение внешним реализациям. Сначала создаётся правильная ownership boundary, затем AI/разработчик выбирает собственную реализацию, сменную внешнюю implementation или hybrid по фактическим техническим критериям.

---

## 29. Правила архитектурной ответственности

Устойчивые инварианты проекта:

- один subsystem / resource lifecycle — один логический владелец;
- Orchestration Core — единственный владелец task state machine;
- Durable Execution Engine + Execution Ledger — владелец durable-step/history semantics, но не бизнес-цели задачи;
- Rust supervisor — владелец физических AutoCoder-owned child processes;
- Provider Runtime — владелец AI-provider semantics, но не OS process ownership desktop runtime;
- Council Engine — владелец deliberation/topology/position semantics, но не фактической истины о выполнении tools;
- Project Intelligence — владелец нормализованного project knowledge/context, но не конкретный parser/LSP/indexer;
- Workspace Transaction — владелец фактического применения AI-изменений;
- Editor отображает workspace, но не заменяет его;
- Tool Runtime исполняет capability, но не объявляет semantic completion задачи;
- Persistence хранит факты/state, но не решает business transitions;
- UI отправляет intents и отображает state, но не является главным orchestration engine;
- Diagnostics наблюдает систему, но не участвует в принятии business decisions;
- interoperability adapters переводят внешние protocols в AutoCoder contracts, но не определяют внутреннюю архитектуру;
- сторонняя библиотека не должна становиться скрытым владельцем доменной семантики AutoCoder;
- capability registry описывает доступные возможности, но не задаёт искусственный whitelist возможностей AI;
- autonomy/approval policy управляет разрешением выполнения, но не должна обеднять архитектурный toolbox ядра.

Если новая функция нарушает эти границы, сначала пересматривается архитектура, а не добавляется ещё одна локальная защита.

---

## 30. Проектная память и состояние

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

## 31. Принципы работы над AutoCoder

- Фактический repository state важнее памяти модели.
- Подтверждённое поведение Windows build важнее предположений.
- Технические API и возможности быстро меняющихся инструментов проверяются по актуальной официальной документации.
- Нельзя выдумывать API, параметры или элементы интерфейса.
- AI-разработчик имеет свободу использовать любые доступные инструменты, методы, сеть, dependencies, tests и архитектурные подходы, если они помогают лучше выполнить задачу и находятся в пределах фактической среды и пользовательских границ.
- Нельзя исправлять неизвестную причину по последовательности догадок, если можно сначала получить фактическую диагностику.
- При повторяющемся классе багов нужно искать отсутствующий архитектурный механизм, а не бесконечно добавлять локальные исключения.
- Поэтапная миграция предпочтительна, когда она снижает риск и улучшает проверяемость; большой связный rewrite/refactor допустим, если фактический анализ показывает, что он является лучшим или необходимым решением.
- Существующий рабочий механизм не является неприкосновенным: его нужно сохранить, перенести, переработать или заменить исходя из фактической пользы и целевой архитектуры.
- Один законченный технический пакет должен решать связанный архитектурный результат, а не только ближайший симптом; его размер не ограничивается искусственно.
- При проектировании новой внутренней функции изучаются зрелые внешние реализации/стандарты, после чего свободно выбирается лучший вариант: собственный модуль, сменная внешняя implementation через adapter, direct isolated dependency или hybrid.
- COMSOL остаётся финишной специализацией после универсального ядра.

---

## 32. Открытые архитектурные вопросы после фиксации памяти

После принятия этого PROJECT_MEMORY отдельно спроектировать и проверить на фактическом repository state:

1. точную семантику `Participant` в Model Council;
2. сложную настраиваемую схему передачи информации между командами, уровнями и раундами;
3. формальную модель `Position`, Position Stability Analysis, diversity и calibration signals;
4. хранение deliberation data между Execution Ledger, Diagnostics и persistent stores;
5. точный durable-step contract, idempotency/reconciliation semantics и связь с replay;
6. точный migration order от текущей архитектуры к целевой без обязательства сохранять искусственные промежуточные ограничения и без переписывания проекта с нуля, если полный rewrite не окажется фактически лучшим решением;
7. критерии и механизмы автоматического architecture/diagnostics coverage discovery;
8. границы Project Intelligence adapters и минимальный собственный normalized fact model;
9. какие interoperability adapters реально нужны первыми и какой минимальный поднабор каждого протокола поддерживать;
10. dependency/replacement map: какие текущие и будущие third-party components остаются сменными реализациями, а какие действительно оправданно считать частью platform stack;
11. точную модель capability discovery + authorization, включая Full Autonomy без hardcoded AI-tool whitelist;
12. механизм свободного выбора и расширения verification/test capabilities самим AutoCoder.
