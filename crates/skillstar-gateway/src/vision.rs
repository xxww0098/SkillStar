//! When a target cannot see, a vision model describes each Chat image first.
//!
//! The model id is the top-level `vision` field of
//! `config_dir()/model_gateway.json`. Empty, `off`, or a missing file means
//! off, and off does not ask anyone to describe an image. A description
//! request is not described again.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::sync::{Condvar, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

/// System prompt sent with every description. No trailing newline.
pub const VISION_SYSTEM: &str = "You describe images for an AI model that cannot see them. It will answer the user from your description alone, so leave nothing out that it may need.\n- Transcribe all text exactly as written, keeping its layout: code, terminal output, error messages, logs, UI labels, menus, file names, numbers.\n- For a screenshot of an app or page: which app or page it is, its layout, and the state of what is on it (selected, disabled, checked, highlighted, error states).\n- For a chart or table: its kind, axes and labels, and every value you can read.\n- For a diagram: its elements and how they are connected.\n- For a photo or drawing: what it shows, with the details that matter.\nDescribe only what is there. Don't guess at what can't be read, say it can't be read. Don't answer questions or give advice. No preamble.";

/// User-Agent on a description request.
pub const VISION_USER_AGENT: &str = "skillstar-vision/1";
/// How long one description may take.
pub const VISION_TIMEOUT: Duration = Duration::from_secs(2 * 60);
/// How many images of one request are described at once.
pub const VISION_PARALLEL: usize = 4;
/// How many successful descriptions are kept.
pub const VISION_CACHE: usize = 256;

const DESCRIBE: &str = "Describe this image.";
const MAX_TOKENS: i64 = 4096;
const OMITTED: &str = "[Image omitted: this model accepts text only.]";
const OMITTED_FAILED: &str =
    "[Image omitted: this model accepts text only, and the image couldn't be described.]";
const FILE_ONLY: &str = "the image is a file only its vendor can open";

/// What the caller posts to the vision model.
#[derive(Debug)]
pub struct VisionCall {
    pub user_agent: &'static str,
    pub timeout: Duration,
    pub body: Vec<u8>,
}

/// What the vision model sent back.
#[derive(Debug)]
pub enum VisionReply {
    /// A body the model returned.
    Bytes(Vec<u8>),
    /// The call failed before an answer.
    Failed,
    /// The call did not answer within [`VISION_TIMEOUT`].
    TimedOut,
}

/// The target cannot take this image request.
#[derive(Debug)]
pub struct VisionReject {
    pub status: u16,
    pub message: String,
}

struct Miss {
    why: String,
}

struct ImageAt {
    message: usize,
    block: usize,
    src: String,
    current: bool,
}

struct Job {
    doc: Value,
    target: String,
    images: Vec<ImageAt>,
    srcs: Vec<String>,
}

struct Memory {
    order: VecDeque<u64>,
    text: HashMap<u64, String>,
}

static MEMORY: LazyLock<Mutex<Memory>> = LazyLock::new(|| {
    Mutex::new(Memory {
        order: VecDeque::new(),
        text: HashMap::new(),
    })
});

/// Replace Chat images when `sees` is false.
///
/// `describing` is the caller's mark that this body is already a description.
/// Off leaves a seeing target unchanged and rejects a current image for a
/// target that cannot see, without calling `ask`.
pub fn apply_vision(
    body: &[u8],
    sees: bool,
    describing: bool,
    ask: &(impl Fn(&VisionCall) -> VisionReply + Sync),
) -> Result<Vec<u8>, VisionReject> {
    if sees || describing || marked(body) {
        return Ok(body.to_vec());
    }
    let Some(model) = stored_vision() else {
        return off_body(body);
    };
    let Some(job) = plan(body) else {
        return Ok(body.to_vec());
    };
    let described = describe_many(&model, &job.srcs, ask);
    finish(body, job, &model, &described)
}

/// Serve path. Off, or a target that is the vision model, keeps the bytes.
pub(crate) async fn rewrite_forward(body: &[u8], base: &str) -> Result<Vec<u8>, VisionReject> {
    let Some(model) = stored_vision() else {
        return Ok(body.to_vec());
    };
    if marked(body) || names_model(body, &model) {
        return Ok(body.to_vec());
    }
    let Some(job) = plan(body) else {
        return Ok(body.to_vec());
    };
    let described = describe_upstream(&model, &job.srcs, base).await;
    finish(body, job, &model, &described)
}

