//! Running one script in a fresh, limited QuickJS runtime.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use rquickjs::context::EvalOptions;
use rquickjs::prelude::{Async, Opt};
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Exception, Function, Object};

use crate::model::{ScriptError, ScriptState};
use crate::{Script, ScriptHost, ScriptLimits, native};

/// The Bruno-compatible API, written in JavaScript. Runs before every script.
const PRELUDE: &str = include_str!("prelude.js");

/// dayjs (MIT), vendored so scripts have a small date library (`dayjs()`), as in Bruno.
const DAYJS: &str = include_str!("dayjs.min.js");

/// The file name QuickJS puts in stack traces (`at <eval> (script:3:7)`); used to find the line.
const SCRIPT_FILE: &str = "script";

/// Runs `script` against `state` and returns the changed state. On failure the state still
/// holds everything the script did before it failed (logs, tests, variables).
///
/// QuickJS runtimes are not thread-safe (`!Send`), but Tauri commands and tokio tasks may move
/// between threads at every `.await`. So the script gets a thread of its own
/// (`spawn_blocking`) and drives its async work (`await fri.sendRequest(...)`) there with
/// `Handle::block_on`, which still uses the app's tokio runtime for timers and network I/O.
pub async fn run_script(
    script: Script,
    state: ScriptState,
    host: Arc<dyn ScriptHost>,
    limits: ScriptLimits,
) -> (ScriptState, Option<ScriptError>) {
    let location = format!("{}, {}", script.owner, state.phase.label());
    let original = state.clone();
    let handle = tokio::runtime::Handle::current();
    let joined = tokio::task::spawn_blocking(move || {
        handle.block_on(run_isolated(script.source, state, host, limits))
    })
    .await;
    let (state, failure) = match joined {
        Ok(result) => result,
        Err(err) => (
            None,
            Some(Failure::message(format!("script crashed: {err}"))),
        ),
    };
    let error = failure.map(|f| ScriptError {
        location,
        message: f.message,
        line: f.line,
    });
    (state.unwrap_or(original), error)
}

/// Why a script failed, before the location is added.
#[derive(Debug)]
struct Failure {
    message: String,
    line: Option<u32>,
}

impl Failure {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
        }
    }
}

async fn run_isolated(
    source: String,
    state: ScriptState,
    host: Arc<dyn ScriptHost>,
    limits: ScriptLimits,
) -> (Option<ScriptState>, Option<Failure>) {
    let state_json = match serde_json::to_string(&state) {
        Ok(json) => json,
        Err(err) => return (None, Some(Failure::message(err.to_string()))),
    };
    let runtime = match AsyncRuntime::new() {
        Ok(rt) => rt,
        Err(err) => return (None, Some(Failure::message(err.to_string()))),
    };
    runtime.set_memory_limit(limits.memory_bytes).await;
    runtime.set_max_stack_size(1024 * 1024).await;

    // QuickJS calls this handler regularly while running JS; returning `true` aborts the script.
    // That is how an endless loop is stopped. `Rc<Cell<_>>` is a single-threaded shared flag
    // (fine here: everything runs on this one thread), like a non-volatile `boolean` holder.
    let deadline = Instant::now() + limits.timeout;
    let armed = Rc::new(Cell::new(true));
    let timed_out = Rc::new(Cell::new(false));
    {
        let (armed, timed_out) = (Rc::clone(&armed), Rc::clone(&timed_out));
        runtime
            .set_interrupt_handler(Some(Box::new(move || {
                let stop = armed.get() && Instant::now() > deadline;
                if stop {
                    timed_out.set(true);
                }
                stop
            })))
            .await;
    }
    let context = match AsyncContext::full(&runtime).await {
        Ok(ctx) => ctx,
        Err(err) => return (None, Some(Failure::message(err.to_string()))),
    };

    let run = context.async_with(async |ctx| {
        let failure = execute(&ctx, &state_json, &source, host).await.err();
        // Read the state back even after a failure, with the time limit off.
        armed.set(false);
        let state = ctx
            .eval::<String, _>("__frida.exportState()")
            .ok()
            .and_then(|json| serde_json::from_str::<ScriptState>(&json).ok());
        (state, failure)
    });
    // Also covers time spent awaiting (sleep, sendRequest), where no JS runs to interrupt.
    match tokio::time::timeout(limits.timeout, run).await {
        Ok((state, failure)) => {
            let failure = failure.map(|f| {
                if timed_out.get() {
                    Failure::message(timeout_message(limits))
                } else {
                    f
                }
            });
            (state, failure)
        }
        Err(_) => (None, Some(Failure::message(timeout_message(limits)))),
    }
}

fn timeout_message(limits: ScriptLimits) -> String {
    format!(
        "the script was stopped after {} ms (time limit)",
        limits.timeout.as_millis()
    )
}

