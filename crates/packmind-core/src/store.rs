//! SQLite-backed graph store. One database per repo root:
//! `.packmind/index.db` (WAL mode, single writer, many readers).

use crate::model::{id_hex, EdgeKind, Node, NodeId, NodeKind};
use crate::pack::ContextPack;
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct Store {
    pub conn: Connection,
    pub root: PathBuf,
    pub fts_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct FileRow {
    pub path: String,
    pub file_node: Option<NodeId>,
    /// Ordered merkle children: (byte_start, byte_end, chunk node id).
    pub merkle: Vec<(i64, i64, NodeId)>,
    pub mtime: i64,
    pub size: i64,
    pub content_sha: Option<String>,
    pub skipped: Option<String>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Counts {
    pub files: i64,
    pub skipped_files: i64,
    pub chunks: i64,
    pub signatures: i64,
    pub docs: i64,
    pub edges: i64,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;

CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS nodes (
  id            BLOB PRIMARY KEY,
  hash          BLOB NOT NULL,
  kind          INTEGER NOT NULL,
  path          TEXT NOT NULL,
  symbol        TEXT,
  lang          TEXT,
  role          TEXT NOT NULL DEFAULT '',
  byte_start    INTEGER NOT NULL DEFAULT 0,
  byte_end      INTEGER NOT NULL DEFAULT 0,
  line_start    INTEGER NOT NULL DEFAULT 0,
  line_end      INTEGER NOT NULL DEFAULT 0,
  tokens        INTEGER NOT NULL,
  content       TEXT NOT NULL,
  valid         INTEGER NOT NULL DEFAULT 1,
  superseded_by BLOB,
  commit_sha    TEXT,
  centrality    REAL NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  staled_at     INTEGER
);
CREATE INDEX IF NOT EXISTS idx_nodes_path   ON nodes(path, valid);
CREATE INDEX IF NOT EXISTS idx_nodes_symbol ON nodes(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_nodes_kind   ON nodes(kind, valid);

CREATE TABLE IF NOT EXISTS edges (
  src BLOB NOT NULL, kind INTEGER NOT NULL, dst BLOB NOT NULL,
  weight REAL NOT NULL DEFAULT 1.0,
  PRIMARY KEY (src, kind, dst)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_edges_rev ON edges(dst, kind);

CREATE TABLE IF NOT EXISTS files (
  path        TEXT PRIMARY KEY,
  file_node   BLOB,
  merkle_json TEXT NOT NULL DEFAULT '[]',
  mtime       INTEGER NOT NULL DEFAULT 0,
  size        INTEGER NOT NULL DEFAULT 0,
  content_sha TEXT,
  skipped     TEXT
);

CREATE TABLE IF NOT EXISTS stats (
  node_id BLOB PRIMARY KEY,
  hits INTEGER NOT NULL DEFAULT 0,
  last_used INTEGER,
  ewma REAL NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS hot_set (
  version INTEGER NOT NULL, position INTEGER NOT NULL,
  node_id BLOB NOT NULL, PRIMARY KEY (version, position)
);

CREATE TABLE IF NOT EXISTS packs (
  id TEXT PRIMARY KEY, ts INTEGER NOT NULL,
  query TEXT NOT NULL, budget INTEGER NOT NULL,
  hot_set_version INTEGER,
  selected_tokens INTEGER, raw_tokens INTEGER,
  pack_json TEXT NOT NULL, surface TEXT NOT NULL
);
"#;

const FTS_SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(
  symbol, path, body, node_id UNINDEXED,
  tokenize = "unicode61 tokenchars '_'"
);
"#;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Store {
    /// Open (creating if needed) the store for a repo root.
    pub fn open(root: &Path) -> Result<Store> {
        let dir = root.join(crate::STATE_DIR);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let conn = Connection::open(dir.join(crate::DB_FILE))?;
        conn.execute_batch(SCHEMA)?;
        let fts_enabled = match conn.execute_batch(FTS_SCHEMA) {
            Ok(()) => true,
            Err(_) => false, // SQLite built without FTS5: fall back to LIKE search
        };
        let store = Store {
            conn,
            root: root.to_path_buf(),
            fts_enabled,
        };
        if store.meta_get("schema_version").is_none() {
            store.meta_set("schema_version", &crate::SCHEMA_VERSION.to_string())?;
            store.meta_set("norm_version", crate::NORM_VERSION)?;
        }
        Ok(store)
    }

    /// Open an existing store; error with an actionable message when absent.
    pub fn open_existing(root: &Path) -> Result<Store> {
        let db = root.join(crate::STATE_DIR).join(crate::DB_FILE);
        if !db.exists() {
            return Err(anyhow!(
                "no PackMind index found at {} — run: packmind index .",
                root.display()
            ));
        }
        Store::open(root)
    }

    // ---------- meta ----------

    pub fn meta_get(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn meta_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------- nodes ----------

    /// Insert or refresh a node. Returns true when the node is new.
    /// Content-addressed: re-upserting identical content refreshes position
    /// metadata and revalidates, but never duplicates.
    pub fn upsert_node(&self, n: &Node) -> Result<bool> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT valid FROM nodes WHERE id=?1",
                [n.id.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let is_new = existing.is_none();
        self.conn.execute(
            "INSERT INTO nodes(id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                               line_start,line_end,tokens,content,valid,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,1,?14)
             ON CONFLICT(id) DO UPDATE SET
               path=excluded.path, symbol=excluded.symbol, role=excluded.role,
               byte_start=excluded.byte_start, byte_end=excluded.byte_end,
               line_start=excluded.line_start, line_end=excluded.line_end,
               valid=1, superseded_by=NULL, staled_at=NULL",
            params![
                n.id.as_slice(),
                n.hash.as_slice(),
                n.kind.as_i64(),
                n.path,
                n.symbol,
                n.lang,
                n.role,
                n.byte_start,
                n.byte_end,
                n.line_start,
                n.line_end,
                n.tokens,
                n.content,
                now(),
            ],
        )?;
        Ok(is_new)
    }

    pub fn get_node(&self, id: &NodeId) -> Option<Node> {
        self.conn
            .query_row(
                "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                        line_start,line_end,tokens,content,valid,centrality
                 FROM nodes WHERE id=?1",
                [id.as_slice()],
                row_to_node,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn nodes_by_path(&self, path: &str, kinds: &[NodeKind]) -> Result<Vec<Node>> {
        let kind_list = kinds
            .iter()
            .map(|k| k.as_i64().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                    line_start,line_end,tokens,content,valid,centrality
             FROM nodes WHERE path=?1 AND valid=1 AND kind IN ({kind_list})
             ORDER BY byte_start"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([path], row_to_node)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Paths whose tail matches `suffix` (e.g. "validator.py" or "payments/validator.py").
    pub fn paths_matching(&self, suffix: &str, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files WHERE skipped IS NULL AND (path=?1 OR path LIKE '%/' || ?1)
             ORDER BY length(path) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![suffix, limit as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Valid AST chunks whose symbol equals `name` or ends with `.name`.
    pub fn find_symbol(&self, name: &str, limit: usize) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                    line_start,line_end,tokens,content,valid,centrality
             FROM nodes
             WHERE valid=1 AND kind=2 AND (symbol=?1 OR symbol LIKE '%.' || ?1)
             ORDER BY centrality DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![name, limit as i64], row_to_node)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn signature_for(&self, path: &str, symbol: &str) -> Option<Node> {
        self.conn
            .query_row(
                "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                        line_start,line_end,tokens,content,valid,centrality
                 FROM nodes WHERE path=?1 AND symbol=?2 AND kind=3 AND valid=1",
                params![path, symbol],
                row_to_node,
            )
            .optional()
            .ok()
            .flatten()
    }

    /// The AST chunk a signature node was derived from.
    pub fn chunk_for(&self, path: &str, symbol: &str) -> Option<Node> {
        self.conn
            .query_row(
                "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                        line_start,line_end,tokens,content,valid,centrality
                 FROM nodes WHERE path=?1 AND symbol=?2 AND kind=2 AND valid=1",
                params![path, symbol],
                row_to_node,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn file_node(&self, path: &str) -> Option<Node> {
        self.conn
            .query_row(
                "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                        line_start,line_end,tokens,content,valid,centrality
                 FROM nodes WHERE path=?1 AND kind=1 AND valid=1",
                [path],
                row_to_node,
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Mark a node stale unless another file still references it
    /// (identical chunks can be shared by multiple files — content addressing).
    pub fn mark_stale(&self, id: &NodeId, except_path: &str) -> Result<bool> {
        let hexid = id_hex(id);
        let still_referenced: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE path != ?1 AND merkle_json LIKE '%' || ?2 || '%'",
            params![except_path, hexid],
            |r| r.get(0),
        )?;
        if still_referenced > 0 {
            return Ok(false);
        }
        self.conn.execute(
            "UPDATE nodes SET valid=0, staled_at=?2 WHERE id=?1",
            params![id.as_slice(), now()],
        )?;
        self.fts_delete(id)?;
        Ok(true)
    }

    /// Stale all valid nodes of `kind` at `path` whose id is not in `keep`.
    /// Used for signatures / doc chunks / file nodes after re-indexing a file.
    pub fn stale_others(&self, path: &str, kind: NodeKind, keep: &HashSet<NodeId>) -> Result<i64> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM nodes WHERE path=?1 AND kind=?2 AND valid=1")?;
        let ids: Vec<NodeId> = stmt
            .query_map(params![path, kind.as_i64()], |r| {
                let v: Vec<u8> = r.get(0)?;
                Ok(v)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|v| v.try_into().ok())
            .collect();
        let mut n = 0;
        for id in ids {
            if !keep.contains(&id) {
                self.conn.execute(
                    "UPDATE nodes SET valid=0, staled_at=?2 WHERE id=?1",
                    params![id.as_slice(), now()],
                )?;
                self.fts_delete(&id)?;
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn set_centrality_bulk(&mut self, values: &[(NodeId, f64)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE nodes SET centrality=?2 WHERE id=?1")?;
            for (id, c) in values {
                stmt.execute(params![id.as_slice(), c])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---------- full-text search ----------

    pub fn fts_replace(&self, n: &Node) -> Result<()> {
        if !self.fts_enabled {
            return Ok(());
        }
        let hexid = id_hex(&n.id);
        self.conn
            .execute("DELETE FROM search WHERE node_id=?1", [&hexid])?;
        self.conn.execute(
            "INSERT INTO search(symbol,path,body,node_id) VALUES(?1,?2,?3,?4)",
            params![
                n.symbol.clone().unwrap_or_default(),
                n.path,
                n.content,
                hexid
            ],
        )?;
        Ok(())
    }

    pub fn fts_delete(&self, id: &NodeId) -> Result<()> {
        if !self.fts_enabled {
            return Ok(());
        }
        self.conn
            .execute("DELETE FROM search WHERE node_id=?1", [id_hex(id)])?;
        Ok(())
    }

    /// Lexical search. Returns (node, rank_score in 0..1, matched_term).
    pub fn search_text(&self, query: &str, limit: usize) -> Result<Vec<(Node, f64, String)>> {
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| t.len() >= 2)
            .map(|t| t.to_string())
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        if self.fts_enabled {
            let match_expr = terms
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "")))
                .collect::<Vec<_>>()
                .join(" OR ");
            let mut stmt = self.conn.prepare(
                "SELECT node_id, bm25(search) FROM search WHERE search MATCH ?1
                 ORDER BY bm25(search) LIMIT ?2",
            )?;
            let rows: Vec<(String, f64)> = stmt
                .query_map(params![match_expr, limit as i64], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            let n = rows.len().max(1) as f64;
            for (i, (hexid, _bm25)) in rows.into_iter().enumerate() {
                if let Some(id) = crate::model::id_from_hex(&hexid) {
                    if let Some(node) = self.get_node(&id) {
                        if node.valid {
                            let score = 1.0 - (i as f64) / n;
                            let matched = terms
                                .iter()
                                .find(|t| {
                                    node.content.contains(*t)
                                        || node
                                            .symbol
                                            .as_deref()
                                            .map(|s| s.contains(*t))
                                            .unwrap_or(false)
                                })
                                .cloned()
                                .unwrap_or_else(|| terms[0].clone());
                            out.push((node, score, matched));
                        }
                    }
                }
            }
        } else {
            // LIKE fallback when SQLite lacks FTS5.
            let like = format!("%{}%", terms[0]);
            let mut stmt = self.conn.prepare(
                "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                        line_start,line_end,tokens,content,valid,centrality
                 FROM nodes WHERE valid=1 AND kind IN (2,3,4)
                   AND (symbol LIKE ?1 OR content LIKE ?1)
                 ORDER BY centrality DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![like, limit as i64], row_to_node)?;
            for (i, node) in rows.filter_map(|r| r.ok()).enumerate() {
                let score = 1.0 - (i as f64) / (limit as f64);
                out.push((node, score, terms[0].clone()));
            }
        }
        Ok(out)
    }

    // ---------- edges ----------

    pub fn add_edge(&self, src: &NodeId, kind: EdgeKind, dst: &NodeId, weight: f64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO edges(src,kind,dst,weight) VALUES(?1,?2,?3,?4)
             ON CONFLICT(src,kind,dst) DO UPDATE SET weight=MAX(weight, excluded.weight)",
            params![src.as_slice(), kind.as_i64(), dst.as_slice(), weight],
        )?;
        Ok(())
    }

    pub fn out_edges(
        &self,
        src: &NodeId,
        kinds: &[EdgeKind],
    ) -> Result<Vec<(EdgeKind, NodeId, f64)>> {
        self.edges_query("src", "dst", src, kinds)
    }

    pub fn in_edges(
        &self,
        dst: &NodeId,
        kinds: &[EdgeKind],
    ) -> Result<Vec<(EdgeKind, NodeId, f64)>> {
        self.edges_query("dst", "src", dst, kinds)
    }

    fn edges_query(
        &self,
        from_col: &str,
        to_col: &str,
        id: &NodeId,
        kinds: &[EdgeKind],
    ) -> Result<Vec<(EdgeKind, NodeId, f64)>> {
        let kind_list = kinds
            .iter()
            .map(|k| k.as_i64().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT kind, {to_col}, weight FROM edges WHERE {from_col}=?1 AND kind IN ({kind_list})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([id.as_slice()], |r| {
            let kind: i64 = r.get(0)?;
            let other: Vec<u8> = r.get(1)?;
            let weight: f64 = r.get(2)?;
            Ok((kind, other, weight))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter_map(|(k, other, w)| {
                let kind = EdgeKind::from_i64(k)?;
                let id: NodeId = other.try_into().ok()?;
                Some((kind, id, w))
            })
            .collect())
    }

    /// Degree of every node that participates in any edge.
    pub fn degree_counts(&self) -> Result<Vec<(NodeId, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, SUM(c) FROM (
               SELECT src AS id, COUNT(*) AS c FROM edges GROUP BY src
               UNION ALL
               SELECT dst AS id, COUNT(*) AS c FROM edges GROUP BY dst
             ) GROUP BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: Vec<u8> = r.get(0)?;
            let c: i64 = r.get(1)?;
            Ok((id, c))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter_map(|(v, c)| Some((v.try_into().ok()?, c)))
            .collect())
    }

    // ---------- files ----------

    pub fn get_file(&self, path: &str) -> Option<FileRow> {
        self.conn
            .query_row(
                "SELECT path,file_node,merkle_json,mtime,size,content_sha,skipped
                 FROM files WHERE path=?1",
                [path],
                row_to_file,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn all_files(&self) -> Result<Vec<FileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT path,file_node,merkle_json,mtime,size,content_sha,skipped FROM files",
        )?;
        let rows = stmt.query_map([], row_to_file)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn upsert_file(&self, row: &FileRow) -> Result<()> {
        let merkle_json = serde_json::to_string(
            &row.merkle
                .iter()
                .map(|(s, e, id)| (s, e, id_hex(id)))
                .collect::<Vec<_>>(),
        )?;
        self.conn.execute(
            "INSERT INTO files(path,file_node,merkle_json,mtime,size,content_sha,skipped)
             VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(path) DO UPDATE SET
               file_node=excluded.file_node, merkle_json=excluded.merkle_json,
               mtime=excluded.mtime, size=excluded.size,
               content_sha=excluded.content_sha, skipped=excluded.skipped",
            params![
                row.path,
                row.file_node.map(|i| i.to_vec()),
                merkle_json,
                row.mtime,
                row.size,
                row.content_sha,
                row.skipped
            ],
        )?;
        Ok(())
    }

    pub fn delete_file(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM files WHERE path=?1", [path])?;
        Ok(())
    }

    // ---------- hot set ----------

    pub fn hot_set(&self) -> Result<(i64, Vec<NodeId>)> {
        let version: i64 = self
            .meta_get("hot_set_version")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut stmt = self
            .conn
            .prepare("SELECT node_id FROM hot_set WHERE version=?1 ORDER BY position")?;
        let rows = stmt.query_map([version], |r| r.get::<_, Vec<u8>>(0))?;
        let ids = rows
            .filter_map(|r| r.ok())
            .filter_map(|v| v.try_into().ok())
            .collect();
        Ok((version, ids))
    }

    pub fn replace_hot_set(&mut self, ids: &[NodeId]) -> Result<i64> {
        let version: i64 = self
            .meta_get("hot_set_version")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
            + 1;
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO hot_set(version,position,node_id) VALUES(?1,?2,?3)")?;
            for (i, id) in ids.iter().enumerate() {
                stmt.execute(params![version, i as i64, id.as_slice()])?;
            }
            tx.execute("DELETE FROM hot_set WHERE version < ?1", [version - 2])?;
        }
        tx.commit()?;
        self.meta_set("hot_set_version", &version.to_string())?;
        Ok(version)
    }

    // ---------- packs ----------

    pub fn record_pack(&self, pack: &ContextPack, surface: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO packs(id,ts,query,budget,hot_set_version,selected_tokens,raw_tokens,pack_json,surface)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                pack.pack_id,
                now(),
                pack.query,
                pack.token_budget,
                pack.layout.hot_set_version,
                pack.totals.selected_tokens,
                pack.totals.estimated_raw_tokens,
                serde_json::to_string(pack)?,
                surface
            ],
        )?;
        // Flight-recorder cap: keep the most recent 10k packs.
        self.conn.execute(
            "DELETE FROM packs WHERE id NOT IN (SELECT id FROM packs ORDER BY ts DESC LIMIT 10000)",
            [],
        )?;
        Ok(())
    }

    pub fn get_pack(&self, id: &str) -> Option<String> {
        self.conn
            .query_row("SELECT pack_json FROM packs WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    /// Most recent recorded packs, newest first: (pack_json, surface).
    pub fn recent_packs(&self, limit: usize) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT pack_json, surface FROM packs ORDER BY ts DESC, id DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn bump_stats(&self, ids: &[NodeId]) -> Result<()> {
        for id in ids {
            self.conn.execute(
                "INSERT INTO stats(node_id,hits,last_used,ewma) VALUES(?1,1,?2,1.0)
                 ON CONFLICT(node_id) DO UPDATE SET
                   hits=hits+1, last_used=?2, ewma=ewma*0.9 + 1.0",
                params![id.as_slice(), now()],
            )?;
        }
        Ok(())
    }

    /// All valid nodes of the given kinds (graph export, demo rendering).
    pub fn valid_nodes(&self, kinds: &[NodeKind]) -> Result<Vec<Node>> {
        let ks: Vec<String> = kinds.iter().map(|k| k.as_i64().to_string()).collect();
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                    line_start,line_end,tokens,content,valid,centrality
             FROM nodes WHERE valid=1 AND kind IN ({}) ORDER BY path, line_start",
            ks.join(",")
        ))?;
        let rows = stmt.query_map([], row_to_node)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Every edge in the graph: (src, kind, dst).
    pub fn all_edges(&self) -> Result<Vec<(NodeId, EdgeKind, NodeId)>> {
        let mut stmt = self.conn.prepare("SELECT src, kind, dst FROM edges")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter_map(|(s, k, d)| {
                Some((
                    NodeId::try_from(s).ok()?,
                    EdgeKind::from_i64(k)?,
                    NodeId::try_from(d).ok()?,
                ))
            })
            .collect())
    }

    /// Top valid AST chunks by centrality (hot-set construction, impact ranking).
    pub fn top_chunks(&self, limit: usize) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,hash,kind,path,symbol,lang,role,byte_start,byte_end,
                    line_start,line_end,tokens,content,valid,centrality
             FROM nodes WHERE valid=1 AND kind=2
             ORDER BY centrality DESC, tokens ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], row_to_node)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// (id, symbol, path, role) for every valid AST chunk with a symbol.
    /// Used to build the symbol resolution map during edge extraction.
    pub fn symbol_entries(&self) -> Result<Vec<(NodeId, String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, symbol, path, role FROM nodes
             WHERE valid=1 AND kind=2 AND symbol IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: Vec<u8> = r.get(0)?;
            Ok((
                id,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter_map(|(v, s, p, role)| Some((v.try_into().ok()?, s, p, role)))
            .collect())
    }

    // ---------- maintenance ----------

    pub fn counts(&self) -> Result<Counts> {
        let q = |sql: &str| -> i64 { self.conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
        Ok(Counts {
            files: q("SELECT COUNT(*) FROM files WHERE skipped IS NULL"),
            skipped_files: q("SELECT COUNT(*) FROM files WHERE skipped IS NOT NULL"),
            chunks: q("SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=2"),
            signatures: q("SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=3"),
            docs: q("SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=4"),
            edges: q("SELECT COUNT(*) FROM edges"),
        })
    }

    /// Drop stale nodes older than `days` and their edges; vacuum.
    pub fn gc(&self, days: i64) -> Result<(i64, i64)> {
        let cutoff = now() - days * 86_400;
        let nodes = self.conn.execute(
            "DELETE FROM nodes WHERE valid=0 AND staled_at IS NOT NULL AND staled_at < ?1",
            [cutoff],
        )? as i64;
        let edges = self.conn.execute(
            "DELETE FROM edges WHERE src NOT IN (SELECT id FROM nodes)
                               OR dst NOT IN (SELECT id FROM nodes)",
            [],
        )? as i64;
        self.conn.execute_batch("VACUUM;")?;
        Ok((nodes, edges))
    }
}

fn row_to_node(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    let id: Vec<u8> = r.get(0)?;
    let hash: Vec<u8> = r.get(1)?;
    let kind: i64 = r.get(2)?;
    Ok(Node {
        id: id.try_into().unwrap_or([0u8; 16]),
        hash: hash.try_into().unwrap_or([0u8; 32]),
        kind: NodeKind::from_i64(kind).unwrap_or(NodeKind::File),
        path: r.get(3)?,
        symbol: r.get(4)?,
        lang: r.get(5)?,
        role: r.get(6)?,
        byte_start: r.get(7)?,
        byte_end: r.get(8)?,
        line_start: r.get(9)?,
        line_end: r.get(10)?,
        tokens: r.get(11)?,
        content: r.get(12)?,
        valid: r.get::<_, i64>(13)? != 0,
        centrality: r.get(14)?,
    })
}

fn row_to_file(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileRow> {
    let file_node: Option<Vec<u8>> = r.get(1)?;
    let merkle_json: String = r.get(2)?;
    let merkle: Vec<(i64, i64, String)> = serde_json::from_str(&merkle_json).unwrap_or_default();
    Ok(FileRow {
        path: r.get(0)?,
        file_node: file_node.and_then(|v| v.try_into().ok()),
        merkle: merkle
            .into_iter()
            .filter_map(|(s, e, h)| Some((s, e, crate::model::id_from_hex(&h)?)))
            .collect(),
        mtime: r.get(3)?,
        size: r.get(4)?,
        content_sha: r.get(5)?,
        skipped: r.get(6)?,
    })
}
