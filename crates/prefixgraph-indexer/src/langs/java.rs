use crate::plugin::*;
use tree_sitter::{Node, Tree};

pub struct JavaPlugin;

impl LanguagePlugin for JavaPlugin {
    fn id(&self) -> &'static str {
        "java"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".java"]
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn scan(&self, tree: &Tree, src: &str) -> FileScan {
        let mut scan = FileScan::default();
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "import_declaration" => {
                    let text = node_text(child, src);
                    let spec = text
                        .trim_start_matches("import")
                        .trim()
                        .trim_start_matches("static")
                        .trim()
                        .trim_end_matches(';')
                        .trim()
                        .to_string();
                    if !spec.is_empty() {
                        scan.imports.push(spec);
                    }
                }
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration" => {
                    if let Some(d) = type_decl(child, src) {
                        scan.decls.push(d);
                    }
                }
                _ => {}
            }
        }
        scan
    }

    fn is_test_path(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        path.contains("src/test/")
            || name.ends_with("Test.java")
            || name.ends_with("Tests.java")
            || name.ends_with("IT.java")
    }
}

fn type_decl(node: Node<'_>, src: &str) -> Option<Decl> {
    let name = node_text(node.child_by_field_name("name")?, src).to_string();
    let body = node.child_by_field_name("body")?;
    let kind: &'static str = match node.kind() {
        "interface_declaration" => "interface",
        "enum_declaration" => "enum",
        "record_declaration" => "record",
        _ => "class",
    };

    // Skeleton: type header + field lines + method/constructor headers.
    let mut signature = collapse_ws(slice(src, node.start_byte(), body.start_byte()));
    signature.push_str(" {\n");
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        match member.kind() {
            "method_declaration" | "constructor_declaration" => {
                let header_end = member
                    .child_by_field_name("body")
                    .map(|b| b.start_byte())
                    .unwrap_or(member.end_byte());
                let header = collapse_ws(slice(src, member.start_byte(), header_end));
                signature.push_str(&format!("  {header};\n"));
            }
            "field_declaration" | "constant_declaration" => {
                signature.push_str(&format!("  {}\n", collapse_ws(node_text(member, src))));
            }
            _ => {}
        }
    }
    signature.push_str("}\n");

    let mut bases = Vec::new();
    let mut impls = Vec::new();
    let mut hc = node.walk();
    for child in node.children(&mut hc) {
        match child.kind() {
            "superclass" => visit(child, &mut |n| {
                if n.kind() == "type_identifier" {
                    bases.push(node_text(n, src).to_string());
                }
            }),
            "super_interfaces" | "extends_interfaces" => visit(child, &mut |n| {
                if n.kind() == "type_identifier" {
                    impls.push(node_text(n, src).to_string());
                }
            }),
            _ => {}
        }
    }
    bases.dedup();
    impls.dedup();

    let mut calls = Vec::new();
    visit(node, &mut |n| match n.kind() {
        "method_invocation" => {
            if let Some(m) = n.child_by_field_name("name") {
                calls.push(node_text(m, src).to_string());
            }
        }
        "object_creation_expression" => {
            if let Some(t) = n.child_by_field_name("type") {
                let t = node_text(t, src);
                let short = t.split('<').next().unwrap_or(t);
                let short = short.rsplit('.').next().unwrap_or(short);
                calls.push(short.to_string());
            }
        }
        _ => {}
    });
    calls.sort();
    calls.dedup();

    Some(Decl {
        kind,
        name,
        byte_start: node.start_byte(),
        byte_end: node.end_byte(),
        line_start: node.start_position().row + 1,
        line_end: node.end_position().row + 1,
        signature,
        calls,
        bases,
        impls,
    })
}
