use std::borrow::Cow;
use std::str;

extern crate inflector;
use inflector::cases::pascalcase::to_pascal_case;
use inflector::cases::snakecase::to_snake_case;

use roxmltree::{Namespace, Node};

use crate::parser::constants::attribute;
use crate::parser::parser::parse_node;
use crate::parser::types::{RsEntity, StructField};
use crate::parser::xsd_elements::{ElementType, XsdNode};

pub fn split_comment_line(s: &str, max_len: usize, indent: usize) -> String {
    let indent_str = " ".repeat(indent);

    let mut splitted = format!("{}//", indent_str);
    let mut current_line_length = indent + 2;
    for word in s.split_whitespace() {
        let len = word.len();
        if current_line_length + len < max_len {
            splitted = format!("{} {}", splitted, word);
            current_line_length += 1 + len;
        } else {
            splitted = format!("{}\n{}// {}", splitted, indent_str, word);
            current_line_length = indent + 3 + len;
        }
    }
    format!("{}\n", splitted)
}

pub fn get_formatted_comment(doc: Option<&str>) -> String {
    doc.unwrap_or("")
        .lines()
        .map(|s| s.trim())
        .filter(|s| s.len() > 1)
        .map(|s| split_comment_line(s, 80, 0))
        .fold(String::new(), |x, y| (x + &y))
}

pub fn match_type(type_name: &str, target_ns: Option<&roxmltree::Namespace>) -> Cow<'static, str> {
    fn replace(s: &str) -> String {
        match s.find(':') {
            Some(index) => format!(
                "{}::{}",
                &s[0..index],
                to_pascal_case(&s[index..].replace("-", "_"))
            ),
            None => to_pascal_case(s.replace("-", "_").as_str()),
        }
    }
    match type_name {
        "xs:hexBinary" => "String".into(),
        "xs:base64Binary" => "String".into(),

        "xs:boolean" => "bool".into(),

        // TODO: should use something like num_bigint::Bigint instead, but with a wrap that
        // implements Yaserde (de)serialization. For that we need to use the "flatten" from yaserde,
        // as it will be updated on the crates.io.
        "xs:integer" => "i64".into(),
        "xs:nonNegativeInteger" => "u64".into(),
        "xs:positiveInteger" => "u64".into(),
        "xs:nonPositiveInteger" => "i64".into(),
        "xs:negativeInteger" => "i64".into(),

        "xs:long" => "i64".into(),
        "xs:int" => "i32".into(),
        "xs:short" => "i16".into(),
        "xs:byte" => "i8".into(),

        "xs:unsignedLong" => "u64".into(),
        "xs:unsignedInt" => "u32".into(),
        "xs:unsignedShort" => "u16".into(),
        "xs:unsignedByte" => "u8".into(),

        // TODO: should use something like bigdecimal::BigDecimal instead, but with a wrap that
        // implements Yaserde (de)serialization. For that we need to use the "flatten" from yaserde,
        // as it will be updated on the crates.io.
        "xs:decimal" => "f64".into(),

        "xs:double" => "f64".into(),
        "xs:float" => "f64".into(),

        // TODO: might use types from chrono crate instead, but with a wrap that implements Yaserde
        // (de)serialization. For that we need to use the "flatten" from yaserde, as it will be
        // updated on the crates.io.
        "xs:date" => "String".into(),
        "xs:time" => "String".into(),
        "xs:dateTime" => "String".into(),
        "xs:dateTimeStamp" => "String".into(),

        "xs:duration" => "tt::Duration".into(),

        // TODO: would be nice to have types with both numeric representation of value and proper
        // (de)serialization. For that we might use the "flatten" from yaserde, as it will be
        // updated on the crates.io.
        "xs:gDay" => "String".into(),
        "xs:gMonth" => "String".into(),
        "xs:gMonthDay" => "String".into(),
        "xs:gYear" => "String".into(),
        "xs:gYearMonth" => "String".into(),

        "xs:string" => "String".into(),
        "xs:normalizedString" => "String".into(),
        "xs:token" => "String".into(),
        "xs:language" => "String".into(),
        "xs:Name" => "String".into(),
        "xs:NCName" => "String".into(),
        "xs:ENTITY" => "String".into(),
        "xs:ID" => "String".into(),
        "xs:IDREF" => "String".into(),
        "xs:NMTOKEN" => "String".into(),
        "xs:anyURI" => "String".into(),
        "xs:QName" => "String".into(),

        "xs::NOTATION" => "String".into(),

        // Built-in list types:
        "xs:ENTITIES" => "Vec<String>".into(),
        "xs:IDREFS" => "Vec<String>".into(),
        "xs:NMTOKENS" => "Vec<String>".into(),

        x => {
            let prefix = target_ns.and_then(|ns| ns.name());
            match prefix {
                Some(name) => {
                    if x.starts_with(name) {
                        to_pascal_case(&x[name.len() + 1..])
                    } else {
                        replace(x)
                    }
                }
                None => replace(x),
            }
            .into()
        }
    }
}

