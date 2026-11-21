//! The assertion vocabulary used by the contract schema.

use super::{TreeError, schema};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The compiled schema patterns, kept across calls: compiling them cost
/// several milliseconds per decode, and the extension host decodes per node.
fn compiled() -> &'static Mutex<HashMap<String, Regex>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn validate(value: &Value, root: &Value) -> Result<(), TreeError> {
    let mut regexes = compiled().lock().map(|m| m.clone()).unwrap_or_default();
    let known = regexes.len();
    let result =
        check(value, root, root, "", &mut regexes).map_err(|(path, reason)| schema(&path, reason));
    if regexes.len() > known
        && let Ok(mut cache) = compiled().lock()
    {
        cache.extend(regexes);
    }
    result
}

type Failure = (String, &'static str);

fn fail(path: &str, reason: &'static str) -> Failure {
    (path.to_string(), reason)
}

fn check(
    value: &Value,
    rule: &Value,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    if let Some(reference) = rule["$ref"].as_str() {
        let Some(target) = reference.strip_prefix("#/") else {
            return Err(fail(path, "external schema reference"));
        };
        let pointer = format!("/{target}");
        let Some(referred) = root.pointer(&pointer) else {
            return Err(fail(path, "missing schema reference"));
        };
        return check(value, referred, root, path, regexes);
    }
    check_base(value, rule, path)?;
    check_combinators(value, rule, root, path, regexes)?;
    check_content(value, rule, root, path, regexes)
}

fn check_base(value: &Value, rule: &Value, path: &str) -> Result<(), Failure> {
    if let Some(expected) = rule["const"].as_str() {
        if value.as_str() != Some(expected) {
            return Err(fail(path, "constant differs"));
        }
    } else if rule.get("const").is_some() && value != &rule["const"] {
        return Err(fail(path, "constant differs"));
    }
    if rule["enum"]
        .as_array()
        .is_some_and(|items| !items.contains(value))
    {
        return Err(fail(path, "value is not in the enum"));
    }
    check_type(value, rule, path)
}

fn check_type(value: &Value, rule: &Value, path: &str) -> Result<(), Failure> {
    let Some(expected) = rule["type"].as_str() else {
        return Ok(());
    };
    let matches = match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(fail(path, "wrong value type"))
    }
}

fn check_content(
    value: &Value,
    rule: &Value,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    if let Some(s) = value.as_str() {
        check_string(s, rule, path, regexes)?;
    }
    if let Some(n) = value.as_f64() {
        check_number(n, rule, path)?;
    }
    if let Some(items) = value.as_array() {
        check_array(items, rule, root, path, regexes)?;
    }
    if let Some(object) = value.as_object() {
        check_object(object, rule, root, path, regexes)?;
    }
    Ok(())
}

fn check_combinators(
    value: &Value,
    rule: &Value,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    for name in ["allOf", "anyOf", "oneOf"] {
        let Some(branches) = rule[name].as_array() else {
            continue;
        };
        check_alternative(value, branches, name, root, path, regexes)?;
    }
    check_conditionals(value, rule, root, path, regexes)
}

fn check_alternative(
    value: &Value,
    branches: &[Value],
    name: &str,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    if name != "allOf"
        && let Some(kind) = value["type"].as_str()
    {
        if let Some(branch) = typed_branch(branches, kind) {
            return check(value, branch, root, path, regexes);
        }
    }
    let (passed, failure) = matching_branches(value, branches, name, root, path, regexes);
    if name == "allOf" && passed != branches.len()
        || name == "anyOf" && passed == 0
        || name == "oneOf" && passed != 1
    {
        return Err(failure.unwrap_or_else(|| fail(path, "schema alternatives do not match")));
    }
    Ok(())
}

fn typed_branch<'a>(branches: &'a [Value], kind: &str) -> Option<&'a Value> {
    branches.iter().find(|branch| {
        branch["$ref"]
            .as_str()
            .is_some_and(|reference| reference.ends_with(&format!("/{kind}")))
    })
}

