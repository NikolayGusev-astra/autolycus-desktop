// src-tauri/src/meeting_briefing.rs
// L8: Pre-meeting briefing for calendar events.
//
// Extends the ADR-008 Smart Briefing idea with a *meeting-specific* entry
// point: instead of a global 7-day dashboard digest, this gathers context
// for ONE upcoming meeting — the meeting source (organizer), attendees,
// the engineer's active tasks, and past chat sessions — then asks the Hermes
// agent (LLM) to produce a focused briefing.
//
// Reuses the same WS transport path as generate_smart_briefing_cmd
// (connect-per-message, source="meeting_briefing" so it never loops).

use serde::{Deserialize, Serialize};

/// Result of a meeting briefing generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingBriefingResult {
    pub event_uid: String,
    /// Type classifier result: "daily" | "customer" | "other".
    pub meeting_type: String,
    /// The generated briefing text (agent's reply).
    pub briefing_text: String,
    /// Meeting title (echoed back for the UI card).
    pub summary: String,
    /// The session_id of the briefing chat (for linking/audit).
    pub session_id: String,
}

/// Classify a meeting as daily / customer / other from its summary + organizer.
/// Pure function — easy to unit test without the Hermes backend.
pub fn classify_meeting(summary: &str, organizer: &str) -> &'static str {
    let s = summary.to_lowercase();
    let o = organizer.to_lowercase();
    // Daily / standup / scrum / утренний статус.
    if s.contains("daily") || s.contains("standup") || s.contains("scrum")
        || s.contains("дейли") || s.contains("стендап") || s.contains("утренний статус")
        || s.contains("статус-митинг")
    {
        return "daily";
    }
    // Customer-facing: organizer is external (not our corp domain) OR title
    // mentions a customer / заказчик / встреча по проекту.
    let external = o.contains("@") && !o.ends_with("@corp.ru") && !o.ends_with("@rupost.ru");
    if external
        || s.contains("заказчик")
        || s.contains("клиент")
        || s.contains("customer")
        || s.contains("demo")
        || s.contains("презент")
        || s.contains("встреча по проект")
    {
        return "customer";
    }
    "other"
}

/// Build the meeting briefing prompt sent to the agent.
///
/// `related_task_titles` and `related_session_previews` are pre-fetched by the
/// caller (from productivity.db / state.db) and injected so the agent has the
/// concrete engineer context without re-querying everything itself.
pub fn meeting_briefing_prompt(
    summary: &str,
    description: &str,
    organizer: &str,
    attendees: &[String],
    meeting_type: &str,
    related_task_titles: &[String],
    related_session_previews: &[String],
) -> String {
    let attendee_list = if attendees.is_empty() {
        "(нет данных об участниках)".to_string()
    } else {
        attendees.join(", ")
    };
    let tasks_block = if related_task_titles.is_empty() {
        "(нет активных задач, связанных с этой темой)".to_string()
    } else {
        related_task_titles
            .iter()
            .map(|t| format!("- {}", t))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let sessions_block = if related_session_previews.is_empty() {
        "(нет связанных прошлых сессий)".to_string()
    } else {
        related_session_previews
            .iter()
            .map(|s| format!("- {}", s))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let focus = match meeting_type {
        "daily" => {
            "Это ДЕЙЛИ-статус. Сделай упор на: мои активные задачи и их статусы, \
             что было проделано (по прошлым сессиям), блокеры, что сказать на статусе. \
             Кратко — формат «что сделал / что делаю / блокеры»."
        }
        "customer" => {
            "Это ВСТРЕЧА ПО ЗАКАЗЧИКУ. Сделай упор на: задачи, связанные с этим \
             заказчиком/проектом, их статусы и открытые вопросы, что подготовить \
             к встрече, какие решения нужны. Найди задачи по названию заказчика/проекта."
        }
        _ => "Сделай краткий бриф по встрече: контекст, что обсудить, статусы задач по теме.",
    };

    format!(
        r#"Ты — персональный ИИ-ассистент руководителя. Подготовь КРАТКИЙ брифинг перед встречей.

ВСТРЕЧА:
- Название: {summary}
- Организатор (источник): {organizer}
- Участники: {attendees}
- Описание/повестка: {description}

КОНТЕКСТ (уже собран):
Мои активные задачи по теме:
{tasks}

Прошлые сессии по теме:
{sessions}

ФОКУС БРИФИНГА: {focus}

Шаги:
1. Проанализируй задачи и сессии выше.
2. Сформулируй: что обсудить, статусы по задачам, открытые вопросы, что подготовить.
3. Для встреч по заказчику — найди задачи, связанные с этим заказчиком (по названию в задачах).

Формат: деловой стиль, русский, **жирный** для имён и статусов. Без длинных полотен.
Если данных нет — честно пиши «нет данных»."#,
        summary = summary,
        organizer = organizer,
        attendees = attendee_list,
        description = if description.trim().is_empty() {
            "(повестка не указана)"
        } else {
            description
        },
        tasks = tasks_block,
        sessions = sessions_block,
        focus = focus,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_meeting_detects_daily() {
        assert_eq!(classify_meeting("Daily standup", "boss@corp.ru"), "daily");
        assert_eq!(classify_meeting("Дейли по проекту", ""), "daily");
        assert_eq!(classify_meeting("Утренний статус", ""), "daily");
        assert_eq!(classify_meeting("SCRUM", "x@corp.ru"), "daily");
    }

    #[test]
    fn classify_meeting_detects_customer() {
        assert_eq!(classify_meeting("Встреча с заказчиком Acme", "client@acme.com"), "customer");
        assert_eq!(classify_meeting("Demo для клиента", "x@corp.ru"), "customer");
        assert_eq!(classify_meeting("Презентация проекта", ""), "customer");
        assert_eq!(classify_meeting("1:1", "external@other.com"), "customer");
    }

    #[test]
    fn classify_meeting_falls_back_to_other() {
        assert_eq!(classify_meeting("Обед с командой", "me@corp.ru"), "other");
        assert_eq!(classify_meeting("Ретроспектива", ""), "other");
    }

    #[test]
    fn meeting_briefing_prompt_includes_context() {
        let prompt = meeting_briefing_prompt(
            "Daily standup",
            "Quick sync",
            "boss@corp.ru",
            &["alice@corp.ru".to_string()],
            "daily",
            &["DEVOS-3 Настроить MCP".to_string()],
            &["Сессия — обсудили MCP".to_string()],
        );
        assert!(prompt.contains("Daily standup"));
        assert!(prompt.contains("boss@corp.ru"));
        assert!(prompt.contains("alice@corp.ru"));
        assert!(prompt.contains("DEVOS-3 Настроить MCP"));
        assert!(prompt.contains("Сессия — обсудили MCP"));
        assert!(prompt.contains("ДЕЙЛИ"));
    }
}
