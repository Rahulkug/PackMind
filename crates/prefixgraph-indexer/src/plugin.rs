//! The LanguagePlugin interface — the primary community contribution surface.
//! Adding a language = implement this trait + provide a conformance corpus.
//! See docs/writing-language-plugins.md.

use tree_sitter::{Node, Tree};

/// One top-level declaration extracted from a file.
#[derive(Debug, Clone)]
pub struct Decl {
    /// "function" | "class" | "interface" | "enum" | "type" | "record"
    pub kind: &'static str,
    pub name: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,
    pub line_end: usize,
    /// Low-token skeleton: decl header (+ member signatures for types),
    /// bodies elided with `...`.
    pub signature: String,
    /// Simple names of functions/methods called within this declaration.
    pub calls: Vec<String>,
    /// Names of types this declaration extends.
    pub bases: Vec<String>,
    /// Names of interfaces this declaration implements.
    pub impls: Vec<String>,
}

#[derive(Debug, Default)]
pub struct FileScan {
    pub decls: Vec<Decl>,
    /// Raw import targets as written in source (module paths / specifiers).
    pub imports: Vec<String>,
}

pub trait LanguagePlugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn language(&self) -> tree_sitter::Language;
    /// Single pass over a parsed file: declarations + imports.
    fn scan(&self, tree: &Tree, src: &str) -> FileScan;
    fn is_test_path(&self, path: &str) -> bool;
}

// ---------- shared helpers for plugin implementations ----------

pub fn node_text<'a>(node: Node<'_>, src: &'a str) -> &'a str {
    src.get(node.byte_range()).unwrap_or("")
}

pub fn slice<'a>(src: &'a str, start: usize, end: usize) -> &'a str {
    src.get(start..end).unwrap_or("")
}

/// Depth-first visit of every node in a subtree.
pub fn visit<'a, F: FnMut(Node<'a>)>(node: Node<'a>, f: &mut F) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, f);
    }
}

/// Collapse a (possibly multi-line) declaration header into one line.
pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse(language: &tree_sitter::Language, src: &str) -> Option<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).ok()?;
    parser.parse(src, None)
}
