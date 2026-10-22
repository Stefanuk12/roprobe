use rmcp::{Peer, RoleServer, handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    mcp::{RoprobeMcp, bytecode_parser, tools::core::luau_str},
    protocol::Operation,
};

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ScriptArgs {
    /// Full path of the script, e.g. `game.ReplicatedStorage.Mod`.
    script: String,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
    /// Seconds to wait (default 15).
    timeout: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ClosureArgs {
    /// Path to a function or script, as the client resolves it.
    path: String,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
    /// Seconds to wait (default 15).
    timeout: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct HookArgs {
    /// Full path of the remote, e.g. `game.ReplicatedStorage.Remotes.Fire`.
    remote: String,
    /// log (default) or block.
    mode: Option<String>,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UnhookArgs {
    /// Full path of the hooked remote.
    remote: String,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TraceArgs {
    /// Path to the function to trace, as the client resolves it.
    target: String,
    /// Start (default) or stop tracing.
    enabled: Option<bool>,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Plain text to look for (not a Lua pattern).
    pattern: String,
    /// Max results (default 100).
    limit: Option<u32>,
    /// Target session id; defaults to the active one.
    session: Option<u32>,
    /// Seconds to wait (default 60).
    timeout: Option<u64>,
}

impl RoprobeMcp {
    async fn ask(&self, op: Operation, session: Option<u32>, secs: u64) -> Result<String, String> {
        let sink = self.sink(session).await?;
        let (reply, rx) = tokio::sync::oneshot::channel();
        sink.send(crate::server::OpRequest { operation: op, reply })
            .await
            .map_err(|_| "session stopped accepting ops")?;
        Self::wait(rx, secs).await
    }

    async fn closure(&self, a: ClosureArgs) -> Result<Value, String> {
        let out = self
            .ask(Operation::InspectClosure { path: a.path }, a.session, a.timeout.unwrap_or(15))
            .await?;
        serde_json::from_str(&out).map_err(|e| format!("bad closure reply: {e}"))
    }
}

#[tool_router(router = bytecode_router, vis = "pub(crate)")]
impl RoprobeMcp {
    #[tool(name = "disassemble", description = "Fetch a script's compiled Luau bytecode: version, size, base64.")]
    async fn disassemble(&self, Parameters(a): Parameters<ScriptArgs>) -> Result<String, String> {
        let op = Operation::GetBytecode { script_path: a.script };
        let out = self.ask(op, a.session, a.timeout.unwrap_or(15)).await?;
        Ok(bytecode_parser::summarize(&out)?.to_string())
    }

    #[tool(name = "constants", description = "Constants of a closure.")]
    async fn constants(&self, Parameters(a): Parameters<ClosureArgs>) -> Result<String, String> {
        let v = self.closure(a).await?;
        Ok(json!({"constants": v["constants"]}).to_string())
    }

    #[tool(name = "upvalues", description = "Upvalues of a closure.")]
    async fn upvalues(&self, Parameters(a): Parameters<ClosureArgs>) -> Result<String, String> {
        let v = self.closure(a).await?;
        Ok(json!({"upvalues": v["upvalues"]}).to_string())
    }

    #[tool(name = "inspect-closure", description = "Name, params, constants, upvalues, protos and source of a closure.")]
    async fn inspect_closure(&self, Parameters(a): Parameters<ClosureArgs>) -> Result<String, String> {
        Ok(self.closure(a).await?.to_string())
    }

    #[tool(name = "hook-remote", description = "Hook a remote; calls stream back as logging notifications (logger roprobe.hook). mode: log or block.")]
    async fn hook_remote(&self, peer: Peer<RoleServer>, Parameters(a): Parameters<HookArgs>) -> Result<String, String> {
        let mode = match a.mode.as_deref().unwrap_or("log") {
            "log" => 0,
            "block" => 1,
            m => return Err(format!("mode must be log or block, got {m}")),
        };
        let id = self.pick(a.session).await?;
        let op = Operation::HookRemote { remote_path: a.remote, mode };
        self.ask(op, Some(id.0), 15).await?;
        self.streams.watch_hooks(peer, &self.ctx.sessions, id);
        Ok("hooked".into())
    }

    #[tool(name = "unhook-remote", description = "Drop a remote's hook.")]
    async fn unhook_remote(&self, Parameters(a): Parameters<UnhookArgs>) -> Result<String, String> {
        self.ask(Operation::UnhookRemote { remote_path: a.remote }, a.session, 15).await?;
        Ok("unhooked".into())
    }

    #[tool(name = "trace", description = "Start or stop tracing a function; calls stream back as logging notifications (logger roprobe.trace).")]
    async fn trace(&self, peer: Peer<RoleServer>, Parameters(a): Parameters<TraceArgs>) -> Result<String, String> {
        let on = a.enabled.unwrap_or(true);
        let id = self.pick(a.session).await?;
        self.ask(Operation::Trace { target_path: a.target, enabled: on }, Some(id.0), 15).await?;
        if on {
            self.streams.watch_traces(peer, &self.ctx.sessions, id);
        }
        Ok(if on { "tracing" } else { "stopped" }.into())
    }

    #[tool(name = "bytecode-search", description = "Find scripts whose compiled bytecode contains some text.")]
    async fn bytecode_search(&self, Parameters(a): Parameters<SearchArgs>) -> Result<String, String> {
        let src = format!(
            "local p,lim=ARGPAT,{lim}
             local r={{}}
             for _,s in getscripts()do
             local ok,bc=pcall(getscriptbytecode,s)
             if ok and type(bc)=='string' and bc:find(p,1,true)then
             r[#r+1]=s.ClassName..' '..s:GetFullName()
             if #r>=lim then break end end end
             return #r..' matches\n'..table.concat(r,'\n')",
            lim = a.limit.unwrap_or(100),
        )
        .replace("ARGPAT", &luau_str(&a.pattern));
        self.ask(Operation::RunCode { source: src }, a.session, a.timeout.unwrap_or(60)).await
    }

    #[tool(name = "string-scan", description = "Find Luau closures with a string constant containing some text.")]
    async fn string_scan(&self, Parameters(a): Parameters<SearchArgs>) -> Result<String, String> {
        let src = format!(
            "local p,lim=ARGPAT,{lim}
             local r,seen={{}},{{}}
             for _,f in getgc()do
             if type(f)=='function' and not iscclosure(f)then
             local ok,cs=pcall(debug.getconstants,f)
             if ok then for _,c in cs do
             if type(c)=='string' and c:find(p,1,true)then
             local src,name=debug.info(f,'sn')
             local k=tostring(src)..':'..tostring(name)..':'..c
             if not seen[k]then seen[k]=true
             r[#r+1]=tostring(src)..' '..tostring(name)..' = '..c end break end end end end
             if #r>=lim then break end end
             return #r..' matches\n'..table.concat(r,'\n')",
            lim = a.limit.unwrap_or(100),
        )
        .replace("ARGPAT", &luau_str(&a.pattern));
        self.ask(Operation::RunCode { source: src }, a.session, a.timeout.unwrap_or(60)).await
    }
}
