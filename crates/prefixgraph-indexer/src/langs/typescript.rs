use crate::plugin::*;
use tree_sitter::{Node, Tree};

pub struct TypeScriptPlugin {
    pub tsx: bool,
}

impl LanguagePlugin for TypeScriptPlugin {
    fn id(&self) -> &'static str {
        if self.tsx {
            "tsx"
        } else {
            "typescript"
        }
    }

    fn extensions(&self) -> &'static [&'static str] {
        if self.tsx {
            // The TSX grammar is a near-superset of JS/JSX, so plain JS files
            // are parsed with it too.
            &[".tsx", ".jsx", ".js", ".mjs", ".cjs"]
        } else {
            &[".ts", ".mts", ".cts"]
        }
    }

    fn language(&self) -> tree_sitter::Language {
        if self.tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
    }

    fn scan(&self, tree: &Tree, src: &str) -> FileScan {
        let mut scan = FileScan::default();
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            handle_top(child, None, src, &mut scan);
        }
        scan
    }

    fn is_test_path(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        name.contains(".test.") || name.contains(".spec.") || path.contains("__tests__/")
    }
}

fn handle_top(node: Node<'_>, export_outer: Option<Node<'_>>, src: &str, scan: &mut FileScan) {
    let outer = export_outer.unwrap_or(node);
    match node.kind() {
        "import_statement" => {
            if let Some(source) = node.child_by_field_name("source") {
                let spec = node_text(source, src).trim_matches(['"', '\'']).to_string();
                scan.imports.push(spec);
            }
        }
        "export_statement" => {
            if let Some(decl) = node.child_by_field_name("declaration") {
                handle_top(decl, Some(node), src, scan);
            }
        }
        "function_declaration" | "generator_function_declaration" => {
            if let Some(d) = function_decl(node, outer, src) {
                scan.decls.push(d);
            }
        }
        "class_declaration" | "abstract_class_declaration" => {
            if let Some(d) = class_decl(node, outer, src) {
                scan.decls.push(d);
            }
        }
        "interface_declaration" => {
            if let Some(d) = typeish_decl(node, outer, src, "interface") {
                scan.decls.push(d);
            }
        }
        "enum_declaration" => {
            if let Some(d) = typeish_decl(node, outer, src, "enum") {
                scan.decls.push(d);
            }
        }
        "type_alias_declaration" => {
            if let Some(d) = typeish_decl(node, outer, src, "type") {
                scan.decls.push(d);
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            let mut c = node.walk();
            for declarator in node.named_children(&mut c) {
                if declarator.kind() != "variable_declarator" {
                    continue;
                }
                let Some(value) = declarator.child_by_field_name("value") else {
                    continue;
                };
                if !matches!(
                    value.kind(),
                    "arrow_function" | "function_expression" | "function"
                ) {
                    continue;
                }
                let Some(name) = declarator.child_by_field_name("name") else {
                    continue;
                };
                let body_start = value
                    .child_by_field_name("body")
                    .map(|b| b.start_byte())
                    .unwrap_or(value.end_byte());
                let header = collapse_ws(slice(src, outer.start_byte(), body_start));
                scan.decls.push(Decl {
                    kind: "function",
                    name: node_text(name, src).to_string(),
                    byte_start: outer.start_byte(),
                    byte_end: outer.end_byte(),
                    line_start: outer.start_position().row + 1,
                    line_end: outer.end_position().row + 1,
                    signature: format!("{header} ...\n"),
                    calls: collect_calls(value, src),
                    bases: vec![],
                    impls: vec![],
                });
                break; // one chunk per statement
            }
        }
        _ => {}
    }
}

fn function_decl(node: Node<'_>, outer: Node<'_>, src: &str) -> Option<Decl> {
    let name = node_text(node.child_by_field_name("name")?, src).to_string();
    let body_start = node
        .child_by_field_name("body")
        .map(|b| b.start_byte())
        .unwrap_or(node.end_byte());
    let header = collapse_ws(slice(src, outer.start_byte(), body_start));
    Some(Decl {
        kind: "function",
        name,
        byte_start: outer.start_byte(),
        byte_end: outer.end_byte(),
        line_start: outer.start_position().row + 1,
        line_end: outer.end_position().row + 1,
        signature: format!("{header} ...\n"),
        calls: collect_calls(node, src),
        bases: vec![],
        impls: vec![],
    })
}

fn class_decl(node: Node<'_>, outer: Node<'_>, src: &str) -> Option<Decl> {
    let name = node_text(node.child_by_field_name("name")?, src).to_string();
    let body = node.child_by_field_name("body")?;

    let mut signature = collapse_ws(slice(src, outer.start_byte(), body.start_byte()));
    signature.push_str(" {\n");
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        match member.kind() {
            "method_definition" => {
                let mb_start = member
                    .child_by_field_name("body")
                    .map(|b| b.start_byte())
                    .unwrap_or(member.end_byte());
                let header = collapse_ws(slice(src, member.start_byte(), mb_start));
                signature.push_str(&format!("  {header} ...\n"));
            }
            "public_field_definition" => {
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
        if child.kind() == "class_heritage" {
            visit(child, &mut |n| match n.kind() {
                "identifier" | "type_identifier" => {
                    // tree position determines extends vs implements; collect under
                    // the nearest clause kind
                    let mut p = n.parent();
                    while let Some(pp) = p {
                        match pp.kind() {
                            "extends_clause" => {
                                bases.push(node_text(n, src).to_string());
                                break;
                            }
                            "implements_clause" => {
                                impls.push(node_text(n, src).to_string());
                                break;
                            }
                            "class_heritage" => break,
                            _ => p = pp.parent(),
                        }
                    }
                }
                _ => {}
            });
        }
    }
    bases.dedup();
    impls.dedup();

    Some(Decl {
        kind: "class",
        name,
        byte_start: outer.start_byte(),
        byte_end: outer.end_byte(),
        line_start: outer.start_position().row + 1,
        line_end: outer.end_position().row + 1,
        signature,
        calls: collect_calls(node, src),
        bases,
        impls,
    })
}

fn typeish_decl(node: Node<'_>, outer: Node<'_>, src: &str, kind: &'static str) -> Option<Decl> {
    let name = node_text(node.child_by_field_name("name")?, src).to_string();
    let text = node_text(outer, src);
    // Short type declarations are their own best signature.
    let signature = if text.lines().count() <= 12 {
        format!("{text}\n")
    } else {
        format!("{} ...\n", collapse_ws(text.lines().next().unwrap_or("")))
    };
    Some(Decl {
        kind,
        name,
        byte_start: outer.start_byte(),
        byte_end: outer.end_byte(),
        line_start: outer.start_position().row + 1,
        line_end: outer.end_position().row + 1,
        signature,
        calls: vec![],
        bases: vec![],
        impls: vec![],
    })
}

fn collect_calls(node: Node<'_>, src: &str) -> Vec<String> {
    let mut calls = Vec::new();
    visit(node, &mut |n| {
        if n.kind() == "call_expression" {
            if let Some(f) = n.child_by_field_name("function") {
                let name = match f.kind() {
                    "identifier" => Some(node_text(f, src).to_string()),
                    "member_expression" => f
                        .child_by_field_name("property")
                        .map(|p| node_text(p, src).to_string()),
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
    calls
}
