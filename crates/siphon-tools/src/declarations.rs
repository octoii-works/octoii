//! The tools a script calls, told in TypeScript, as a model reads TypeScript best and writes the script that calls them
//! in it: each a function of `tools` taking its arguments as one object — the JSON Schema of its parameters written as
//! the TypeScript type of that object, each description a doc comment beside what it describes — and resolving to what
//! the tool gives, as text.

use std::fmt::Write as _;

use serde_json::Value;

use crate::tool::Declared;

/// How deep a schema is written out; below, `unknown`.
const DEPTH_MAX: usize = 8;

/// `tools` as a script is told of it.
#[must_use]
pub fn declarations(tools: &[Declared]) -> String {
    let mut out = String::from("declare const tools: {\n");
    for tool in tools {
        let _ = writeln!(out, "{}", doc(&tool.description, "  "));
        let parameters = type_of(&tool.parameters, &tool.parameters, 1, 0);
        let _ = writeln!(out, "  {}(args: {parameters}): Promise<string>;", quoted_key(&tool.name));
    }
    out.push_str("};");
    out
}

/// A doc comment of `text`, each line of it at `indent`.
fn doc(text: &str, indent: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = text.lines().map(|line| format!("{indent} * {line}")).collect();
    format!("{indent}/**\n{}\n{indent} */", lines.join("\n"))
}

/// A property's or a method's name as TypeScript takes it: as it is where it is an identifier, quoted where not.
fn quoted_key(name: &str) -> String {
    let identifier = name.chars().next().is_some_and(|first| first.is_alphabetic() || first == '_')
        && name.chars().all(|char| char.is_alphanumeric() || char == '_' || char == '$');
    if identifier {
        name.to_owned()
    } else {
        serde_json::json!(name).to_string()
    }
}

/// The schema `schema` points to, where it is a `$ref` into `root`'s `$defs`.
fn resolved<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
    match schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
    {
        Some(name) => &root["$defs"][name],
        None => schema,
    }
}

/// The TypeScript type of `schema`, of the document `root`, at `indent` levels, `depth` deep.
fn type_of(schema: &Value, root: &Value, indent: usize, depth: usize) -> String {
    if depth > DEPTH_MAX {
        return "unknown".into();
    }
    let schema = resolved(schema, root);
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values.iter().map(Value::to_string).collect::<Vec<_>>().join(" | ");
    }
    if let Some(value) = schema.get("const") {
        return value.to_string();
    }
    for union in ["anyOf", "oneOf"] {
        if let Some(each) = schema.get(union).and_then(Value::as_array) {
            return each
                .iter()
                .map(|one| type_of(one, root, indent, depth + 1))
                .collect::<Vec<_>>()
                .join(" | ");
        }
    }
    match &schema["type"] {
        Value::Array(types) => types
            .iter()
            .map(|kind| {
                let mut one = schema.clone();
                one["type"] = kind.clone();
                type_of(&one, root, indent, depth + 1)
            })
            .collect::<Vec<_>>()
            .join(" | "),
        Value::String(kind) => match kind.as_str() {
            "string" => "string".into(),
            "number" | "integer" => "number".into(),
            "boolean" => "boolean".into(),
            "null" => "null".into(),
            "array" => format!("Array<{}>", type_of(&schema["items"], root, indent, depth + 1)),
            "object" => object_of(schema, root, indent, depth),
            _ => "unknown".into(),
        },
        _ if schema.get("properties").is_some() => object_of(schema, root, indent, depth),
        _ => "unknown".into(),
    }
}

/// The TypeScript object type of the object schema `schema`: each property, optional where it is not required, with
/// its description as a doc comment.
fn object_of(schema: &Value, root: &Value, indent: usize, depth: usize) -> String {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return "Record<string, unknown>".into();
    };
    if properties.is_empty() {
        return "{}".into();
    }
    let required: Vec<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let pad = "  ".repeat(indent + 1);
    let mut out = String::from("{\n");
    for (name, property) in properties {
        let description = resolved(property, root)["description"]
            .as_str()
            .or_else(|| property["description"].as_str())
            .unwrap_or_default();
        if !description.is_empty() {
            let _ = writeln!(out, "{}", doc(description, &pad));
        }
        let optional = if required.contains(&name.as_str()) { "" } else { "?" };
        let _ = writeln!(
            out,
            "{pad}{}{optional}: {};",
            quoted_key(name),
            type_of(property, root, indent + 1, depth + 1)
        );
    }
    let _ = write!(out, "{}}}", "  ".repeat(indent));
    out
}