fn matching_branches(
    value: &Value,
    branches: &[Value],
    name: &str,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> (usize, Option<Failure>) {
    let mut passed = 0;
    let mut failure = None;
    for branch in branches {
        match check(value, branch, root, path, regexes) {
            Ok(()) => {
                passed += 1;
                if name == "anyOf" || name == "oneOf" && passed > 1 {
                    break;
                }
            }
            Err(detail) if failure.is_none() => failure = Some(detail),
            Err(_) => {}
        }
    }
    (passed, failure)
}

fn check_conditionals(
    value: &Value,
    rule: &Value,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    if let Some(negated) = rule.get("not") {
        if check(value, negated, root, path, regexes).is_ok() {
            return Err(fail(path, "forbidden value"));
        }
    }
    if let Some(condition) = rule.get("if") {
        if check(value, condition, root, path, regexes).is_ok() {
            if let Some(consequence) = rule.get("then") {
                check(value, consequence, root, path, regexes)?;
            }
        } else if let Some(otherwise) = rule.get("else") {
            check(value, otherwise, root, path, regexes)?;
        }
    }
    Ok(())
}

fn check_string(
    value: &str,
    rule: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    let chars = value.chars().count();
    if rule["minLength"]
        .as_u64()
        .is_some_and(|n| chars < usize::try_from(n).unwrap_or(usize::MAX))
        || rule["maxLength"]
            .as_u64()
            .is_some_and(|n| chars > usize::try_from(n).unwrap_or(usize::MAX))
    {
        return Err(fail(path, "string length is outside the schema"));
    }
    if let Some(pattern) = rule["pattern"].as_str() {
        if pattern.starts_with("^(?![iI]") {
            if value.eq_ignore_ascii_case("id") || value.eq_ignore_ascii_case("class") {
                return Err(fail(path, "attribute key is reserved"));
            }
            return pattern_match(value, "^[A-Za-z_][A-Za-z0-9_-]*$", path, regexes);
        }
        pattern_match(value, pattern, path, regexes)?;
    }
    Ok(())
}

fn pattern_match(
    value: &str,
    pattern: &str,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    if !regexes.contains_key(pattern) {
        let compiled = Regex::new(pattern).map_err(|_| fail(path, "schema pattern is invalid"))?;
        regexes.insert(pattern.to_string(), compiled);
    }
    if !regexes.get(pattern).is_some_and(|re| re.is_match(value)) {
        return Err(fail(path, "string does not match schema pattern"));
    }
    Ok(())
}

fn check_number(value: f64, rule: &Value, path: &str) -> Result<(), Failure> {
    if rule["minimum"].as_f64().is_some_and(|n| value < n)
        || rule["maximum"].as_f64().is_some_and(|n| value > n)
        || rule["exclusiveMinimum"]
            .as_f64()
            .is_some_and(|n| value <= n)
        || rule["exclusiveMaximum"]
            .as_f64()
            .is_some_and(|n| value >= n)
    {
        return Err(fail(path, "number is outside the schema"));
    }
    Ok(())
}

fn check_array(
    items: &[Value],
    rule: &Value,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    if rule["minItems"]
        .as_u64()
        .is_some_and(|n| items.len() < usize::try_from(n).unwrap_or(usize::MAX))
        || rule["maxItems"]
            .as_u64()
            .is_some_and(|n| items.len() > usize::try_from(n).unwrap_or(usize::MAX))
    {
        return Err(fail(path, "array length is outside the schema"));
    }
    if rule["uniqueItems"] == true {
        for (i, item) in items.iter().enumerate() {
            if items.get(..i).is_some_and(|prefix| prefix.contains(item)) {
                return Err(fail(&format!("{path}/{i}"), "array item repeats"));
            }
        }
    }
    let prefixes = rule["prefixItems"].as_array();
    for (i, item) in items.iter().enumerate() {
        let item_rule = prefixes
            .and_then(|p| p.get(i))
            .or_else(|| rule.get("items"));
        match item_rule {
            Some(Value::Bool(false)) => {
                return Err(fail(&format!("{path}/{i}"), "extra array item"));
            }
            Some(item_rule) => check(item, item_rule, root, &format!("{path}/{i}"), regexes)?,
            None => {}
        }
    }
    if let Some(contains) = rule.get("contains") {
        let count = items
            .iter()
            .filter(|item| check(item, contains, root, path, regexes).is_ok())
            .count();
        if count < usize::try_from(rule["minContains"].as_u64().unwrap_or(1)).unwrap_or(usize::MAX)
            || rule["maxContains"]
                .as_u64()
                .is_some_and(|n| count > usize::try_from(n).unwrap_or(usize::MAX))
        {
            return Err(fail(path, "array contents do not match"));
        }
    }
    Ok(())
}

fn check_object(
    object: &serde_json::Map<String, Value>,
    rule: &Value,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    if rule["minProperties"]
        .as_u64()
        .is_some_and(|n| object.len() < usize::try_from(n).unwrap_or(usize::MAX))
        || rule["maxProperties"]
            .as_u64()
            .is_some_and(|n| object.len() > usize::try_from(n).unwrap_or(usize::MAX))
    {
        return Err(fail(path, "object field count is outside the schema"));
    }
    for required in rule["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !object.contains_key(required) {
            return Err(fail(
                &format!("{path}/{required}"),
                "required field is missing",
            ));
        }
    }
    for (key, value) in object {
        check_property(key, value, rule, root, path, regexes)?;
    }
    check_dependent_required(object, rule, path)
}

fn check_property(
    key: &str,
    value: &Value,
    rule: &Value,
    root: &Value,
    path: &str,
    regexes: &mut HashMap<String, Regex>,
) -> Result<(), Failure> {
    let place = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
    if let Some(names) = rule.get("propertyNames") {
        check(
            &Value::String(key.to_string()),
            names,
            root,
            &place,
            regexes,
        )?;
    }
    if let Some(named) = rule["properties"].get(key) {
        return check(value, named, root, &place, regexes);
    }
    let mut patterned = false;
    if let Some(patterns) = rule["patternProperties"].as_object() {
        for (pattern, subrule) in patterns {
            if pattern_match(key, pattern, &place, regexes).is_ok() {
                check(value, subrule, root, &place, regexes)?;
                patterned = true;
            }
        }
    }
    if patterned {
        return Ok(());
    }
    match rule.get("additionalProperties") {
        Some(Value::Bool(false)) => Err(fail(&place, "unknown field")),
        Some(subrule) if subrule.is_object() => check(value, subrule, root, &place, regexes),
        _ => Ok(()),
    }
}

fn check_dependent_required(
    object: &serde_json::Map<String, Value>,
    rule: &Value,
    path: &str,
) -> Result<(), Failure> {
    if let Some(dependencies) = rule["dependentRequired"].as_object() {
        for (key, required) in dependencies {
            if !object.contains_key(key) {
                continue;
            }
            for name in required
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !object.contains_key(name) {
                    return Err(fail(
                        &format!("{path}/{name}"),
                        "dependent field is missing",
                    ));
                }
            }
        }
    }
    Ok(())
}
