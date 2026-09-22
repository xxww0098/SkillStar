//! `skillstar decide` — run the local AgentJev decision model.
//!
//! Three things share one entry point because they share one surface: the
//! checkpoint directory. `--status` and `--verify` inspect it, `--download`
//! fills it in, and the default path loads it and answers a JSON payload. The
//! payload is the `agentjev.decision.v1` contract, so the same file works
//! against the hosted TypeSafe API or any other server that speaks it.

use std::io::Read;
use std::sync::atomic::AtomicBool;

use skillstar_decision::{
    download_with_shared_client, DecisionEngine, DeviceChoice, DTypeChoice, EngineOptions,
    ModelPaths, ModelState,
};

/// Options for `skillstar decide`.
pub struct DecideOpts<'a> {
    /// JSON payload file, or `-` for stdin.
    pub file: Option<&'a str>,
    /// Read the payload from stdin.
    pub stdin: bool,
    /// Download the checkpoint and exit.
    pub download: bool,
    /// Re-check every checkpoint digest and exit.
    pub verify: bool,
    /// Print the checkpoint state and exit.
    pub status: bool,
    /// Machine-readable output.
    pub json: bool,
    /// Device override.
    pub device: &'a str,
    /// Dtype override.
    pub dtype: &'a str,
}

/// Handle `skillstar decide`.
pub fn cmd_decide(opts: DecideOpts<'_>) {
    let paths = ModelPaths::resolve();

    if opts.status {
        return print_status(&paths, opts.json);
    }
    if opts.verify {
        return verify_checkpoint(&paths, opts.json);
    }
    if opts.download {
        return download_checkpoint(&paths);
    }

    let payload = match read_payload(&opts) {
        Ok(payload) => payload,
        Err(error) => fail(&error.to_string()),
    };
    let options = match engine_options(opts.device, opts.dtype) {
        Ok(options) => options,
        Err(error) => fail(&error.to_string()),
    };

    eprintln!("Loading {} ({}) …", paths.dir().display(), describe(options));
    let engine = match DecisionEngine::load(&paths, options) {
        Ok(engine) => engine,
        Err(error) => fail(&error.to_string()),
    };

    let started = std::time::Instant::now();
    let outcome = match engine.evaluate(&payload) {
        Ok(outcome) => outcome,
        Err(error) => fail(&error.to_string()),
    };

    if opts.json {
        match serde_json::to_string_pretty(&outcome) {
            Ok(text) => println!("{text}"),
            Err(error) => fail(&format!("failed to serialize the result: {error}")),
        }
        return;
    }

    for request in &outcome.results {
        println!("request {}", request.id);
        for answer in &request.answers {
            print_answer(answer);
        }
    }
    println!(
        "\n{} questions · {} candidate paths · {} backbone tokens · {} ms (model {})",
        outcome.usage.questions,
        outcome.usage.candidate_paths,
        outcome.usage.backbone_input_tokens,
        started.elapsed().as_millis(),
        outcome.model,
    );
}

fn print_answer(answer: &skillstar_decision::AnswerDto) {
    let head = format!(
        "  {} ({}) → {}  p={:.3}",
        answer.id,
        answer.kind.as_str(),
        answer.selected_description,
        answer.top_probability
    );
    match answer.kind {
        skillstar_decision::QuestionKind::Boolean => {
            let probability = answer.probability_true.unwrap_or_default();
            println!("{head}  [true {probability:.3} | false {:.3}]", 1.0 - probability);
        }
        skillstar_decision::QuestionKind::Choice => {
            println!("{head}  margin={:.3}", answer.margin);
            print_distribution(answer);
        }
        skillstar_decision::QuestionKind::Score => {
            println!(
                "{head}  score={:.2} level={}",
                answer.score.unwrap_or_default(),
                answer.level.unwrap_or_default()
            );
            print_distribution(answer);
        }
    }
}

fn print_distribution(answer: &skillstar_decision::AnswerDto) {
    const WIDTH: usize = 24;
    for entry in &answer.distribution {
        let filled = (entry.probability.clamp(0.0, 1.0) * WIDTH as f32).round() as usize;
        let bar = "█".repeat(filled);
        let description = answer
            .level_descriptions
            .get(entry.key.parse::<usize>().unwrap_or(usize::MAX))
            .cloned()
            .unwrap_or_default();
        let label = if description.is_empty() || answer.kind == skillstar_decision::QuestionKind::Choice
        {
            entry.key.clone()
        } else {
            format!("{} · {description}", entry.key)
        };
        println!("      {label:<28} {:>6.3}  {bar}", entry.probability);
    }
}

