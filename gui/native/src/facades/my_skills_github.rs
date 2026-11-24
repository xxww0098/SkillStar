//! GitHub sign-in slice facade (wave 4) — the `my_skills/github_sign_in.rs`
//! seam: the device-authorization dialog (start device flow, poll until
//! Connected/Denied/Expired, cancel on close). The slice worker owns this
//! file and adds `MySkillsGitHub` methods only — registration is already
//! in `lib.rs`.
//!
//! Bridge contract the slice must follow (primitives in `bridge_task.rs`):
//!
//! * **One shared facade per process**, exactly like the GPUI shell's
//!   `OnceLock` (github_sign_in.rs:30-41: one `GitHubAuthFacade` so
//!   `cancel_device_flow` always hits the same runtime the poll uses).
//!   `MySkills::github_status` already builds one per call for the gate —
//!   fine for reads; the *sign-in flow* must use this module's shared one
//!   or cancel cannot reach it.
//! * **The poll is a loop, not a chain**: the GPUI dialog re-arms
//!   `schedule_poll` on every `Pending`/`SlowDown` with the domain's
//!   `retry_after_seconds` (github_sign_in.rs:141-179 — SlowDown is the
//!   same arm, the backoff seconds come from the domain). Here that is one
//!   `#[module]` async fn: `timer(Duration::from_secs(interval)).await`,
//!   `poll_device_flow().await`, match `DeviceFlowPoll` — `Pending |
//!   SlowDown => continue with retry_after_seconds`, `Connected` resolves
//!   with the connection, `Denied`/`Expired`/`Err` rejects with the
//!   domain message the dialog shows (the i18n keys are the TSX side's).
//!   `timer` (not `sleep`) keeps the task on the engine thread so the
//!   callback posts stay legal.
//! * **Cancel**: the loop checks `JobFlag::cancelled` before every poll
//!   and calls `facade.cancel_device_flow()` once (github_sign_in.rs
//!   :110-113's `cancel`), then resolves with `{cancelled: true}` — the
//!   next poll's `NoPendingAuthorization` never needs to run. TSX cancels
//!   on dialog close (X / Escape / backdrop all funnel to
//!   `DomainJob.cancel()`/`dispose()`), and phase UI (user code /
//!   verification uri / remaining seconds) rides the progress stream: post
//!   `{phase: "waiting", userCode, verificationUri, remainingSecs}` once
//!   after `start_device_flow` resolves.
//! * **No blocking pool**: every call is an async HTTP round trip; there
//!   is nothing synchronous to wrap (the device-flow API surface is
//!   `start_device_flow` / `poll_device_flow` / `cancel_device_flow` /
//!   `status`, all async, ss-skills `github_auth`).
//!
//! Two contract details this implementation settles, both reported for
//! the ledger:
//!
//! * **Denied/Expired reject with the i18n KEY, not an English sentence.**
//!   The GPUI dialog maps them through `t("settings.githubAuthDenied")` /
//!   `t("settings.githubAuthExpired")` (github_sign_in.rs:168-175); the
//!   domain carries no message for either. The rejected `String` is
//!   therefore the locale key, and the TSX side funnels every rejection
//!   through `t(err)` — `t` falls back to the raw string, so genuine
//!   domain errors (network, protocol…) display verbatim, exactly like
//!   the GPUI `error.to_string()` arm (github_sign_in.rs:176).
//! * **The waiting event carries RFC 3339 `expiresAt`** alongside the
//!   derived `remainingSecs` (computed against the domain's own
//!   `SystemClock`, like github_sign_in.rs:124-126). The TSX side shows
//!   the static snapshot the GPUI dialog shows (it never ticks the
//!   countdown either).
use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use nsh_core::JsonValue;
use nsh_core::prelude::*;

use ss_skills::github_auth::{
    Clock, CredentialStore, DeviceFlowPoll, FileCredentialStore, GitHubAuthFacade,
    GitHubConnectionStatus, GitHubGateway, ProductionGitHubGateway, SystemClock,
};

use nsh_core::element::__private::JsonMap;

use crate::facades::bridge_task::{DomainJob, JobFlag, job_flag};

#[path = "my_skills_github_publish.rs"]
mod publish;