pub fn get_field_name(name: &str) -> String {
    let result = to_snake_case(name);
    if result.chars().next().unwrap().is_numeric() || RS_KEYWORDS.contains(&result.as_str()) {
        return format!("_{}", result);
    }
    result
}

pub fn get_type_name(name: &str) -> String {
    let result = to_pascal_case(name);
    if result.chars().next().unwrap().is_numeric() || RS_KEYWORDS.contains(&result.as_str()) {
        return format!("_{}", result);
    }
    result
}

pub fn any_attribute_field() -> StructField {
    StructField {
        name: "any_attribute".to_string(),
        type_name: "AnyAttribute".to_string(),
        comment: Some("//".to_string()),
        macros: "//TODO: any_attribute macros \n//".to_string(),
        subtypes: vec![],
    }
}

pub fn target_namespace<'a, 'input>(node: &Node<'a, 'input>) -> Option<&'a Namespace<'input>> {
    match node.attribute(attribute::TARGET_NAMESPACE) {
        Some(tn) => node.namespaces().iter().find(|a| a.uri() == tn),
        None => None,
    }
}

pub fn find_child<'a, 'input>(node: &Node<'a, 'input>, tag_name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|e| e.is_element() && e.tag_name().name() == tag_name)
}

pub fn get_documentation<'a>(node: &Node<'a, '_>) -> Option<String> {
    find_child(node, "annotation")
        .and_then(|node| find_child(&node, "documentation"))
        .and_then(|node| node.text().map(|s| s.to_string()))
}

pub fn tuple_struct_macros() -> String {
    "//TODO: Tuple Struct macros\n".to_string()
}

pub fn yaserde_for_attribute(name: &str) -> String {
    if let Some(index) = name.find(':') {
        format!(
            "  #[yaserde(attribute, prefix = \"{}\" rename = \"{}\")]\n",
            &name[0..index],
            &name[index + 1..]
        )
    } else {
        format!("  #[yaserde(attribute, rename = \"{}\")]\n", name)
    }
}

pub fn yaserde_for_element(name: &str, target_namespace: Option<&roxmltree::Namespace>) -> String {
    let prefix = target_namespace.and_then(|ns| ns.name());
    match prefix {
        Some(p) => format!("  #[yaserde(prefix = \"{}\", rename = \"{}\")]\n", p, name),
        None => format!("  #[yaserde(rename = \"{}\")]\n", name),
    }
}

pub fn get_parent_name<'a>(node: &Node<'a, '_>) -> &'a str {
    match node.parent_element() {
        Some(parent) => {
            if parent.xsd_type() == ElementType::Schema {
                return "SchemaElement";
            }

            match parent.attribute(attribute::NAME) {
                Some(s) => s,
                None => get_parent_name(&parent),
            }
        }
        None => "UnsupportedName",
    }
}

pub fn struct_macro(target_namespace: Option<&roxmltree::Namespace>) -> String {
    let derives = "#[derive(Default, PartialEq, Debug, YaSerialize, YaDeserialize)]\n";
    match target_namespace {
        Some(tn) => match tn.name() {
            Some(name) => format!(
                "{derives}#[yaserde(prefix = \"{prefix}\", namespace = \"{prefix}: {uri}\")]\n",
                derives = derives,
                prefix = name,
                uri = tn.uri()
            ),
            None => format!(
                "{derives}#[yaserde(namespace = \"{uri}\")]\n",
                derives = derives,
                uri = tn.uri()
            ),
        },
        None => format!("{derives}#[yaserde()]\n", derives = derives),
    }
}

pub fn attributes_to_fields(node: &Node, target_ns: Option<&Namespace>) -> Vec<StructField> {
    node.children()
        .filter(|n| n.is_element() && n.xsd_type() == ElementType::Attribute)
        .map(|n| match parse_node(&n, node, target_ns) {
            RsEntity::StructField(sf) => sf,
            _ => unreachable!("Invalid attribute parsing: {:?}", n),
        })
        .collect()
}

const RS_KEYWORDS: &[&str] = &[
    "abstract",
    "alignof",
    "as",
    "become",
    "box",
    "break",
    "const",
    "continue",
    "crate",
    "do",
    "else",
    "enum",
    "extern crate",
    "extern",
    "false",
    "final",
    "fn",
    "for",
    "if let",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "macro",
    "match",
    "mod",
    "move",
    "mut",
    "offsetof",
    "override",
    "priv",
    "proc",
    "pub",
    "pure",
    "ref",
    "return",
    "Self",
    "self",
    "sizeof",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "typeof",
    "unsafe",
    "unsized",
    "use",
    "virtual",
    "where",
    "while",
    "yield",
];
