// src-tauri/src/briefing.rs
// Briefing: sends an analysis prompt to the running Hermes Agent via the
// WebSocket /api/ws transport (ADR-004). The agent has direct access to all
// connected sources (email via himalaya MCP, Jira MCP, Telegram, chat
// sessions in state.db) and the LLM for reasoning — no separate Python
// briefing server needed.
//
// This replaces the old mcp-smart-briefing/server.py subprocess approach,
// which spawned additional email/jira MCP children (losing credentials),
// read the wrong DB path, and had zero LLM analysis.

use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::config::profile_home;
use crate::mcp::read_mcp_servers_yaml;
use crate::sessions::state_db_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingResult {
    pub session_id: String,
    pub source: String,
    pub started_at: i64,
    pub title: String,
    pub preview: String,
    pub formatted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedBriefing {
    pub session_id: String,
    pub title: String,
    pub text: String,
    /// When the briefing was generated (unix epoch seconds). Matches the
    /// `generated_at` field the frontend expects — do NOT rename (serde field
    /// name is the wire contract).
    pub generated_at: i64,
    /// True when the cached briefing is older than max_age_secs.
    pub stale: bool,
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The briefing prompt sent to the agent. It names only the MCP sources that
/// are configured and enabled for this profile; the tool registry remains the
/// source of truth for exact available tools.
///
/// The structure follows the user's reference Working Briefing format:
/// statistics line + sections for configured sources, tasks, and task↔chat
/// cross-linking + suggested replies.
pub fn briefing_prompt(hermes_home: &Path, profile: Option<&str>, days: i64) -> String {
    let mut configured_sources: Vec<_> = read_mcp_servers_yaml(hermes_home, profile)
        .into_iter()
        .filter(|(_, server)| server.enabled.unwrap_or(true))
        .map(|(name, _)| name)
        .collect();
    configured_sources.sort();
    let source_list = if configured_sources.is_empty() {
        "- Нет настроенных MCP-источников.".to_string()
    } else {
        configured_sources
            .iter()
            .map(|name| format!("- {}", name))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"Ты — персональный ИИ-ассистент руководителя. Подготовь «Рабочий брифинг» за последние {days} дней.

Настроенные MCP-источники:
{source_list}

ШАГ 1. Собери сырые данные только из перечисленных выше источников. Используй только инструменты, которые есть в твоём реестре: если инструмента или источника нет в реестре, он недоступен. Не пытайся вызывать несуществующие инструменты и не повторяй вызовы, вернувшие ошибку «not found». Проанализируй также доступные локальные задачи и недавние чат-сессии.

ШАГ 2. Перекрёстный анализ задач и обсуждений:
- Если задача выполнена/обсуждена в чате, но статус в трекере не обновлён → предложи обновить статус.
- Если в задаче есть статус, а в подключённом источнике вопрос не решён и нет ответа → предложи готовый краткий ответ.

ШАГ 3. Сформируй брифинг СТРОГО по структуре ниже. Краткие выжимки, без длинных полотен.

# Рабочий брифинг
**[День недели, Дата]**

📊 **Статистика дня:** Встреч: [N] | Задач в работе: [N] | Просрочено/Без движения: [N]

## ДАННЫЕ ИЗ ПОДКЛЮЧЁННЫХ ИСТОЧНИКОВ
* [Источник]: [краткая выжимка только из доступных данных].

## БЛОКЕРЫ ЗАДАЧ
* [ID задачи] — [Название задачи]. Причина блокировки: [Описание]. Кто блокирует: [Имя].

## БЕЗ ДВИЖЕНИЯ
* [ID задачи] — [Название задачи] (Просрочено на [X] дней / Без движения [X] дней).

## ЗАДАЧИ — СВЯЗЬ С ОБСУЖДЕНИЯМИ
* [ID задачи]: статус [статус]. Обсуждалась в чате: да/нет.
  - Если выполнена в чате, но статус не обновлён → предложи обновить.
  - Если ждёт ответа, но ответа не было → предложи, что ответить.

## ПРЕДЛОЖЕНИЯ ОТВЕТОВ
* На [запрос из подключённого источника]: [краткий готовый ответ в 1–2 предложениях].

## ОБЩАЯ ГОТОВНОСТЬ
* Выполнено: N. В работе: M. Требует внимания: K.

ПРАВИЛА ФОРМАТИРОВАНИЯ:
1. Строгий деловой стиль, на русском языке.
2. Выделяй **жирным** имена, статусы («Ждёт вас», «Мяч у вас», «БЕЗ ДВИЖЕНИЯ») и критичные проблемы.
3. Если в блоке нет данных — выводи: «Нет критичных обновлений».
4. Если источник недоступен (нет подключения, нет MCP, нет данных) — честно укажи «Источник недоступен: [причина]». НЕ ВЫДУМЫВАЙ данные.
5. Указывай ID задач (например, PRX-123, AD-62485).
6. Предложения ответов — краткие, готовые к отправке."#,
        days = days,
        source_list = source_list
    )
}

/// Read the most recent cached briefing from state.db.
pub fn get_cached_briefing(
    hermes_home: &Path,
    profile: Option<&str>,
    max_age_secs: f64,
) -> CachedBriefing {
    let session_id = format!("smart_briefing:{}", profile.unwrap_or("default"));
    let profile_path = profile_home(hermes_home, profile);
    let db_path = state_db_path(&profile_path, None);

    let now = now_ts();
    let mut text = String::new();
    let mut title = String::new();
    let mut generated_at = 0i64;

    if db_path.exists() {
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            if let Ok(row) = conn.query_row(
                "SELECT title, preview, started_at FROM sessions WHERE id = ?1",
                rusqlite::params![&session_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0).unwrap_or_default(),
                        r.get::<_, String>(1).unwrap_or_default(),
                        r.get::<_, i64>(2).unwrap_or(0),
                    ))
                },
            ) {
                title = row.0;
                text = row.1;
                generated_at = row.2;
            }
        }
    }

    let age = now - generated_at;
    CachedBriefing {
        session_id,
        title,
        text,
        generated_at,
        stale: age as f64 > max_age_secs,
    }
}

// ── state.db session insert ───────────────────────────────────────────────
// (Removed: insert_briefing_session was only used by the dead
//  generate_smart_briefing; the live path in lib.rs does its own
//  INSERT OR REPLACE directly into the sessions table.)
