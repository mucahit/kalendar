use kalendar_core::Event;

const MEETING_HOSTS: &[&str] = &[
    "8x8.vc",
    "chime.aws",
    "hangouts.google.com",
    "meet.google.com",
    "teams.live.com",
    "teams.microsoft.com",
    "teams.microsoft.us",
    "whereby.com",
    "zoom.us",
];

#[must_use]
pub(crate) fn description(event: &Event) -> Option<String> {
    let notes = event.notes.as_deref()?;
    let parsed = plain_text(notes);
    (!parsed.is_empty()).then_some(parsed)
}

#[must_use]
pub(crate) fn meeting_url(event: &Event) -> Option<String> {
    let event_url = event.url.as_deref().and_then(clean_url);
    let description_urls = event.notes.as_deref().map(urls_in).unwrap_or_default();
    let location_urls = event.location.as_deref().map(urls_in).unwrap_or_default();

    event_url
        .iter()
        .chain(description_urls.iter())
        .chain(location_urls.iter())
        .find(|url| is_meeting_url(url))
        .cloned()
        .or(event_url)
}

fn plain_text(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let unescaped = unescape_ical_text(&normalized);
    let text = if looks_like_html(&unescaped) {
        strip_html(&unescaped)
    } else {
        decode_html_entities(&unescaped)
    };
    tidy_lines(&text)
}

fn looks_like_html(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "<a ", "<body", "<br", "<div", "<html", "<li", "<ol", "<p", "<span", "<table", "<ul",
    ]
    .iter()
    .any(|tag| lower.contains(tag))
}

fn strip_html(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        result.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('>') else {
            result.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let raw_tag = after_start[..end].trim().to_ascii_lowercase();
        let tag = raw_tag.trim_start_matches('/');
        let name = tag
            .split(|character: char| character.is_ascii_whitespace() || character == '/')
            .next()
            .unwrap_or_default();
        if !name.starts_with('!')
            && !name.starts_with('?')
            && !name.starts_with(char::is_alphabetic)
        {
            result.push('<');
            rest = after_start;
            continue;
        }
        if matches!(
            name,
            "br" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "p" | "tr"
        ) {
            push_newline(&mut result);
        }
        if name == "li" && !raw_tag.starts_with('/') {
            result.push_str("• ");
        }
        rest = &after_start[end + 1..];
    }
    result.push_str(rest);
    decode_html_entities(&result)
}

fn push_newline(value: &mut String) {
    if !value.is_empty() && !value.ends_with('\n') {
        value.push('\n');
    }
}

fn unescape_ical_text(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.peek().copied() {
            Some('n' | 'N') => {
                characters.next();
                result.push('\n');
            }
            Some(',' | ';' | '\\') => {
                result.push(characters.next().unwrap_or_default());
            }
            _ => result.push(character),
        }
    }
    result
}

fn decode_html_entities(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        result.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find(';').filter(|end| *end <= 12) else {
            result.push('&');
            rest = after_start;
            continue;
        };
        let entity = &after_start[..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "apos" => Some('\''),
            "gt" => Some('>'),
            "lt" => Some('<'),
            "nbsp" => Some(' '),
            "quot" => Some('"'),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|digits| u32::from_str_radix(digits, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|digits| digits.parse().ok())
                })
                .and_then(char::from_u32),
        };
        if let Some(character) = decoded {
            result.push(character);
        } else {
            result.push('&');
            result.push_str(entity);
            result.push(';');
        }
        rest = &after_start[end + 1..];
    }
    result.push_str(rest);
    result
}

fn tidy_lines(value: &str) -> String {
    let mut lines = Vec::new();
    let mut previous_was_empty = true;
    for line in value.lines() {
        let line = line.trim();
        let is_empty = line.is_empty();
        if !is_empty || !previous_was_empty {
            lines.push(line);
        }
        previous_was_empty = is_empty;
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn urls_in(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let lower = value.to_ascii_lowercase();
    let mut offset = 0;
    while offset < value.len() {
        let remaining = &lower[offset..];
        let Some(relative_start) = ["https://", "http://", "zoommtg://", "msteams://"]
            .iter()
            .filter_map(|scheme| remaining.find(scheme))
            .min()
        else {
            break;
        };
        let start = offset + relative_start;
        let end = value[start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | '<' | '>')
            })
            .map_or(value.len(), |end| start + end);
        if let Some(url) = clean_url(&value[start..end]) {
            result.push(url);
        }
        offset = end.max(start + 1);
    }
    result
}

fn clean_url(value: &str) -> Option<String> {
    let decoded = decode_html_entities(value.trim());
    let cleaned =
        decoded.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'']);
    let lower = cleaned.to_ascii_lowercase();
    ["https://", "http://", "zoommtg://", "msteams://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        .then(|| cleaned.to_owned())
}

fn is_meeting_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("zoommtg://") || lower.starts_with("msteams://") {
        return true;
    }
    let Some((scheme, remainder)) = lower.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_end_matches('.');
    MEETING_HOSTS
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        || (host == "app.slack.com" && remainder.contains("/huddle/"))
        || (host.ends_with(".webex.com") && remainder.contains("/meet/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use kalendar_core::{CalendarBackend, DateRange, MockBackend, local_at};

    async fn event() -> Event {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        MockBackend::demo(date)
            .events(DateRange::new(
                local_at(date - chrono::Duration::days(7), 0, 0),
                local_at(date + chrono::Duration::days(7), 0, 0),
            ))
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    }

    #[tokio::test]
    async fn parses_html_entities_breaks_and_ical_escapes() {
        let mut event = event().await;
        event.notes = Some(
            "<p>Agenda &amp; context</p><p>First item\\nSecond item</p><div>2 < 3 and 5 > 4 &copy; Done &#x2713;</div><ul><li>Next</li></ul>"
                .into(),
        );
        assert_eq!(
            description(&event).as_deref(),
            Some(
                "Agenda & context\nFirst item\nSecond item\n2 < 3 and 5 > 4 &copy; Done ✓\n• Next"
            )
        );
    }

    #[tokio::test]
    async fn finds_meeting_urls_in_descriptions_and_decodes_query_entities() {
        let mut event = event().await;
        event.url = None;
        event.notes = Some(
            r#"<a href="https://teams.microsoft.com/l/meetup-join/abc?x=1&amp;y=2">Join the meeting</a>"#
                .into(),
        );
        assert_eq!(
            meeting_url(&event).as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/abc?x=1&y=2")
        );
    }

    #[tokio::test]
    async fn prefers_a_recognized_description_meeting_over_a_generic_event_url() {
        let mut event = event().await;
        event.url = Some("https://example.com/agenda".into());
        event.notes = Some("Join at https://meet.google.com/abc-defg-hij".into());
        assert_eq!(
            meeting_url(&event).as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
    }

    #[tokio::test]
    async fn finds_meeting_urls_in_locations() {
        let mut event = event().await;
        event.url = None;
        event.location = Some("Zoom: https://us02web.zoom.us/j/123456789".into());
        assert_eq!(
            meeting_url(&event).as_deref(),
            Some("https://us02web.zoom.us/j/123456789")
        );
    }
}