fn off_body(body: &[u8]) -> Result<Vec<u8>, VisionReject> {
    let Some(mut job) = plan(body) else {
        return Ok(body.to_vec());
    };
    if job.images.iter().any(|image| image.current) {
        return Err(VisionReject {
            status: 400,
            message: format!("model \"{}\" does not support image input", job.target),
        });
    }
    let images = std::mem::take(&mut job.images);
    for image in &images {
        put_text(&mut job.doc, image, OMITTED);
    }
    Ok(serialize(body, &job.doc))
}

fn finish(
    body: &[u8],
    mut job: Job,
    vision_model: &str,
    described: &HashMap<String, Result<String, Miss>>,
) -> Result<Vec<u8>, VisionReject> {
    let images = std::mem::take(&mut job.images);
    for image in &images {
        let text = replacement(image, &job.target, vision_model, described)?;
        put_text(&mut job.doc, image, &text);
    }
    Ok(serialize(body, &job.doc))
}

fn replacement(
    image: &ImageAt,
    target: &str,
    vision_model: &str,
    described: &HashMap<String, Result<String, Miss>>,
) -> Result<String, VisionReject> {
    if image.src.is_empty() && image.current {
        return Err(cannot_describe(target, vision_model, FILE_ONLY));
    }
    if image.src.is_empty() {
        return Ok(OMITTED.to_string());
    }
    match described.get(&image.src) {
        Some(Ok(text)) => Ok(described_block(vision_model, text)),
        Some(Err(miss)) if image.current => Err(cannot_describe(target, vision_model, &miss.why)),
        Some(Err(_)) => Ok(OMITTED_FAILED.to_string()),
        None if image.current => Err(cannot_describe(target, vision_model, "no description")),
        None => Ok(OMITTED_FAILED.to_string()),
    }
}

fn cannot_describe(target: &str, vision_model: &str, why: &str) -> VisionReject {
    VisionReject {
        status: 502,
        message: format!(
            "model \"{target}\" can't see images, and {vision_model} couldn't describe the image for it: {why}"
        ),
    }
}

fn described_block(model: &str, text: &str) -> String {
    format!(
        "\n[Image, as {model} describes it for this model, which can't see images:]\n{text}\n[End of the image's description]\n"
    )
}

fn describe_many(
    model: &str,
    srcs: &[String],
    ask: &(impl Fn(&VisionCall) -> VisionReply + Sync),
) -> HashMap<String, Result<String, Miss>> {
    let (mut out, missing) = take_cached(model, srcs);
    let fetched = map_parallel(&missing, |src| {
        let call = vision_call(model, src);
        reply_text(&ask(&call))
    });
    store_fetched(model, missing, fetched, &mut out);
    out
}

async fn describe_upstream(
    model: &str,
    srcs: &[String],
    base: &str,
) -> HashMap<String, Result<String, Miss>> {
    let (mut out, missing) = take_cached(model, srcs);
    if missing.is_empty() {
        return out;
    }
    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(VISION_PARALLEL));
    let mut joins = Vec::with_capacity(missing.len());
    for src in &missing {
        let Ok(permit) = permits.clone().acquire_owned().await else {
            out.insert(src.clone(), Err(miss("no description")));
            continue;
        };
        let model = model.to_string();
        let src = src.clone();
        let base = base.to_string();
        joins.push(tokio::spawn(async move {
            let _permit = permit;
            let call = vision_call(&model, &src);
            let reply = post_vision(&base, &call).await;
            (src, reply_text(&reply))
        }));
    }
    let mut fetched = Vec::with_capacity(joins.len());
    let mut srcs_done = Vec::with_capacity(joins.len());
    for join in joins {
        if let Ok((src, result)) = join.await {
            srcs_done.push(src);
            fetched.push(result);
        }
    }
    store_fetched(model, srcs_done, fetched, &mut out);
    out
}

