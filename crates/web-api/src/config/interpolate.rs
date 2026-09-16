use anyhow::{anyhow, bail};
use serde_yaml::{Mapping, Value};

type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

pub fn resolve(value: Value) -> anyhow::Result<Value> {
    resolve_with(value, &|name| std::env::var(name).ok())
}

fn resolve_with(value: Value, env: Lookup<'_>) -> anyhow::Result<Value> {
    match value {
        Value::String(text) => scalar(&text, env),
        Value::Sequence(items) => items
            .into_iter()
            .map(|item| resolve_with(item, env))
            .collect::<anyhow::Result<Vec<Value>>>()
            .map(Value::Sequence),
        Value::Mapping(mapping) => {
            let mut resolved = Mapping::with_capacity(mapping.len());
            for (key, value) in mapping {
                let value = match key.as_str() {
                    Some(name) => {
                        let at = format!("at `{name}`");
                        resolve_with(value, env).map_err(|error| error.context(at))?
                    }
                    None => resolve_with(value, env)?,
                };
                resolved.insert(key, value);
            }
            Ok(Value::Mapping(resolved))
        }
        Value::Tagged(mut tagged) => {
            tagged.value = resolve_with(tagged.value, env)?;
            Ok(Value::Tagged(tagged))
        }
        other => Ok(other),
    }
}

fn scalar(text: &str, env: Lookup<'_>) -> anyhow::Result<Value> {
    let expanded = expand(text, env)?;

    if is_whole_placeholder(text) {
        Ok(retyped(expanded))
    } else {
        Ok(Value::String(expanded))
    }
}

fn is_whole_placeholder(text: &str) -> bool {
    text.starts_with("${") && closing_brace(text, 2).is_some_and(|end| end + 1 == text.len())
}

fn retyped(text: String) -> Value {
    if let Ok(number) = text.parse::<u64>() {
        return Value::from(number);
    }
    if let Ok(number) = text.parse::<i64>() {
        return Value::from(number);
    }

    match text.as_str() {
        "true" => Value::from(true),
        "false" => Value::from(false),
        _ => Value::String(text),
    }
}

fn expand(text: &str, env: Lookup<'_>) -> anyhow::Result<String> {
    let bytes = text.as_bytes();
    let mut expanded = String::with_capacity(text.len());
    let mut cursor = 0;

    while cursor < text.len() {
        let Some(offset) = text[cursor..].find('$') else {
            expanded.push_str(&text[cursor..]);
            break;
        };

        expanded.push_str(&text[cursor..cursor + offset]);
        let dollar = cursor + offset;

        match bytes.get(dollar + 1) {
            Some(b'$') => {
                expanded.push('$');
                cursor = dollar + 2;
            }
            Some(b'{') => {
                let end = closing_brace(text, dollar + 2)
                    .ok_or_else(|| anyhow!("`{text}` leaves a `${{` unclosed"))?;
                expanded.push_str(&placeholder(&text[dollar + 2..end], env)?);
                cursor = end + 1;
            }
            _ => {
                expanded.push('$');
                cursor = dollar + 1;
            }
        }
    }

    Ok(expanded)
}

