use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

/// Template engine for variable substitution
pub struct TemplateEngine {
    regex: Regex,
}

impl TemplateEngine {
    pub fn new() -> Self {
        TemplateEngine {
            regex: Regex::new(r"\{\{([^}]+)\}\}").unwrap(),
        }
    }

    /// Render template with context variables
    pub fn render(
        &self,
        template: &str,
        params: &HashMap<String, Value>,
        env: &HashMap<String, String>,
        user: Option<&Value>,
        response_body: Option<&Value>,
        response_headers: Option<&HashMap<String, String>>,
    ) -> String {
        self.regex
            .replace_all(template, |caps: &regex::Captures| {
                let variable = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                self.resolve_variable(variable, params, env, user, response_body, response_headers)
            })
            .to_string()
    }

    /// Resolve a variable path (e.g., "params.city", "env.API_KEY", "user.name")
    fn resolve_variable(
        &self,
        path: &str,
        params: &HashMap<String, Value>,
        env: &HashMap<String, String>,
        user: Option<&Value>,
        response_body: Option<&Value>,
        response_headers: Option<&HashMap<String, String>>,
    ) -> String {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return String::new();
        }

        match parts[0].trim() {
            "params" => {
                if parts.len() < 2 {
                    return String::new();
                }
                let param_name = parts[1].trim();
                params
                    .get(param_name)
                    .and_then(|v| self.extract_nested_value(v, &parts[2..]))
                    .unwrap_or_default()
            }
            "env" => {
                if parts.len() < 2 {
                    return String::new();
                }
                let env_name = parts[1].trim();
                env.get(env_name).cloned().unwrap_or_default()
            }
            "user" => {
                if let Some(user_obj) = user {
                    if parts.len() < 2 {
                        return serde_json::to_string(user_obj).unwrap_or_default();
                    }
                    self.extract_nested_value(user_obj, &parts[1..])
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            }
            "body" => {
                if let Some(body) = response_body {
                    if parts.len() < 2 {
                        return serde_json::to_string(body).unwrap_or_default();
                    }
                    self.extract_nested_value(body, &parts[1..])
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            }
            "headers" => {
                if let Some(headers) = response_headers {
                    if parts.len() < 2 {
                        return String::new();
                    }
                    let header_name = parts[1].trim();
                    headers.get(header_name).cloned().unwrap_or_default()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        }
    }

    /// Extract nested value from JSON using path segments
    fn extract_nested_value(&self, value: &Value, path: &[&str]) -> Option<String> {
        if path.is_empty() {
            return Some(value_to_string(value));
        }

        let mut current = value;
        for segment in path {
            let segment = segment.trim();

            // Handle array indexing (e.g., results[0])
            if let Some(idx_pos) = segment.find('[') {
                let key = &segment[..idx_pos];
                let idx_str = segment[idx_pos + 1..].trim_end_matches(']');

                if !key.is_empty() {
                    current = current.get(key)?;
                }

                if let Ok(idx) = idx_str.parse::<usize>() {
                    current = current.get(idx)?;
                }
            } else {
                current = current.get(segment)?;
            }
        }

        Some(value_to_string(current))
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert JSON value to string representation
fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_param_substitution() {
        let engine = TemplateEngine::new();
        let mut params = HashMap::new();
        params.insert("city".to_string(), json!("New York"));

        let result = engine.render(
            "Weather in {{params.city}}",
            &params,
            &HashMap::new(),
            None,
            None,
            None,
        );

        assert_eq!(result, "Weather in New York");
    }

    #[test]
    fn test_env_variable() {
        let engine = TemplateEngine::new();
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "secret123".to_string());

        let result = engine.render(
            "Bearer {{env.API_KEY}}",
            &HashMap::new(),
            &env,
            None,
            None,
            None,
        );

        assert_eq!(result, "Bearer secret123");
    }

    #[test]
    fn test_user_context() {
        let engine = TemplateEngine::new();
        let user = json!({
            "id": "123",
            "name": "John Doe",
            "email": "john@example.com"
        });

        let result = engine.render(
            "User: {{user.name}} ({{user.email}})",
            &HashMap::new(),
            &HashMap::new(),
            Some(&user),
            None,
            None,
        );

        assert_eq!(result, "User: John Doe (john@example.com)");
    }

    #[test]
    fn test_nested_body_value() {
        let engine = TemplateEngine::new();
        let body = json!({
            "main": {
                "temp": 25.5
            }
        });

        let result = engine.render(
            "Temperature: {{body.main.temp}}°C",
            &HashMap::new(),
            &HashMap::new(),
            None,
            Some(&body),
            None,
        );

        assert_eq!(result, "Temperature: 25.5°C");
    }

    #[test]
    fn test_array_indexing() {
        let engine = TemplateEngine::new();
        let body = json!({
            "results": [
                {"title": "First Result"},
                {"title": "Second Result"}
            ]
        });

        let result = engine.render(
            "{{body.results[0].title}}",
            &HashMap::new(),
            &HashMap::new(),
            None,
            Some(&body),
            None,
        );

        assert_eq!(result, "First Result");
    }

    #[test]
    fn test_multiple_substitutions() {
        let engine = TemplateEngine::new();
        let mut params = HashMap::new();
        params.insert("city".to_string(), json!("London"));

        let mut env = HashMap::new();
        env.insert("UNITS".to_string(), "metric".to_string());

        let result = engine.render(
            "Weather for {{params.city}} in {{env.UNITS}} units",
            &params,
            &env,
            None,
            None,
            None,
        );

        assert_eq!(result, "Weather for London in metric units");
    }
}