async fn post_vision(base: &str, call: &VisionCall) -> VisionReply {
    let client = match skillstar_core::infra::http_client::probe_http_client(call.timeout) {
        Ok(client) => client,
        Err(_) => return VisionReply::Failed,
    };
    let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));
    let pending = client
        .post(url)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .header(hyper::header::USER_AGENT, call.user_agent)
        .timeout(call.timeout)
        .body(call.body.clone());
    let response = match tokio::time::timeout(call.timeout, pending.send()).await {
        Err(_) => return VisionReply::TimedOut,
        Ok(Err(_)) => return VisionReply::Failed,
        Ok(Ok(response)) => response,
    };
    if response.status().as_u16() >= 300 {
        return VisionReply::Failed;
    }
    match response.bytes().await {
        Ok(bytes) => VisionReply::Bytes(bytes.to_vec()),
        Err(_) => VisionReply::Failed,
    }
}

fn take_cached(
    model: &str,
    srcs: &[String],
) -> (HashMap<String, Result<String, Miss>>, Vec<String>) {
    let mut out = HashMap::new();
    let mut missing = Vec::new();
    for src in srcs {
        if let Some(text) = cache_get(model, src) {
            out.insert(src.clone(), Ok(text));
        } else {
            missing.push(src.clone());
        }
    }
    (out, missing)
}

fn store_fetched(
    model: &str,
    srcs: Vec<String>,
    fetched: Vec<Result<String, Miss>>,
    out: &mut HashMap<String, Result<String, Miss>>,
) {
    for (src, result) in srcs.into_iter().zip(fetched) {
        if let Ok(text) = &result {
            cache_put(model, &src, text);
        }
        out.insert(src, result);
    }
}

fn vision_call(model: &str, src: &str) -> VisionCall {
    let body = json!({
        "model": model,
        "stream": false,
        "messages": [
            {"role": "system", "content": VISION_SYSTEM},
            {"role": "user", "content": [
                {"type": "text", "text": DESCRIBE},
                {"type": "image_url", "image_url": {"url": src}}
            ]}
        ],
        "max_tokens": MAX_TOKENS
    });
    VisionCall {
        user_agent: VISION_USER_AGENT,
        timeout: VISION_TIMEOUT,
        body: serde_json::to_vec(&body).unwrap_or_default(),
    }
}

fn reply_text(reply: &VisionReply) -> Result<String, Miss> {
    match reply {
        VisionReply::TimedOut => Err(miss(&format!("no answer in {}s", VISION_TIMEOUT.as_secs()))),
        VisionReply::Failed => Err(miss("no description")),
        VisionReply::Bytes(bytes) => parse_description(bytes),
    }
}

fn parse_description(bytes: &[u8]) -> Result<String, Miss> {
    let doc: Value = match serde_json::from_slice(bytes) {
        Ok(doc) => doc,
        Err(_) => return Err(miss("not an answer")),
    };
    if doc.get("error").is_some() {
        let why = doc
            .pointer("/error/message")
            .and_then(Value::as_str)
            .filter(|message| !message.is_empty())
            .unwrap_or("no description");
        return Err(miss(why));
    }
    let text = doc
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        Err(miss("no description"))
    } else {
        Ok(text.to_string())
    }
}

fn miss(why: &str) -> Miss {
    Miss {
        why: why.to_string(),
    }
}

fn plan(body: &[u8]) -> Option<Job> {
    let doc: Value = serde_json::from_slice(body).ok()?;
    if !doc.is_object() {
        return None;
    }
    let images = find_images(&doc);
    if images.is_empty() {
        return None;
    }
    let mut seen = HashSet::new();
    let mut srcs = Vec::new();
    for image in &images {
        if !image.src.is_empty() && seen.insert(image.src.clone()) {
            srcs.push(image.src.clone());
        }
    }
    let target = doc
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    Some(Job {
        doc,
        target,
        images,
        srcs,
    })
}

fn find_images(doc: &Value) -> Vec<ImageAt> {
    let Some(messages) = doc.get("messages").and_then(Value::as_array) else {
        return Vec::new();
    };
    let last = messages.len().saturating_sub(1);
    let mut found = Vec::new();
    for (message_at, message) in messages.iter().enumerate() {
        let tool = message.get("role").and_then(Value::as_str) == Some("tool");
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for (block_at, block) in blocks.iter().enumerate() {
            let Some(src) = image_src(block) else {
                continue;
            };
            found.push(ImageAt {
                message: message_at,
                block: block_at,
                src,
                current: message_at == last && !tool,
            });
        }
    }
    found
}