fn closing_brace(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 1usize;

    for index in from..bytes.len() {
        match bytes[index] {
            b'{' if bytes[index - 1] == b'$' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }

    None
}

fn placeholder(body: &str, env: Lookup<'_>) -> anyhow::Result<String> {
    let (name, fallback) = split(body)?;
    let value = env(name);

    match fallback {
        Fallback::None => match value {
            Some(value) => Ok(value),
            None => bail!("{name} is not set and the config gives it no default"),
        },
        Fallback::WhenUnset(default) => match value {
            Some(value) => Ok(value),
            None => expand(default, env),
        },
        Fallback::WhenEmpty(default) => match value {
            Some(value) if !value.is_empty() => Ok(value),
            _ => expand(default, env),
        },
        Fallback::Required(message) => match value {
            Some(value) if !value.is_empty() => Ok(value),
            _ => bail!("{name} is not set: {}", expand(message, env)?),
        },
    }
}

enum Fallback<'a> {
    None,
    WhenUnset(&'a str),
    WhenEmpty(&'a str),
    Required(&'a str),
}

fn split(body: &str) -> anyhow::Result<(&str, Fallback<'_>)> {
    let Some(operator) = body.find([':', '-', '?']) else {
        return validated(body, Fallback::None);
    };

    let (name, rest) = body.split_at(operator);

    if let Some(default) = rest.strip_prefix(":-") {
        return validated(name, Fallback::WhenEmpty(default));
    }
    if let Some(message) = rest.strip_prefix(":?") {
        return validated(name, Fallback::Required(message));
    }
    if let Some(default) = rest.strip_prefix('-') {
        return validated(name, Fallback::WhenUnset(default));
    }
    if let Some(message) = rest.strip_prefix('?') {
        return validated(name, Fallback::Required(message));
    }

    bail!(
        "`${{{body}}}` uses an unknown operator; expected ${{NAME}}, ${{NAME-default}}, ${{NAME:-default}} or ${{NAME:?message}}"
    )
}

fn validated<'a>(name: &'a str, fallback: Fallback<'a>) -> anyhow::Result<(&'a str, Fallback<'a>)> {
    if name.is_empty() {
        bail!("a placeholder is missing its variable name");
    }

    Ok((name, fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn resolve_yaml(
        yaml: &str,
        pairs: &'static [(&'static str, &'static str)],
    ) -> anyhow::Result<Value> {
        let lookup = env(pairs);
        resolve_with(serde_yaml::from_str(yaml).unwrap(), &lookup)
    }

    #[test]
    fn substitutes_nested_values() {
        let resolved = resolve_yaml(
            "eth:\n  rpc:\n    url: ${ETH_RPC_URL}\n    tags:\n      - ${ENV}-primary\n",
            &[("ETH_RPC_URL", "https://rpc.example/key"), ("ENV", "dev")],
        )
        .unwrap();

        assert_eq!(resolved["eth"]["rpc"]["url"], "https://rpc.example/key");
        assert_eq!(resolved["eth"]["rpc"]["tags"][0], "dev-primary");
    }

    #[test]
    fn whole_placeholder_keeps_the_scalar_type() {
        let resolved = resolve_yaml(
            "rps: ${RPS}\nquiet: ${QUIET}\n",
            &[("RPS", "42"), ("QUIET", "true")],
        )
        .unwrap();

        assert_eq!(resolved["rps"].as_u64(), Some(42));
        assert_eq!(resolved["quiet"].as_bool(), Some(true));
    }

    #[test]
    fn partial_substitution_stays_a_string() {
        let resolved = resolve_yaml("port: prefix-${PORT}\n", &[("PORT", "3000")]).unwrap();

        assert_eq!(resolved["port"], "prefix-3000");
    }

    #[test]
    fn defaults_apply_to_unset_and_empty() {
        let resolved = resolve_yaml(
            "unset: ${MISSING:-fallback}\nempty: ${EMPTY:-fallback}\nkept: ${EMPTY-fallback}\n",
            &[("EMPTY", "")],
        )
        .unwrap();

        assert_eq!(resolved["unset"], "fallback");
        assert_eq!(resolved["empty"], "fallback");
        assert_eq!(resolved["kept"], "");
    }

    #[test]
    fn defaults_nest() {
        let resolved =
            resolve_yaml("url: ${MISSING:-${FALLBACK:-last}}\n", &[("FALLBACK", "")]).unwrap();

        assert_eq!(resolved["url"], "last");
    }

    #[test]
    fn double_dollar_escapes_a_placeholder() {
        let resolved = resolve_yaml("literal: $${NOT_A_VAR}\n", &[]).unwrap();

        assert_eq!(resolved["literal"], "${NOT_A_VAR}");
    }

    #[test]
    fn unset_without_a_default_fails() {
        let error = resolve_yaml("eth:\n  url: ${MISSING}\n", &[]).unwrap_err();

        assert!(format!("{error:#}").contains("MISSING is not set"));
        assert!(format!("{error:#}").contains("eth"));
    }

    #[test]
    fn required_placeholder_reports_its_message() {
        let error = resolve_yaml("url: ${MISSING:?set it in infra/.env}\n", &[]).unwrap_err();

        assert!(format!("{error:#}").contains("set it in infra/.env"));
    }

    #[test]
    fn unclosed_placeholder_fails() {
        let error = resolve_yaml("url: ${MISSING\n", &[]).unwrap_err();

        assert!(format!("{error:#}").contains("unclosed"));
    }
}