fn obj(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
    let mut map = JsonMap::<String, JsonValue>::new();
    for (key, value) in pairs {
        map.insert(key.to_owned(), value);
    }
    JsonValue::Object(map)
}

fn str(value: &str) -> JsonValue {
    JsonValue::String(value.to_owned())
}

/// The whole-process sign-in facade — `shared_auth_facade` parity
/// (github_sign_in.rs:30-41). Reads (`MySkills::github_status`) build
/// their own per call; the sign-in flow shares THIS one so a close-click
/// cancel always reaches the runtime its poll loop uses.
fn shared_facade(
) -> &'static GitHubAuthFacade<ProductionGitHubGateway, FileCredentialStore, SystemClock> {
    static FACADE: OnceLock<
        GitHubAuthFacade<ProductionGitHubGateway, FileCredentialStore, SystemClock>,
    > = OnceLock::new();
    FACADE.get_or_init(|| {
        GitHubAuthFacade::new(
            ProductionGitHubGateway::from_environment(),
            FileCredentialStore::default(),
            SystemClock,
        )
    })
}

/// The device-authorization seam (`MySkillsGitHub` in TSX): start the
/// device flow, poll Pending/SlowDown with the domain's backoff, resolve
/// Connected, cancel on close.
#[module]
pub struct MySkillsGitHub;

#[module]
impl MySkillsGitHub {
    /// One sign-in attempt as one domain job: `start_device_flow`, post
    /// the waiting phase once, then poll — sleeping `retry_after_seconds`
    /// between attempts (Pending and SlowDown are one arm; the seconds
    /// come from the domain, github_sign_in.rs:141-179). `Connected`
    /// resolves `{connected: true, state: "connected", login}`;
    /// `Denied`/`Expired` reject with their TSX-side locale keys; any
    /// domain error rejects with its message (the dialog shows it
    /// verbatim). A cancelled job (`DomainJob.cancel`/Drop) calls
    /// `cancel_device_flow()` once and resolves `{cancelled: true}` —
    /// the GPUI dialog's `Drop` counterpart (github_sign_in.rs:416-420).
    pub async fn sign_in(
        job: Instance<DomainJob>,
        #[args(event)] on_event: Callback<JsonValue>,
    ) -> Result<JsonValue, String> {
        let flag = job_flag(&job);
        run_device_flow(
            shared_facade(),
            &flag,
            &|event| on_event.post(event),
            &|secs| timer(Duration::from_secs(secs)),
        )
        .await
    }

    /// Clear the stored SkillStar GitHub credential and any in-flight device
    /// flow session (`GitHubAuthFacade::logout`). Runs on the blocking pool.
    pub async fn logout() -> Result<JsonValue, String> {
        blocking(|| {
            shared_facade().logout().map_err(|error| error.message)?;
            ss_app::channel_ops::clear_sessions();
            Ok(connection_json(&GitHubConnectionStatus::SignedOut))
        })
        .await
        .map_err(|join| format!("logout task failed: {join}"))?
    }

    /// Refresh the stored GitHub App access token via `GitHubAuthFacade::refresh`
    /// and return `{ connected, state, login }`.
    pub async fn refresh() -> Result<JsonValue, String> {
        let status = shared_facade()
            .refresh()
            .await
            .map_err(|error| error.message)?;
        Ok(connection_json(&status))
    }

    /// Preflight local-skill publication readiness: git binary check,
    /// SkillStar GitHub App authentication status (`GhStatus`), local skill
    /// metadata summary, and suggested repository name. Runs on the blocking
    /// pool because `gh_manager::check_status` drives synchronous HTTP inside
    /// a short-lived runtime.
    pub async fn publish_preflight(skill_name: String) -> Result<JsonValue, String> {
        blocking(move || publish::publish_preflight_sync(&skill_name))
            .await
            .map_err(|join| format!("publish preflight task failed: {join}"))?
    }

    /// Publish a local skill to a GitHub repository via `gh_manager::publish_skill`
    /// without mutating `.skill-lock.json` provenance (D-081).
    pub async fn publish_skill(
        skill_name: String,
        repo_name: String,
        is_private: bool,
        description: String,
    ) -> Result<JsonValue, String> {
        blocking(move || publish::publish_skill_sync(&skill_name, &repo_name, is_private, &description))
            .await
            .map_err(|join| format!("publish task failed: {join}"))?
    }
}