/// Sets up the API, then runs the user's script and waits for its async work and tests.
async fn execute<'js>(
    ctx: &Ctx<'js>,
    state_json: &str,
    source: &str,
    host: Arc<dyn ScriptHost>,
) -> Result<(), Failure> {
    register_natives(ctx, host).map_err(|e| Failure::message(e.to_string()))?;
    ctx.globals()
        .set("__stateJson", state_json)
        .map_err(|e| Failure::message(e.to_string()))?;
    ctx.eval::<(), _>(DAYJS)
        .catch(ctx)
        .map_err(|e| Failure::message(format!("dayjs failed to load: {e}")))?;
    ctx.eval::<(), _>(PRELUDE)
        .catch(ctx)
        .map_err(|e| Failure::message(format!("prelude failed: {e}")))?;

    // `promise: true` allows top-level `await`, as in Bruno scripts.
    let mut options = EvalOptions::default();
    options.promise = true;
    options.filename = Some(SCRIPT_FILE.to_string());
    let promise = ctx
        .eval_with_options::<rquickjs::Promise, _>(source, options)
        .catch(ctx)
        .map_err(failure_from)?;
    promise
        .into_future::<rquickjs::Value>()
        .await
        .catch(ctx)
        .map_err(failure_from)?;

    // Async `test(...)` callbacks may still be running.
    ctx.eval_promise("await __frida.finish()")
        .catch(ctx)
        .map_err(failure_from)?
        .into_future::<rquickjs::Value>()
        .await
        .catch(ctx)
        .map_err(failure_from)?;
    Ok(())
}

/// Turns a caught JS error into a message and the line it happened on.
fn failure_from(error: rquickjs::CaughtError<'_>) -> Failure {
    match error {
        rquickjs::CaughtError::Exception(exception) => Failure {
            message: exception
                .message()
                .unwrap_or_else(|| "unknown error".to_string()),
            line: exception.stack().as_deref().and_then(line_of),
        },
        rquickjs::CaughtError::Value(value) => Failure::message(
            value
                .as_string()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_else(|| format!("{value:?}")),
        ),
        rquickjs::CaughtError::Error(error) => Failure::message(error.to_string()),
    }
}

/// The line from the first `script:LINE:COL` frame of a stack trace.
fn line_of(stack: &str) -> Option<u32> {
    let pattern = format!("{SCRIPT_FILE}:");
    let start = stack.find(&pattern)? + pattern.len();
    let digits: String = stack[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The `__native` object: the only functions that reach outside the sandbox.
fn register_natives<'js>(ctx: &Ctx<'js>, host: Arc<dyn ScriptHost>) -> rquickjs::Result<()> {
    let natives = Object::new(ctx.clone())?;

    // Async functions return a JS promise; the Rust future runs while the script awaits it.
    let send_host = Arc::clone(&host);
    natives.set(
        "send",
        Function::new(
            ctx.clone(),
            Async(move |call_json: String| {
                let host = Arc::clone(&send_host);
                async move {
                    let reply = match serde_json::from_str(&call_json) {
                        Ok(call) => host.send_request(call).await,
                        Err(err) => Err(format!("invalid request options: {err}")),
                    };
                    let json = match reply {
                        Ok(reply) => serde_json::json!({ "ok": reply }),
                        Err(error) => serde_json::json!({ "error": error }),
                    };
                    rquickjs::Result::Ok(json.to_string())
                }
            }),
        )?,
    )?;
    natives.set(
        "sleep",
        Function::new(
            ctx.clone(),
            Async(|ms: f64| async move {
                let ms = if ms.is_finite() && ms > 0.0 {
                    ms as u64
                } else {
                    0
                };
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                rquickjs::Result::Ok(())
            }),
        )?,
    )?;
    natives.set(
        "interpolate",
        Function::new(
            ctx.clone(),
            move |text: String, vars_json: String| match serde_json::from_str(&vars_json) {
                Ok(vars) => host.interpolate(&text, &vars),
                Err(_) => text,
            },
        )?,
    )?;
    natives.set(
        "hash",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, algorithm: String, data: String| {
                native::hash(&algorithm, &data).map_err(|e| Exception::throw_message(&ctx, &e))
            },
        )?,
    )?;
    natives.set(
        "hmac",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>, algorithm: String, key: String, data: String| {
                native::hmac(&algorithm, &key, &data)
                    .map_err(|e| Exception::throw_message(&ctx, &e))
            },
        )?,
    )?;
    natives.set(
        "btoa",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, text: String| {
            native::btoa(&text).map_err(|e| Exception::throw_message(&ctx, &e))
        })?,
    )?;
    natives.set(
        "atob",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, text: String| {
            native::atob(&text).map_err(|e| Exception::throw_message(&ctx, &e))
        })?,
    )?;
    natives.set(
        "uuid",
        Function::new(ctx.clone(), || uuid::Uuid::new_v4().to_string())?,
    )?;
    natives.set(
        "processEnv",
        Function::new(ctx.clone(), |name: Opt<String>| {
            name.0.and_then(|n| std::env::var(n).ok())
        })?,
    )?;
    ctx.globals().set("__native", natives)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_line_in_a_stack_trace() {
        assert_eq!(line_of("    at <eval> (script:3:7)\n"), Some(3));
        assert_eq!(line_of("    at test (prelude:1)\n"), None);
    }
}
