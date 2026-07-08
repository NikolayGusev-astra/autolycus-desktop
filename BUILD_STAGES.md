# Сборка и аудит autolycus-desktop (Штурман Desktop) — журнал этапов

Дата: 2026-07-08
Среда: Windows 10, Node 22, Rust 1.93, cargo-tauri 2.11.4
| Статус: NSIS пересобран (прокси + фиксы A/Б); MSI — заблокирован (нет VC++ runtime). |

Формат: № | Действие | Шаги | Результат | Статус | Комментарий

---

## 1. Сборка под Windows

| № | Действие | Шаги | Результат | Статус | Комментарий |
|---|----------|------|-----------|--------|-------------|
| 1 | Подготовка среды | `node -v; npm -v; rustc -v; cargo tauri -V` | все версии на месте | ✅ | Node 22, Rust 1.93, cargo-tauri 2.11.4 |
| 2 | npm install | `npm install` в корне репо | up to date, 208 пакетов, 0 уязвимостей | ✅ | зависимости уже разрешены |
| 3 | Сборка фронтенда | `npm run build` (tsc + vite) | `dist/` создан | ✅ | warning: chunk >500KB (framer-motion/tiptap) — не критично для тестов |
| 4 | Генерация icon.ico | PIL: 512x512.png → мультиразмер 256/128/96/64/48/32/16 | `src-tauri/icons/icon.ico` 59KB | ✅ | в репо был только .icns/.png — обязателен для Windows-бандла |
| 5 | Бандлеры WiX+NSIS | winget неработоспособен (stdin not a tty) → ручная загрузка zip + распаковка | `tools/nsis/nsis-3.11/makensis.exe`, `tools/wix/{candle,light}.exe` | ✅ | добавлены в PATH текущей сессии |
| 6 | Компиляция Rust-ядра | `cargo build` (src-tauri) | Finished dev, 25.5s | ✅ | только warnings (dead_code) |
| 7 | Релизная сборка ядра | `cargo build --release` | Finished release, 1m11s | ✅ | готовый exe 20MB в target/release |
| 8 | NSIS-инсталлятор | `cargo tauri build --bundles nsis` | `bundle/nsis/Штурман Desktop_3.2.0_x64-setup.exe` 5.3MB | ✅ | ГОТОВ К УСТАНОВКЕ И ТЕСТАМ |
| 9 | MSI-инсталлятор | `cargo tauri build --bundles msi` | ошибка light.exe (tauri-WiX кэш кривой) | ⚠️ | скопировал рабочий WiX в AppData/Local/tauri/WixTools314, но нужен VC++ runtime; доработать отдельно |

## 2. Аудит соединений с бэкендом

| № | Объект | Что проверено | Результат | Статус | Комментарий |
|---|--------|---------------|-----------|--------|-------------|
| 10 | gateway.rs | старт gateway, прокидывание API_SERVER_PORT, health-check (TCP+HTTP, 30s) | порт синхронизирован через env | ✅ | риск: если Hermes игнорирует API_SERVER_PORT и берёт порт из config.yaml — рассинхрон портов |
| 11 | chat.rs | Local mode → всегда через gateway HTTP API (`send_message_via_api` на get_api_url) | корректно | ✅ | требует запущенного gateway; иначе чат молчит |
| 12 | discovery.rs | поиск python/hermes/venv/gateway/версия | логика рабочая | ⚠️ | зависит от venv-раскладки; если Hermes глобально (не venv) — discovery может не найти бэкенд для Local mode |
| 13 | Прокси для моделей | per-connector SOCKS5 (use_proxy + proxy_url); пишется в `proxy:` блок config.yaml, читается обратно; reqwest Client::proxy(Proxy::all) в telegram/chat/remote/ssh, fallback без прокси при ошибке | реализовано в коде | ✅ DONE | UI-галочка в ModelsTab + блок прокси у источников; дефолт socks5://127.0.0.1:12334, fallback env HTTP_PROXY/HTTPS_PROXY |

## 3. Найденные баги (брифинг)

| № | Баг | Корень | Локация | Статус | Комментарий |
|---|-----|--------|---------|--------|-------------|
| 14 | А: рекурсия брифинга | `generateBriefing` шлёт `session_id: null` → бэкенд плодит сессии `desk-<uuid>` → они попадают в `list_feed_cmd` (sessions.rs берёт ВСЕ сессии без фильтра) → следующий брифинг берёт их в контекст | FeedView.tsx:138 + sessions.rs:67-77 | ✅ ИСПРАВЛЕН | фронт: session_id=`briefing:<key>`; бэкенд: `list_feed` и `list_sessions` исключают `id LIKE 'briefing:%'` |
| 15 | Б: брифинг игнорит задачи/источники | `generateBriefing` тянет только `items` (голые сессии), НЕ вызывает list_tasks/list_projects/list_goals; берёт `items.filter(started_at >= weekAgo)` = N последних сессий по времени | FeedView.tsx:97-146 | ✅ ИСПРАВЛЕН | брифинг параллельно тянет list_tasks/list_projects/list_goals + собирает контекст; промпт переписан под агрегацию |

## 4. План фиксов (после тестирования)

| № | Фикс | Подход | Статус |
|---|------|--------|--------|
| 16 | Баг А | `session_id: null` → `briefing:<key>`; в sourceItems исключать `session_id.startsWith("briefing:")` | ✅ DONE | фронт + бэкенд (sessions.rs исключает `briefing:%`) |
| 17 | Баг Б | параллельно дотянуть list_tasks_cmd/list_projects_cmd/list_goals_cmd + полнотекст источников; переписать промпт брифинга | ✅ DONE | брифинг агрегирует задачи/проекты/цели + сессии |
| 18 | Proxy for connectors | SOCKS5 checkbox + URL per Telegram/Email/Jira source in SettingsPanel.tsx | ✅ DONE | `proxy_url?: string` в интерфейсах + checkbox в fields + proxy_url в sources.rs структурах, backend читает proxy_url из source при отправке |
| 19 | Registry proxy optional | MCP catalog fetch через SOCKS5 | ⏭ SKIPPED | catalog доступен прямой, registry.rs пока без прокси |
| 19 | MSI | поставить VC++ runtime / корректный WiX, пересобрать msi | ⏳ | заблокировано: tauri-WiX кэш требует VC++ runtime; оставить отдельной задачей |

---
Готово к тестированию: `C:\Users\n.gusev\ZCodeProject\autolycus-desktop\src-tauri\target\release\bundle\nsis\Штурман Desktop_3.2.0_x64-setup.exe`
