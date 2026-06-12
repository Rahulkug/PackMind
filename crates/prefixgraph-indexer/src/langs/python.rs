use crate::plugin::*;
use tree_sitter::{Node, Tree};

pub struct PythonPlugin;

impl LanguagePlugin for PythonPlugin {
    fn id(&self) -> &'static str {
        "python"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".py"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn scan(&self, tree: &Tree, src: &str) -> FileScan {
        let mut scan = FileScan::default();
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "import_statement" => {
                    let mut c = child.walk();
                    for item in child.named_children(&mut c) {
                        match item.kind() {
                            "dotted_name" => scan.imports.push(node_text(item, src).to_string()),
                            "aliased_import" => {
                                if let Some(name) = item.child_by_field_name("name") {
                                    scan.imports.push(node_text(name, src).to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "import_from_statement" => {
                    if let Some(m) = child.child_by_field_name("module_name") {
                        scan.imports.push(node_text(m, src).to_string());
                    }
                }
                "function_definition" | "class_definition" => {
                    if let Some(d) = build_decl(child, child, src) {
                        scan.decls.push(d);
                    }
                }
                "decorated_definition" => {
                    if let Some(def) = child.child_by_field_name("definition") {
                        if let Some(d) = build_decl(def, child, src) {
                            scan.decls.push(d);
                        }
                    }
                }
                _ => {}
            }
        }
        scan
    }

    fn is_test_path(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        name.starts_with("test_")
            || name.ends_with("_test.py")
            || path.contains("tests/")
            || path.contains("test/")
    }
}

/// `def_node` is the function/class definition; `outer` is the node whose byte
/// range becomes the chunk (the decorated_definition when decorators exist).
fn build_decl(def_node: Node<'_>, outer: Node<'_>, src: &str) -> Option<Decl> {
    let name = node_text(def_node.child_by_field_name("name")?, src).to_string();
    let body = def_node.child_by_field_name("body")?;
    let kind: &'static str = if def_node.kind() == "class_definition" {
        "class"
    } else {
        "function"
    };

    let signature = if kind == "class" {
        class_signature(def_node, body, src)
    } else {
        let header = collapse_ws(slice(src, def_node.start_byte(), body.start_byte()));
        match docstring(body, src) {
            Some(d) => format!("{header}\n    \"\"\"{d}\"\"\"\n    ...\n"),
            None => format!("{header} ...\n"),
        }
    };

    let mut calls = Vec::new();
    visit(def_node, &mut |n| {
        if n.kind() == "call" {
            if let Some(f) = n.child_by_field_name("function") {
                let name = match f.kind() {
                    "identifier" => Some(node_text(f, src).to_string()),
                    "attribute" => f
                        .child_by_field_name("attribute")
                        .map(|a| node_text(a, src).to_string()),
                    _ => None,
                };
                if let Some(n) = name {
                    calls.push(n);
                }
            }
        }
    });
    calls.sort();
    calls.dedup();

    let mut bases = Vec::new();
    if kind == "class" {
        if let Some(supers) = def_node.child_by_field_name("superclasses") {
            let mut c = supers.walk();
            for arg in supers.named_children(&mut c) {
                if arg.kind() == "identifier" || arg.kind() == "attribute" {
                    let t = node_text(arg, src);
                    let short = t.rsplit('.').next().unwrap_or(t);
                    bases.push(short.to_string());
                }
            }
        }
    }

    Some(Decl {
        kind,
        name,
        byte_start: outer.start_byte(),
        byte_end: outer.end_byte(),
        line_start: outer.start_position().row + 1,
        line_end: outer.end_position().row + 1,
        signature,
        calls,
        bases,
        impls: vec![],
    })
}

fn class_signature(class_node: Node<'_>, body: Node<'_>, src: &str) -> String {
    let mut s = collapse_ws(slice(src, class_node.start_byte(), body.start_byte()));
    s.push('\n');
    if let Some(d) = docstring(body, src) {
        s.push_str(&format!("    \"\"\"{d}\"\"\"\n"));
    }
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        let def = if member.kind() == "decorated_definition" {
            member.child_by_field_name("definition")
        } else {
            Some(member)
        };
        if let Some(m) = def {
            if m.kind() == "function_definition" {
                if let Some(mb) = m.child_by_field_name("body") {
                    let header = collapse_ws(slice(src, m.start_byte(), mb.start_byte()));
                    s.push_str(&format!("    {header} ...\n"));
                }
            }
        }
    }
    s
}

/// First line of a module/class/function docstring, when present.
fn docstring(body: Node<'_>, src: &str) -> Option<String> {
    let first = body.named_child(0)?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let string = first.named_child(0)?;
    if string.kind() != "string" {
        return None;
    }
    let text = node_text(string, src)
        .trim_matches(|c| c == '"' || c == '\'')
        .trim();
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}
