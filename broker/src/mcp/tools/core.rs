use std::time::Duration;

use rbx_dom_weak::types::Ref;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::{
    mcp::RoprobeMcp,
    protocol::{LogLevel, OpResult, Operation, RemoteDirection},
    server::{OpRequest, SessionDom, SessionId},
};

const TREE_KIDS: usize = 100;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetActiveArgs {
    /// Session id from list-sessions.
    id: u32,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ExecArgs {
    /// Luau source to run.
    source: String,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct EvalArgs {
    /// Luau source to run; `return` or print to get output.
    source: String,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
    /// Seconds to wait for a result (default 15).
    timeout: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ConsoleArgs {
    /// Max lines, newest last (default 50).
    limit: Option<usize>,
    /// Only this level: print, info, warn or error.
    level: Option<String>,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SpyArgs {
    /// list (default), status or clear.
    op: Option<String>,
    /// list: only remotes whose path contains this.
    filter: Option<String>,
    /// list: max calls, newest last (default 50).
    limit: Option<usize>,
    /// list: full detail (args, returns, source) of one call id.
    call: Option<u32>,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// QueryDescendants selector, e.g. `Part.Enemy` or `#Name > Folder`.
    selector: String,
    /// Max results (default 100).
    limit: Option<u32>,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TreeArgs {
    /// Client id of the node to start at; defaults to the top level.
    root: Option<String>,
    /// Levels to expand (default 3, max 6).
    depth: Option<usize>,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct LockArgs {
    /// Session id from list-sessions.
    id: u32,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct InfoArgs {
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

fn level_of(s: &str) -> Option<LogLevel> {
    match s.to_ascii_lowercase().as_str() {
        "print" => Some(LogLevel::Print),
        "info" => Some(LogLevel::Info),
        "warn" => Some(LogLevel::Warn),
        "error" => Some(LogLevel::Error),
        _ => None,
    }
}

fn level_name(l: LogLevel) -> &'static str {
    match l {
        LogLevel::Print => "print",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

pub(super) fn luau_str(s: &str) -> String {
    let body = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r");
    format!("\"{body}\"")
}

fn tree_node(d: &SessionDom, r: Ref, depth: usize) -> Value {
    let Some(i) = d.dom().get_by_ref(r) else { return Value::Null };
    let mut v = json!({"id": d.id_of(r), "name": i.name, "class": i.class.as_str()});
    let kids = i.children();
    if depth == 0 || kids.is_empty() {
        if d.has_children(r) {
            v["more"] = true.into();
        }
        return v;
    }
    let mut out: Vec<Value> = kids.iter().take(TREE_KIDS).map(|c| tree_node(d, *c, depth - 1)).collect();
    if kids.len() > TREE_KIDS {
        out.push(json!({"truncated": kids.len() - TREE_KIDS}));
    }
    v["children"] = out.into();
    v
}

impl RoprobeMcp {
    async fn resolve(&self, session: Option<u32>) -> Result<SessionId, String> {
        match session {
            Some(id) => Ok(SessionId(id)),
            None => {
                let holder = self.ctx.sessions.holder.read().await;
                holder.current().map(|s| s.id).ok_or_else(|| "no active session".to_string())
            }
        }
    }

    pub(super) async fn pick(&self, session: Option<u32>) -> Result<SessionId, String> {
        let id = self.resolve(session).await?;
        if self.ctx.sessions.holder.read().await.find(id).is_none() {
            self.locks.clear_session(id).await; // ghost locks begone
        }
        self.locks.check(id, &self.agent_id).await?;
        Ok(id)
    }

    pub(super) async fn wait(rx: oneshot::Receiver<OpResult>, secs: u64) -> Result<String, String> {
        match tokio::time::timeout(Duration::from_secs(secs), rx).await {
            Err(_) => Err("timed out".into()),
            Ok(Err(_)) => Err("session disconnected before answering".into()),
            Ok(Ok(OpResult::Output(s))) => Ok(s),
            Ok(Ok(OpResult::Ok)) => Ok(String::new()),
            Ok(Ok(OpResult::Err(e))) => Err(e),
            Ok(Ok(OpResult::Reads(r))) => Ok(format!("{r:?}")),
        }
    }

    pub(super) async fn sink(&self, session: Option<u32>) -> Result<tokio::sync::mpsc::Sender<OpRequest>, String> {
        let id = self.pick(session).await?;
        let holder = self.ctx.sessions.holder.read().await;
        let s = holder.find(id).ok_or("no such session (or none active)")?;
        s.mirror.op_sink().ok_or_else(|| "session has no live op sink".to_string())
    }

    async fn run(&self, source: String, session: Option<u32>) -> Result<oneshot::Receiver<OpResult>, String> {
        let sink = self.sink(session).await?;
        let (reply, rx) = oneshot::channel();
        let op = OpRequest { operation: Operation::RunCode { source }, reply };
        sink.send(op).await.map_err(|_| "session stopped accepting ops")?;
        Ok(rx)
    }
}

#[tool_router(router = core_router, vis = "pub(crate)")]
impl RoprobeMcp {
    #[tool(name = "list-sessions", description = "List connected Roblox sessions and which is active.")]
    async fn list_sessions(&self) -> Result<String, String> {
        let list = self.ctx.sessions.holder.read().await.list();
        let mut out = serde_json::to_value(&list).map_err(|e| e.to_string())?;
        for s in out.as_array_mut().into_iter().flatten() {
            let Some(id) = s["id"].as_u64().and_then(|n| u32::try_from(n).ok()) else { continue };
            if let Some(owner) = self.locks.get_owner(SessionId(id)).await {
                s["lockedBy"] = json!({"agent": owner, "you": owner == self.agent_id});
            }
        }
        Ok(out.to_string())
    }

    #[tool(name = "set-active", description = "Make a session the active one.")]
    async fn set_active(&self, Parameters(a): Parameters<SetActiveArgs>) -> Result<String, String> {
        let id = self.ctx.sessions.set_current(Some(SessionId(a.id))).await.map_err(|e| e.to_string())?;
        Ok(format!("active: {}", id.map_or(0, |i| i.0)))
    }

    #[tool(name = "execute", description = "Run Luau in a session, fire and forget. Use eval for output.")]
    async fn execute(&self, Parameters(a): Parameters<ExecArgs>) -> Result<String, String> {
        let rx = self.run(a.source, a.session).await?;
        tokio::spawn(rx); // yeet the result
        Ok("sent".into())
    }

    #[tool(name = "eval", description = "Run Luau in a session and wait for its output.")]
    async fn eval(&self, Parameters(a): Parameters<EvalArgs>) -> Result<String, String> {
        let rx = self.run(a.source, a.session).await?;
        Self::wait(rx, a.timeout.unwrap_or(15)).await
    }

    #[tool(name = "console", description = "Read buffered console output (newest last), optionally by level.")]
    async fn console(&self, Parameters(a): Parameters<ConsoleArgs>) -> Result<String, String> {
        let id = self.pick(a.session).await?;
        let level = match a.level.as_deref() {
            Some(l) => Some(level_of(l).ok_or("level must be print, info, warn or error")?),
            None => None,
        };
        let mut lines: Vec<String> = self
            .ctx
            .sessions
            .log_history()
            .into_iter()
            .filter(|(sid, _)| *sid == id)
            .flat_map(|(_, es)| es)
            .filter(|e| level.is_none_or(|l| e.level == l))
            .map(|e| format!("[{}] {}", level_name(e.level), e.content))
            .collect();
        let skip = lines.len().saturating_sub(a.limit.unwrap_or(50));
        lines.drain(..skip);
        Ok(lines.join("\n"))
    }

    #[tool(name = "remote-spy", description = "Captured remote calls. op: list (default), status, or clear.")]
    async fn remote_spy(&self, Parameters(a): Parameters<SpyArgs>) -> Result<String, String> {
        let sessions = &self.ctx.sessions;
        match a.op.as_deref().unwrap_or("list") {
            "status" => {
                let buffered: usize = sessions.remote_history().iter().map(|(_, c)| c.len()).sum();
                Ok(json!({"config": sessions.spy(), "buffered": buffered}).to_string())
            }
            "clear" => {
                let dropped = sessions.clear_remotes();
                sessions.broadcast_remotes_cleared();
                Ok(format!("dropped {dropped}"))
            }
            "list" => {
                let id = self.pick(a.session).await?;
                let mut calls = sessions
                    .remote_history()
                    .into_iter()
                    .filter(|(sid, _)| *sid == id)
                    .flat_map(|(_, cs)| cs)
                    .filter(|c| a.call.is_none_or(|n| c.id == n))
                    .filter(|c| a.filter.as_deref().is_none_or(|f| c.remote.path.contains(f)))
                    .collect::<Vec<_>>();
                if a.call.is_some() {
                    return match calls.pop() {
                        Some(c) => serde_json::to_string(&c).map_err(|e| e.to_string()),
                        None => Err("no such call".into()),
                    };
                }
                let skip = calls.len().saturating_sub(a.limit.unwrap_or(50));
                let out: Vec<Value> = calls
                    .drain(skip..)
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "dir": if c.direction == RemoteDirection::Outgoing { "out" } else { "in" },
                            "remote": c.remote.path,
                            "class": c.remote.class,
                            "method": c.method,
                            "args": c.arguments.len(),
                            "script": c.source.script,
                            "line": c.source.line,
                            "t": c.timestamp,
                        })
                    })
                    .collect();
                Ok(Value::Array(out).to_string())
            }
            other => Err(format!("unknown op: {other}")),
        }
    }

    #[tool(name = "search", description = "Find instances with a QueryDescendants selector; returns class and full path.")]
    async fn search(&self, Parameters(a): Parameters<SearchArgs>) -> Result<String, String> {
        let src = format!(
            "local ok,r=pcall(function()return game:QueryDescendants({sel})end)\n\
             if not ok then error(r,0)end\n\
             local o={{}}for i=1,math.min(#r,{lim})do o[i]=r[i].ClassName..' '..r[i]:GetFullName()end\n\
             return #r..' matches\\n'..table.concat(o,'\\n')",
            sel = luau_str(&a.selector),
            lim = a.limit.unwrap_or(100),
        );
        let rx = self.run(src, a.session).await?;
        Self::wait(rx, 15).await
    }

    #[tool(name = "tree", description = "Instance tree from the DOM mirror. Nodes marked more have unloaded children.")]
    async fn tree(&self, Parameters(a): Parameters<TreeArgs>) -> Result<String, String> {
        let id = self.pick(a.session).await?;
        let depth = a.depth.unwrap_or(3).min(6);
        let holder = self.ctx.sessions.holder.read().await;
        let s = holder.find(id).ok_or("no such session")?;
        s.mirror.with_dom(|d| -> Result<String, String> {
            let v = match a.root.as_deref() {
                Some(rid) => {
                    let r = d.get(rid).ok_or_else(|| format!("no node {rid}"))?.referent();
                    tree_node(d, r, depth)
                }
                None => {
                    let kids = d.dom().root().children().to_vec();
                    Value::Array(kids.into_iter().map(|c| tree_node(d, c, depth.saturating_sub(1))).collect())
                }
            };
            Ok(v.to_string())
        })
    }

    #[tool(name = "game-info", description = "PlaceId, GameId, JobId, name and creator of the game.")]
    async fn game_info(&self, Parameters(a): Parameters<InfoArgs>) -> Result<String, String> {
        let src = r#"local ok,p=pcall(function()return game:GetService("MarketplaceService"):GetProductInfo(game.PlaceId)end)
return game:GetService("HttpService"):JSONEncode({placeId=game.PlaceId,gameId=game.GameId,jobId=game.JobId,placeVersion=game.PlaceVersion,name=ok and p.Name or nil,creator=ok and p.Creator and p.Creator.Name or nil})"#;
        let rx = self.run(src.into(), a.session).await?;
        Self::wait(rx, 15).await
    }

    #[tool(name = "lock-session", description = "Claim a session; other agents get errors targeting it until unlocked.")]
    async fn lock_session(&self, Parameters(a): Parameters<LockArgs>) -> Result<String, String> {
        let id = SessionId(a.id);
        self.ctx.sessions.holder.read().await.find(id).ok_or("no such session")?;
        self.locks.lock(id, &self.agent_id).await?;
        Ok(format!("locked {}", a.id))
    }

    #[tool(name = "unlock-session", description = "Release a session you locked.")]
    async fn unlock_session(&self, Parameters(a): Parameters<LockArgs>) -> Result<String, String> {
        self.locks.unlock(SessionId(a.id), &self.agent_id).await?;
        Ok(format!("unlocked {}", a.id))
    }
}