/// The poll loop, generic over its seams so the crate tests drive it with
/// a scripted transport and zero-delay sleeps (the backoff semantics are
/// asserted as the *delay call sequence*, not wall-clock seconds). This
/// is the engine-thread half of `schedule_poll`'s chain: one loop instead
/// of re-armed spawns, same domain cadence.
async fn run_device_flow<G, S, C, Event, Delay, DelayFut>(
    facade: &GitHubAuthFacade<G, S, C>,
    flag: &JobFlag,
    on_event: &Event,
    delay: &Delay,
) -> Result<JsonValue, String>
where
    G: GitHubGateway,
    S: CredentialStore,
    C: Clock,
    Event: Fn(JsonValue),
    Delay: Fn(u64) -> DelayFut,
    DelayFut: Future<Output = ()>,
{
    let authorization = match facade.start_device_flow().await {
        Ok(authorization) => authorization,
        Err(error) => return Err(error.message),
    };
    if flag.cancelled() {
        // Closed while the start request was in flight (github_sign_in.rs
        // :110-113: every close path funnels into cancel).
        let _ = facade.cancel_device_flow();
        return Ok(obj(vec![("cancelled", JsonValue::Bool(true))]));
    }
    // `Utc::now()` in the GPUI dialog, spelled through the domain's own
    // clock type (this crate has no direct chrono dependency). NB: the
    // GPUI line's `.clamp(0, u64::MAX as i64)` panics — `u64::MAX as i64`
    // is -1, and clamp(0, -1) is min>max; `max(0)` keeps the intent (a
    // past expiry reads 0) without the trap.
    let now = SystemClock.now();
    let remaining_secs = (authorization.expires_at - now).num_seconds().max(0) as u64;
    on_event(obj(vec![
        ("phase", str("waiting")),
        ("userCode", str(&authorization.user_code)),
        ("verificationUri", str(&authorization.verification_uri)),
        ("remainingSecs", JsonValue::from(remaining_secs)),
        ("expiresAt", str(&authorization.expires_at.to_rfc3339())),
    ]));
    // The first poll waits the grant's interval (github_sign_in.rs:127).
    let mut interval = authorization.interval_seconds.max(1);
    loop {
        delay(interval).await;
        // Before every poll: a cancelled job unwinds without touching the
        // gateway again (the GPUI Drop → cancel → next poll's
        // NoPendingAuthorization, short-circuited).
        if flag.cancelled() {
            let _ = facade.cancel_device_flow();
            return Ok(obj(vec![("cancelled", JsonValue::Bool(true))]));
        }
        match facade.poll_device_flow().await {
            // One arm for both: the domain already folded SlowDown's +5s
            // into retry_after_seconds (github_auth/mod.rs:336-350).
            Ok(DeviceFlowPoll::Pending {
                retry_after_seconds,
            })
            | Ok(DeviceFlowPoll::SlowDown {
                retry_after_seconds,
            }) => {
                interval = retry_after_seconds.max(1);
            }
            Ok(DeviceFlowPoll::Connected { connection }) => {
                return Ok(connection_json(&connection));
            }
            Ok(DeviceFlowPoll::Denied) => return Err("settings.githubAuthDenied".to_owned()),
            Ok(DeviceFlowPoll::Expired) => return Err("settings.githubAuthExpired".to_owned()),
            Err(error) => return Err(error.message),
        }
    }
}