fn engine_options(device: &str, dtype: &str) -> anyhow::Result<EngineOptions> {
    let device = match device {
        "auto" => DeviceChoice::Auto,
        "cpu" => DeviceChoice::Cpu,
        "metal" => DeviceChoice::Metal,
        other => anyhow::bail!("unknown device {other:?}; use auto, cpu or metal"),
    };
    let dtype = match dtype {
        "auto" => DTypeChoice::Auto,
        "f32" => DTypeChoice::F32,
        "f16" => DTypeChoice::F16,
        "bf16" => DTypeChoice::Bf16,
        other => anyhow::bail!("unknown dtype {other:?}; use auto, f32, f16 or bf16"),
    };
    Ok(EngineOptions { device, dtype })
}

fn describe(options: EngineOptions) -> String {
    let device = match options.device {
        DeviceChoice::Auto => "auto",
        DeviceChoice::Cpu => "cpu",
        DeviceChoice::Metal => "metal",
    };
    let dtype = match options.dtype {
        DTypeChoice::Auto => "auto",
        DTypeChoice::F32 => "f32",
        DTypeChoice::F16 => "f16",
        DTypeChoice::Bf16 => "bf16",
    };
    format!("{device}/{dtype}")
}

fn read_payload(opts: &DecideOpts<'_>) -> anyhow::Result<serde_json::Value> {
    let text = if opts.stdin || opts.file == Some("-") {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|error| anyhow::anyhow!("failed to read stdin: {error}"))?;
        buffer
    } else if let Some(file) = opts.file {
        std::fs::read_to_string(file)
            .map_err(|error| anyhow::anyhow!("failed to read {file}: {error}"))?
    } else {
        anyhow::bail!(
            "no payload: pass --file <path> (or - for stdin), --status, --verify or --download"
        );
    };
    serde_json::from_str(&text)
        .map_err(|error| anyhow::anyhow!("payload is not valid JSON: {error}"))
}

fn print_status(paths: &ModelPaths, json: bool) {
    let status = paths.status();
    if json {
        match serde_json::to_string_pretty(&status) {
            Ok(text) => println!("{text}"),
            Err(error) => fail(&format!("failed to serialize the status: {error}")),
        }
        return;
    }
    println!("checkpoint   {}", status.dir);
    println!("endpoint     {}", status.endpoint);
    println!("revision     {}", status.revision);
    println!(
        "state        {} ({:.1} / {:.1} MiB)",
        match status.state {
            ModelState::Missing => "missing",
            ModelState::Partial => "partial",
            ModelState::Ready => "ready",
        },
        status.present_bytes as f64 / 1_048_576.0,
        status.total_bytes as f64 / 1_048_576.0
    );
    for file in &status.files {
        println!(
            "  {} {}",
            if file.present { "✓" } else { "✗" },
            file.name
        );
    }
    if status.state != ModelState::Ready {
        println!("\nRun `skillstar decide --download` to fetch the checkpoint.");
    }
}

fn verify_checkpoint(paths: &ModelPaths, json: bool) {
    let result = paths.verify();
    if json {
        let payload = serde_json::json!({
            "dir": paths.dir().display().to_string(),
            "ok": result.is_ok(),
            "error": result.as_ref().err().map(ToString::to_string),
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(text) => println!("{text}"),
            Err(error) => fail(&format!("failed to serialize the result: {error}")),
        }
        return;
    }
    match result {
        Ok(()) => println!("✓ every checkpoint file matches its pinned SHA-256"),
        Err(error) => fail(&error.to_string()),
    }
}

fn download_checkpoint(paths: &ModelPaths) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => fail(&format!("failed to start the async runtime: {error}")),
    };
    let cancel = AtomicBool::new(false);
    let mut last_reported = 0u64;
    let result = runtime.block_on(download_with_shared_client(paths, &cancel, |progress| {
        // One line per percent-point keeps a 1.2 GB transfer from flooding the
        // terminal while still showing that it is moving.
        let percent = progress.downloaded * 100 / progress.total.max(1);
        if percent != last_reported {
            last_reported = percent;
            eprint!(
                "\r{:>3}%  {:.0} / {:.0} MiB  {}          ",
                percent,
                progress.downloaded as f64 / 1_048_576.0,
                progress.total as f64 / 1_048_576.0,
                progress.file
            );
        }
    }));
    eprintln!();
    match result {
        Ok(()) => println!("✓ checkpoint ready at {}", paths.dir().display()),
        Err(error) => fail(&error.to_string()),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("✗ {message}");
    std::process::exit(1);
}
