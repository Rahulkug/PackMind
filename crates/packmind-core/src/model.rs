use serde::Serialize;

/// 16-byte node identifier: the first 128 bits of the content hash.
pub type NodeId = [u8; 16];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File = 1,
    AstChunk = 2,
    Signature = 3,
    DocChunk = 4,
}

impl NodeKind {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Option<Self> {
        match v {
            1 => Some(NodeKind::File),
            2 => Some(NodeKind::AstChunk),
            3 => Some(NodeKind::Signature),
            4 => Some(NodeKind::DocChunk),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NodeKind::File => "file",
            NodeKind::AstChunk => "ast_chunk",
            NodeKind::Signature => "signature",
            NodeKind::DocChunk => "doc_chunk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Imports = 1,
    Calls = 2,
    Inherits = 3,
    Implements = 4,
    TestedBy = 5,
    MentionsDoc = 6,
    Supersedes = 7,
}

impl EdgeKind {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Option<Self> {
        match v {
            1 => Some(EdgeKind::Imports),
            2 => Some(EdgeKind::Calls),
            3 => Some(EdgeKind::Inherits),
            4 => Some(EdgeKind::Implements),
            5 => Some(EdgeKind::TestedBy),
            6 => Some(EdgeKind::MentionsDoc),
            7 => Some(EdgeKind::Supersedes),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EdgeKind::Imports => "imports",
            EdgeKind::Calls => "calls",
            EdgeKind::Inherits => "inherits",
            EdgeKind::Implements => "implements",
            EdgeKind::TestedBy => "tested_by",
            EdgeKind::MentionsDoc => "doc_mention",
            EdgeKind::Supersedes => "supersedes",
        }
    }
}

/// One content-addressed graph node. `content` holds the normalized bytes
/// (always valid UTF-8 by construction; non-UTF-8 files are excluded from the graph).
#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub hash: [u8; 32],
    pub kind: NodeKind,
    pub path: String,
    pub symbol: Option<String>,
    pub lang: Option<String>,
    pub role: String,
    pub byte_start: i64,
    pub byte_end: i64,
    pub line_start: i64,
    pub line_end: i64,
    pub tokens: i64,
    pub content: String,
    pub valid: bool,
    pub centrality: f64,
}

pub fn id_hex(id: &NodeId) -> String {
    hex::encode(id)
}

pub fn id_from_hex(s: &str) -> Option<NodeId> {
    let bytes = hex::decode(s).ok()?;
    bytes.try_into().ok()
}
