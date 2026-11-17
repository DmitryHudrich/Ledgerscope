# Ledgerscope

Анализ транзакций EVM и Tron: граф адресов, метки и классификация акторов, плюс пассивный TLS-наблюдатель. Rust workspace и фронтенд на React.

## Структура

- `crates/domain` (`domain`): чистые типы: адреса, блоки, транзакции, теги, граф, аккаунты. Без IO.
- `crates/application` (`application`): use-cases и порты (`eth/ports.rs`, `account/ports.rs`), сканирование, сборка графа, классификатор, теги (`eth/tags/*`).
- `crates/adapters` (`adapters`): реализации портов: RPC-пул и fleet (`eth/rpc/*`), ClickHouse, Redis, Tron, chainlist, Postgres для аккаунтов. Фича `aws-public` подключает parquet/arrow. Миграции sqlx в `crates/adapters/migrations`.
- `crates/web-api` (бинарь `web-api`): axum, OpenAPI через utoipa, роуты в `src/routes/*`, DTO в `src/dto/*`, конфиг с интерполяцией `${VAR:-default}`.
- `crates/deeptls` (`deeptls`): классификатор TLS-трафика (kNN, фичи, симулятор).
- `crates/observer` (бинарь `observer`): SNI passthrough, http/socks-прокси, pcap-сниффер за фичей `pcap`.
- `frontend/`: React 19 + Vite + Tailwind, граф на d3-force и canvas. Dev-сервер проксирует `/api/*` на `127.0.0.1:3000`.
- `infra/`: docker compose (api, web/Caddy, clickhouse, postgres, redis, devnet-профили). Подробности в `infra/README.md`.

Зависимости слоёв идут только внутрь: `web-api → adapters → application → domain`. Domain и application не знают про конкретные БД и RPC.

## Команды

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p ledgerscope-web-api
cargo run -p ledgerscope-observer -- --config observer.yaml

cd frontend && npm run typecheck && npm run build
```

Смоук-тесты в `crates/adapters/tests` ходят во внешние сервисы только при заданных `LEDGERSCOPE_TEST_*` переменных (ClickHouse, Redis, Postgres, AWS public), иначе пропускаются.

## Конфиг и секреты

- `config.yaml` и `observer.yaml` локальные и могут содержать ключи RPC. Шаблоны: `config.example.yaml`, `observer.example.yaml`. Новые поля добавлять в шаблон.
- `infra/.env*` в gitignore, туда же секреты compose.
- Ключи, пароли и URL с токенами не логировать и не коммитить.

## Правила

- Никаких комментариев в коде, включая doc-комментарии, если пользователь не попросил явно.
- У каждой метки и тега обязателен источник (провенанс): эвристика, провайдер, пользователь или агент.
- Суммы держать в `U256`/`u128`, без `f64` в расчётах. Конвертация в float только для отображения.
- Данные из RPC и сети считаются недоверенными: никаких `unwrap`/`expect`/`panic!` на их разборе.
- Ошибки: `anyhow` в бинарях, типизированные ошибки на границах портов.

## Агенты

Сабагенты лежат в `.claude/agents/`:

- `explorer`: найти, где в workspace обрабатывается X.
- `check-runner`: прогнать fmt, clippy и тесты и вернуть только ошибки. Использовать вместо прямого запуска cargo, чтобы не тащить вывод в контекст.
- `rust-reviewer`: ревью диффа на корректность Rust-кода.
- `security-auditor`: секреты, недоверенный ввод, зависимости, XSS во фронтенде.
