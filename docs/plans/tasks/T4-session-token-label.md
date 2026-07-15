# Задача: T4 — Frontend label «API Key» → «Session Token» для Remote

## Роль
Ты — senior frontend-инженер. Приводишь UI в соответствие с реальным контрактом
(ADR-005). TDD не применим к labels — верификация через tsc + ручной осмотр.

## Контекст (читай перед работой)
- `docs/plans/ADR-005-remote-ssh-ws-contract.md` — Remote/SSH используют
  session token (не API_SERVER_KEY). Поле `remote_api_key` интерпретируется
  как session token.
- Тех-стек: React 19, TypeScript, Zustand, Tauri 2.

## Проблема
`ConnectScreen.tsx:230` — label «API Key» для поля Remote-соединения. Но после
ADR-005 это session token удалённого бэкенда. Пользователь вводит API_SERVER_KEY
(legacy), чат не работает (WS требует session token). Мislaeding label = путаница.

## Root cause
`src/components/ConnectScreen.tsx:230` — `<label>API Key</label>`,
`:232` placeholder «Optional». Статус-сообщение `:206` «check URL and API key».
Контракт ADR-005: поле = session token удалённого `hermes serve`.

## Definition of Done
1. Label → «Session Token» (или локализованный эквивалент через i18n если есть).
2. Placeholder → «Dashboard session token of the remote backend».
3. Статус-сообщение → «check URL and session token».
4. НЕ трогать InstallScreen/OnboardingScreen — там «API Key» = provider key
   (OpenRouter/Gemini), это другой контракт.
5. `npm run build` (tsc) — зелёный.

## Ограничения
- Не меняй логику handleConnect / invoke — только отображаемый текст.
- Не трогай provider-API-key labels (InstallScreen, OnboardingScreen).
- Если есть i18n — добавь ключ, не хардкодь строку.
- Сохраняй стили (className не трогать).