fn image_src(block: &Value) -> Option<String> {
    if block.get("type").and_then(Value::as_str) != Some("image_url") {
        return None;
    }
    match block.get("image_url") {
        Some(Value::String(url)) => Some(url.clone()),
        Some(Value::Object(obj)) => Some(
            obj.get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        ),
        _ => Some(String::new()),
    }
}

fn put_text(doc: &mut Value, image: &ImageAt, text: &str) {
    let Some(block) = doc
        .get_mut("messages")
        .and_then(|messages| messages.get_mut(image.message))
        .and_then(|message| message.get_mut("content"))
        .and_then(|content| content.get_mut(image.block))
    else {
        return;
    };
    *block = json!({"type": "text", "text": text});
}

fn serialize(original: &[u8], doc: &Value) -> Vec<u8> {
    serde_json::to_vec(doc).unwrap_or_else(|_| original.to_vec())
}

fn marked(body: &[u8]) -> bool {
    let Ok(doc) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    let Some(messages) = doc.get("messages").and_then(Value::as_array) else {
        return false;
    };
    messages.iter().any(|message| {
        message.get("role").and_then(Value::as_str) == Some("system")
            && content_is_prompt(message.get("content"))
    })
}

fn content_is_prompt(content: Option<&Value>) -> bool {
    match content {
        Some(Value::String(text)) => text == VISION_SYSTEM,
        Some(Value::Array(parts)) => parts.iter().any(|part| {
            part.get("text").and_then(Value::as_str) == Some(VISION_SYSTEM)
                || part.get("content").and_then(Value::as_str) == Some(VISION_SYSTEM)
        }),
        _ => false,
    }
}

fn names_model(body: &[u8], model: &str) -> bool {
    let Ok(doc) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    doc.get("model")
        .and_then(Value::as_str)
        .is_some_and(|id| id.trim() == model)
}

fn stored_vision() -> Option<String> {
    let bytes = fs::read(gateway_path()).ok()?;
    let doc: Value = serde_json::from_slice(&bytes).ok()?;
    let raw = doc.get("vision").and_then(Value::as_str)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "off" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn gateway_path() -> std::path::PathBuf {
    skillstar_core::infra::paths::config_dir().join("model_gateway.json")
}

fn cache_key(model: &str, src: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model.hash(&mut hasher);
    src.hash(&mut hasher);
    hasher.finish()
}

fn cache_get(model: &str, src: &str) -> Option<String> {
    let key = cache_key(model, src);
    MEMORY
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .text
        .get(&key)
        .cloned()
}

fn cache_put(model: &str, src: &str, text: &str) {
    let key = cache_key(model, src);
    let mut memory = MEMORY.lock().unwrap_or_else(|poison| poison.into_inner());
    if memory.text.contains_key(&key) {
        return;
    }
    memory.order.push_back(key);
    memory.text.insert(key, text.to_string());
    while memory.text.len() > VISION_CACHE {
        let Some(old) = memory.order.pop_front() else {
            break;
        };
        memory.text.remove(&old);
    }
}

fn map_parallel<T, R>(items: &[T], work: impl Fn(&T) -> R + Sync + Send) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    if items.is_empty() {
        return Vec::new();
    }
    let gate = Gate::new(VISION_PARALLEL);
    let slots = Mutex::new((0..items.len()).map(|_| None).collect::<Vec<Option<R>>>());
    thread::scope(|scope| {
        for (index, item) in items.iter().enumerate() {
            let gate = &gate;
            let slots = &slots;
            let work = &work;
            scope.spawn(move || {
                let _permit = gate.enter();
                let value = work(item);
                slots.lock().unwrap_or_else(|poison| poison.into_inner())[index] = Some(value);
            });
        }
    });
    slots
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner())
        .into_iter()
        .map(|slot| slot.expect("vision job finished"))
        .collect()
}

struct Gate {
    left: Mutex<usize>,
    cv: Condvar,
}

struct Permit<'a> {
    gate: &'a Gate,
}

impl Gate {
    fn new(slots: usize) -> Self {
        Self {
            left: Mutex::new(slots),
            cv: Condvar::new(),
        }
    }

    fn enter(&self) -> Permit<'_> {
        let mut left = self
            .left
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while *left == 0 {
            left = self
                .cv
                .wait(left)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        *left -= 1;
        Permit { gate: self }
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut left = self
            .gate
            .left
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *left += 1;
        self.gate.cv.notify_one();
    }
}