/// What the page keeps after a successful sign-in: the connection state
/// and login the channels-scope gate reads (`page.github = Some(connection)`
/// + `revise`, github_sign_in.rs:160-166).
fn connection_json(connection: &GitHubConnectionStatus) -> JsonValue {
    match connection {
        GitHubConnectionStatus::Connected { identity, .. } => obj(vec![
            ("connected", JsonValue::Bool(true)),
            ("state", str("connected")),
            ("login", str(&identity.login)),
        ]),
        GitHubConnectionStatus::Expired { identity } => obj(vec![
            ("connected", JsonValue::Bool(false)),
            ("state", str("expired")),
            (
                "login",
                match identity {
                    Some(item) => str(&item.login),
                    None => JsonValue::Null,
                },
            ),
        ]),
        GitHubConnectionStatus::SignedOut => obj(vec![
            ("connected", JsonValue::Bool(false)),
            ("state", str("signed_out")),
            ("login", JsonValue::Null),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facades::TEST_ENV_LOCK;
    use ss_skills::github_auth::{
        GitHubAuthError, GitHubAuthErrorCode, GitHubHttpGateway, GitHubHttpResponse,
        GitHubHttpTransport,
    };
    use std::sync::{Arc, Mutex};

    /// The shared sandbox guard (facades/mod.rs TEST_ENV_LOCK): the
    /// Connected path writes credentials through `FileCredentialStore`,
    /// which resolves under SKILLSTAR_DATA_DIR — every test here borrows
    /// the lock for its whole sandbox lifetime (wave-1 lesson; module-local
    /// locks cannot serialize cargo's parallel threads).
    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = TEST_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-my-skills-github-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
            Self { _lock: lock, previous, root }
        }
    }

    impl Drop for IsolatedDataDir {
        fn drop(&mut self) {
            unsafe {
                match self.previous.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    // ---- The scripted transport ----------------------------------------------
    //
    // The crate cannot depend on `async-trait` (its manifest is frozen for
    // the parallel waves), so the `#[async_trait]` expansion is written out
    // by hand: the trait's per-argument lifetimes collapse into one `'a`,
    // the async body becomes `Pin<Box<dyn Future + Send + 'a>>` (async-trait
    // 0.1.89's shape, expand.rs transform_sig). The fake answers GitHub's
    // three endpoints from a script queue and never opens a socket — the
    // "no real network" honesty rule.

    type Reply = Result<(u16, String), GitHubAuthError>;

    struct ScriptedTransport {
        /// One entry per HTTP request, in order (both endpoints share the
        /// queue: the device flow is strictly sequential).
        script: Mutex<Vec<Reply>>,
        /// Every request seen, shared with the fixture's copy: the gateway
        /// holds a clone, both log into the same Arc.
        requests: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>,
    }

    /// The gateway owns a clone; the script queue clones with it (the
    /// fixture's copy is never read again), the request log does not.
    impl Clone for ScriptedTransport {
        fn clone(&self) -> Self {
            Self {
                script: Mutex::new(self.script.lock().unwrap().clone()),
                requests: Arc::clone(&self.requests),
            }
        }
    }

    impl ScriptedTransport {
        fn new(script: Vec<Reply>) -> Self {
            Self {
                script: Mutex::new(script),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn record(&self, url: &str, fields: &[(String, String)]) {
            self.requests.lock().unwrap().push((
                url.to_owned(),
                fields.iter().cloned().collect::<Vec<_>>(),
            ));
        }

        fn next_reply(&self) -> Result<GitHubHttpResponse, GitHubAuthError> {
            let mut script = self.script.lock().unwrap();
            match script.pop() {
                Some(Ok((status, body))) => Ok(GitHubHttpResponse { status, body }),
                Some(Err(error)) => Err(error),
                None => Err(GitHubAuthError::new(
                    GitHubAuthErrorCode::Network,
                    "script exhausted",
                )),
            }
        }
    }

    impl GitHubHttpTransport for ScriptedTransport {
        fn post_form<'life0, 'life1, 'life2, 'async_trait>(
            &'life0 self,
            url: &'life1 str,
            fields: &'life2 [(String, String)],
        ) -> std::pin::Pin<
            Box<
                dyn Future<Output = Result<GitHubHttpResponse, GitHubAuthError>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            'life2: 'async_trait,
            Self: 'async_trait,
        {
            let url = url.to_owned();
            let fields = fields.to_vec();
            Box::pin(async move {
                self.record(&url, &fields);
                self.next_reply()
            })
        }

        fn get_bearer<'life0, 'life1, 'life2, 'async_trait>(
            &'life0 self,
            url: &'life1 str,
            _bearer: &'life2 str,
        ) -> std::pin::Pin<
            Box<
                dyn Future<Output = Result<GitHubHttpResponse, GitHubAuthError>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            'life2: 'async_trait,
            Self: 'async_trait,
        {
            let url = url.to_owned();
            Box::pin(async move {
                self.record(&url, &[]);
                self.next_reply()
            })
        }
    }

    fn ok(body: &str) -> Reply {
        Ok((200, body.to_owned()))
    }

    fn device_code(user_code: &str, interval: u64) -> Reply {
        ok(&format!(
            "{{\"device_code\":\"dev-1\",\"user_code\":\"{user_code}\",\
             \"verification_uri\":\"https://github.com/device\",\
             \"expires_in\":900,\"interval\":{interval}}}"
        ))
    }

    fn pending() -> Reply {
        ok("{\"error\":\"authorization_pending\"}")
    }

    fn slow_down() -> Reply {
        ok("{\"error\":\"slow_down\"}")
    }

    fn denied() -> Reply {
        ok("{\"error\":\"access_denied\"}")
    }

    fn expired() -> Reply {
        ok("{\"error\":\"expired_token\"}")
    }

    fn authorized() -> Reply {
        ok("{\"access_token\":\"gho_token\",\"refresh_token\":\"ghr_refresh\",\"expires_in\":28800}")
    }

    fn current_user() -> Reply {
        ok("{\"id\":1,\"login\":\"octocat\",\"avatar_url\":null}")
    }

    /// One isolated sign-in fixture: scripted gateway + the real credential
    /// store under a sandbox dir. Events, delay calls and HTTP requests are
    /// collected so the assertions read them as sequences.
    struct Fixture {
        _iso: IsolatedDataDir,
        facade:
            GitHubAuthFacade<GitHubHttpGateway<ScriptedTransport>, FileCredentialStore, SystemClock>,
        requests: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>,
        events: Mutex<Vec<JsonValue>>,
        delays: Mutex<Vec<u64>>,
    }

    impl Fixture {
        fn build(script: Vec<Reply>) -> Self {
            let iso = IsolatedDataDir::new();
            let transport = ScriptedTransport::new(script);
            let requests = Arc::clone(&transport.requests);
            let facade = GitHubAuthFacade::new(
                GitHubHttpGateway::with_transport("client-id-test", transport),
                FileCredentialStore::default(),
                SystemClock,
            );
            Self {
                _iso: iso,
                facade,
                requests,
                events: Mutex::new(Vec::new()),
                delays: Mutex::new(Vec::new()),
            }
        }

        fn run(&self, flag: &JobFlag) -> Result<JsonValue, String> {
            let events = &self.events;
            let delays = &self.delays;
            spin(run_device_flow(
                &self.facade,
                flag,
                &|event| {
                    events.lock().unwrap().push(event);
                },
                &|secs| async move {
                    delays.lock().unwrap().push(secs);
                },
            ))
        }

        fn events(&self) -> Vec<JsonValue> {
            self.events.lock().unwrap().clone()
        }

        fn delays(&self) -> Vec<u64> {
            self.delays.lock().unwrap().clone()
        }

        fn requests(&self) -> Vec<(String, Vec<(String, String)>)> {
            self.requests.lock().unwrap().clone()
        }
    }

    /// A job kept alive for the whole test (an `Instance<DomainJob>` that
    /// drops flips its flag — Drop cancels), plus its cloned flag.
    struct LiveJob {
        job: Instance<DomainJob>,
        flag: JobFlag,
    }

    fn live_job(label: &str) -> LiveJob {
        // `DomainJob::new` builds the value; the handle the bridge (and
        // `job_flag`) works with is the Instance wrapper.
        let job = Instance::new(DomainJob::new(label.to_owned()));
        let flag = job_flag(&job);
        LiveJob { job, flag }
    }

    /// Drive a future whose every layer settles on the first poll (the
    /// scripted transport and the test delays are synchronous); the loop
    /// only guards a future that yields once (my_skills.rs `spin` parity).
    fn spin<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        for _ in 0..1000 {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => continue,
            }
        }
        unreachable!("scripted flows settle within a thousand polls");
    }

    /// start → waiting event (user code, uri, remaining seconds, RFC 3339
    /// expiry) → Pending ×2 at the grant's interval → Connected with the
    /// login, over the real gateway + credential store in the sandbox.
    #[test]
    fn pending_polls_at_the_grant_interval_then_connects() {
        let fixture = Fixture::build(vec![
            current_user(),
            authorized(),
            pending(),
            pending(),
            device_code("ABCD-1234", 5),
        ]);
        let live = live_job("poll");
        let outcome = fixture.run(&live.flag).expect("connected");
        assert_eq!(outcome["connected"].as_bool(), Some(true));
        assert_eq!(outcome["login"].as_str(), Some("octocat"));

        let events = fixture.events();
        assert_eq!(events.len(), 1, "exactly one waiting event");
        let event = &events[0];
        assert_eq!(event["phase"].as_str(), Some("waiting"));
        assert_eq!(event["userCode"].as_str(), Some("ABCD-1234"));
        assert_eq!(
            event["verificationUri"].as_str(),
            Some("https://github.com/device")
        );
        let remaining = event["remainingSecs"].as_u64().expect("remaining secs");
        assert!(
            remaining > 890 && remaining <= 900,
            "expires_in=900, just started (got {remaining})"
        );
        assert!(
            event["expiresAt"]
                .as_str()
                .is_some_and(|value| value.ends_with("Z") || value.contains('+')),
            "RFC 3339 expiry for the TSX side"
        );

        // The delay sequence IS the backoff contract: first poll waits the
        // grant interval (5), both Pending replies keep it (github_sign_in.rs
        // :127 + :152-159).
        assert_eq!(fixture.delays(), vec![5, 5, 5]);
        // Five HTTP calls in order: device code → pending → pending →
        // token → current user.
        let requests = fixture.requests();
        let urls: Vec<&str> = requests.iter().map(|(url, _)| url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://github.com/login/device/code",
                "https://github.com/login/oauth/access_token",
                "https://github.com/login/oauth/access_token",
                "https://github.com/login/oauth/access_token",
                "https://api.github.com/user",
            ]
        );
    }

    /// SlowDown stretches the next delay to interval + 5 — the domain's
    /// own arithmetic (github_auth/mod.rs:342-350), surfaced as the delay
    /// the loop passes.
    #[test]
    fn slow_down_backoff_comes_from_the_domain() {
        let fixture = Fixture::build(vec![
            current_user(),
            authorized(),
            pending(),
            slow_down(),
            pending(),
            device_code("ABCD-1234", 5),
        ]);
        let live = live_job("backoff");
        let outcome = fixture.run(&live.flag).expect("resolves");
        assert_eq!(outcome["connected"].as_bool(), Some(true));
        // 5 (grant) → 5 (Pending keeps interval) → 10 (SlowDown = 5+5) →
        // 10 (the stretched interval persists for the next Pending).
        assert_eq!(fixture.delays(), vec![5, 5, 10, 10]);
    }

    /// Denied rejects with the TSX-side locale key; Expired likewise.
    #[test]
    fn denied_and_expired_reject_with_the_locale_keys() {
        // Two fixtures = two IsolatedDataDir sandboxes; the shared
        // TEST_ENV_LOCK is NOT reentrant, so each sandbox lives in its own
        // block and drops its guard before the next one arms (the wave-1
        // lesson, applied at fixture granularity).
        {
            let fixture = Fixture::build(vec![denied(), device_code("ABCD-1234", 5)]);
            let live = live_job("denied");
            let err = fixture.run(&live.flag).expect_err("denied");
            assert_eq!(err, "settings.githubAuthDenied");
            assert_eq!(fixture.delays(), vec![5]);
        }
        {
            let fixture = Fixture::build(vec![expired(), device_code("ABCD-1234", 5)]);
            let live = live_job("expired");
            let err = fixture.run(&live.flag).expect_err("expired");
            assert_eq!(err, "settings.githubAuthExpired");
        }
    }

    /// A gateway failure rejects with the domain message verbatim (the
    /// dialog's `error.to_string()` arm, github_sign_in.rs:176).
    #[test]
    fn a_start_failure_rejects_with_the_domain_message() {
        let fixture = Fixture::build(vec![Err(GitHubAuthError::new(
            GitHubAuthErrorCode::Unavailable,
            "This build does not include a GitHub App client ID",
        ))]);
        let live = live_job("start-fail");
        let err = fixture
            .run(&live.flag)
            .expect_err("start fails before any event");
        assert_eq!(err, "This build does not include a GitHub App client ID");
        assert!(
            fixture.events().is_empty(),
            "no waiting event on a failed start"
        );
    }

    /// Cancel mid-wait (the close click on a live waiting dialog): the
    /// loop stops at the flag check before the next poll,
    /// `cancel_device_flow` clears the runtime (a later poll on the same
    /// facade reports NoPendingAuthorization), and the job resolves
    /// `{cancelled: true}` with no further HTTP traffic. The cancel rides
    /// the waiting event itself — the user closes the dialog the moment
    /// the code shows up (Instance is not Send, but the event closure
    /// carries no Send bound; only the transport must be).
    #[test]
    fn cancelling_mid_wait_unwinds_without_another_poll() {
        let fixture = Fixture::build(vec![pending(), device_code("ABCD-1234", 5)]);
        let live = live_job("cancel-mid");
        let events = &fixture.events;
        let delays = &fixture.delays;
        let job = live.job.clone();
        let outcome = spin(run_device_flow(
            &fixture.facade,
            &live.flag,
            &|event| {
                events.lock().unwrap().push(event);
                // The close click: same `DomainJob` handle the TSX side
                // cancels through (Instance methods go via `with`).
                job.with(|job| job.cancel());
            },
            &|secs| async move {
                delays.lock().unwrap().push(secs);
            },
        ))
        .expect("cancelled resolves, not rejects");
        assert_eq!(outcome["cancelled"].as_bool(), Some(true));
        assert!(live.flag.cancelled(), "the shared flag sees the cancel");
        // The pending poll never ran: one HTTP call (device code) only.
        assert_eq!(fixture.requests().len(), 1);
        // The runtime was cleared: a fresh poll reports NoPendingAuthorization
        // (the domain's own generation gate, github_auth/mod.rs:375-379).
        let poll = spin(fixture.facade.poll_device_flow());
        assert!(poll.is_err(), "no device authorization is pending anymore");
    }

    /// The connection JSON the page keeps: connected + login (the
    /// channels-scope gate's `signedInAs` reads exactly this).
    #[test]
    fn connection_json_carries_state_and_login() {
        let identity = ss_skills::github_auth::GitHubIdentity {
            id: 7,
            login: "octocat".to_owned(),
            avatar_url: None,
        };
        let connected = ss_skills::github_auth::GitHubConnectionStatus::Connected {
            identity,
            access_expires_at: None,
        };
        let json = connection_json(&connected);
        assert_eq!(json["connected"].as_bool(), Some(true));
        assert_eq!(json["state"].as_str(), Some("connected"));
        assert_eq!(json["login"].as_str(), Some("octocat"));

        let expired = GitHubConnectionStatus::Expired {
            identity: Some(ss_skills::github_auth::GitHubIdentity {
                id: 7,
                login: "octocat".to_owned(),
                avatar_url: None,
            }),
        };
        let exp_json = connection_json(&expired);
        assert_eq!(exp_json["connected"].as_bool(), Some(false));
        assert_eq!(exp_json["state"].as_str(), Some("expired"));
        assert_eq!(exp_json["login"].as_str(), Some("octocat"));

        let out_json = connection_json(&GitHubConnectionStatus::SignedOut);
        assert_eq!(out_json["connected"].as_bool(), Some(false));
        assert_eq!(out_json["state"].as_str(), Some("signed_out"));
        assert!(out_json["login"].is_null());
    }

    #[test]
    fn refresh_and_logout_round_trip_through_auth_facade() {
        let fixture = Fixture::build(vec![
            current_user(),
            ok("{\"access_token\":\"ghu_new\",\"refresh_token\":\"ghr_new\",\"expires_in\":28800}"),
            current_user(),
            authorized(),
            device_code("ABCD-1234", 1),
        ]);
        let live = live_job("login");
        let signed_in = fixture.run(&live.flag).expect("device sign-in stores credential");
        assert_eq!(signed_in["state"].as_str(), Some("connected"));
        let refreshed = spin(fixture.facade.refresh()).expect("refresh succeeds");
        assert_eq!(connection_json(&refreshed)["login"].as_str(), Some("octocat"));
        fixture.facade.logout().expect("logout clears store");
        let after = spin(fixture.facade.status()).expect("status after logout");
        assert_eq!(connection_json(&after)["state"].as_str(), Some("signed_out"));
    }
}
