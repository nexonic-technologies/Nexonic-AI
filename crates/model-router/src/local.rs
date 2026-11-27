//! The on-device brain: a GGUF model run by llama.cpp on its own thread.
//! On Apple Silicon every layer runs on the GPU through Metal.

use crate::{chatml_prompt, GenOptions, Message, RouterError, StopFlag, Utf8Stream};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

const BATCH: usize = 512;

enum Event {
    Token(String),
    Done(Result<String, RouterError>),
}

struct Job {
    messages: Vec<Message>,
    opts: GenOptions,
    stop: Arc<StopFlag>,
    events: mpsc::Sender<Event>,
}

/// A loaded model, served by one worker thread. Requests run one at a time.
pub struct LocalEngine {
    jobs: mpsc::Sender<Job>,
    path: PathBuf,
    ctx_tokens: u32,
}

impl std::fmt::Debug for LocalEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalEngine")
            .field("path", &self.path)
            .field("ctx_tokens", &self.ctx_tokens)
            .finish()
    }
}

/// Context size that fits the Mac: smaller on 8 GB machines.
pub fn context_for_memory(memory_bytes: u64) -> u32 {
    const GB: u64 = 1024 * 1024 * 1024;
    if memory_bytes >= 16 * GB {
        16_384
    } else if memory_bytes >= 8 * GB {
        8_192
    } else {
        4_096
    }
}

impl LocalEngine {
    /// Loads the model and starts the worker, with every layer on the GPU
    /// (`NEXONIC_GPU_LAYERS` overrides, e.g. 0 for CPU only).
    pub fn start(path: &Path, ctx_tokens: u32) -> Result<Self, RouterError> {
        let gpu_layers = std::env::var("NEXONIC_GPU_LAYERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);
        Self::start_with(path, ctx_tokens, gpu_layers)
    }

    pub fn start_with(path: &Path, ctx_tokens: u32, gpu_layers: u32) -> Result<Self, RouterError> {
        if !path.is_file() {
            return Err(RouterError::LocalUnavailable(format!(
                "model file not found at {}",
                path.display()
            )));
        }
        let (jobs, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), RouterError>>();
        let model_path = path.to_path_buf();
        thread::Builder::new()
            .name("nexonic-local-model".into())
            .spawn(move || worker(&model_path, ctx_tokens, gpu_layers, &rx, &ready_tx))
            .map_err(|e| RouterError::LocalUnavailable(format!("could not start worker: {e}")))?;
        ready_rx
            .recv()
            .map_err(|_| RouterError::LocalUnavailable("worker stopped while loading".into()))??;
        Ok(Self {
            jobs,
            path: path.to_path_buf(),
            ctx_tokens,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn context_tokens(&self) -> u32 {
        self.ctx_tokens
    }

    /// Streams a reply. Blocks until it is finished, stopped or failed.
    pub fn chat(
        &self,
        messages: Vec<Message>,
        opts: GenOptions,
        stop: Arc<StopFlag>,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String, RouterError> {
        let (events, rx) = mpsc::channel();
        self.jobs
            .send(Job {
                messages,
                opts,
                stop,
                events,
            })
            .map_err(|_| RouterError::LocalUnavailable("the model worker has stopped".into()))?;
        for ev in rx {
            match ev {
                Event::Token(t) => on_token(&t),
                Event::Done(result) => return result,
            }
        }
        Err(RouterError::LocalUnavailable(
            "the model worker has stopped".into(),
        ))
    }
}

fn worker(
    path: &Path,
    ctx_tokens: u32,
    gpu_layers: u32,
    jobs: &mpsc::Receiver<Job>,
    ready: &mpsc::Sender<Result<(), RouterError>>,
) {
    let mut backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => {
            let _ = ready.send(Err(RouterError::LocalUnavailable(format!(
                "llama.cpp: {e}"
            ))));
            return;
        }
    };
    backend.void_logs();
    // llama.cpp caps this at the real layer count.
    let params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
    let model = match LlamaModel::load_from_file(&backend, path, &params) {
        Ok(m) => m,
        Err(e) => {
            let _ = ready.send(Err(RouterError::LocalUnavailable(format!(
                "could not load model: {e}"
            ))));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    for job in jobs {
        let events = job.events.clone();
        let result = generate(&backend, &model, ctx_tokens, &job);
        let _ = events.send(Event::Done(result));
    }
}

fn generate(
    backend: &LlamaBackend,
    model: &LlamaModel,
    ctx_tokens: u32,
    job: &Job,
) -> Result<String, RouterError> {
    let fail = |what: &str, e: &dyn std::fmt::Display| {
        RouterError::LocalUnavailable(format!("{what}: {e}"))
    };
    let vocab = model.vocab();
    let prompt = chatml_prompt(&job.messages);
    let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
    let room = ctx_tokens as usize;
    let max_new = job.opts.max_tokens as usize;
    if tokens.is_empty() || tokens.len() + max_new > room {
        return Err(RouterError::LocalUnavailable(format!(
            "conversation too long for the on-device model ({} tokens, limit {room})",
            tokens.len() + max_new
        )));
    }

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(ctx_tokens))
        .with_n_batch(BATCH as u32);
    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| fail("could not create context", &e))?;

    let mut batch = LlamaBatch::new(BATCH, 1);
    let last = tokens.len() - 1;
    for (chunk_no, chunk) in tokens.chunks(BATCH).enumerate() {
        batch.clear();
        for (i, tok) in chunk.iter().enumerate() {
            let pos = chunk_no * BATCH + i;
            batch
                .add(*tok, pos as i32, &[0], pos == last)
                .map_err(|e| fail("batch", &e))?;
        }
        ctx.decode(&mut batch).map_err(|e| fail("prompt", &e))?;
        if job.stop.is_stopped() {
            return Err(RouterError::Cancelled);
        }
    }

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| d.subsec_nanos());
    // Qwen's recommended non-thinking settings.
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::top_k(20),
        LlamaSampler::top_p(0.8, 1),
        LlamaSampler::temp(job.opts.temperature),
        LlamaSampler::dist(seed),
    ]);

    let mut text = String::new();
    let mut utf8 = Utf8Stream::default();
    for pos in (tokens.len() as i32..).take(max_new) {
        if job.stop.is_stopped() {
            return Err(RouterError::Cancelled);
        }
        let tok = sampler.sample(&ctx, batch.n_tokens() - 1);
        if vocab.is_eog(tok) {
            break;
        }
        let piece = utf8.push(&vocab.token_to_piece(tok, false, None));
        if !piece.is_empty() {
            text.push_str(&piece);
            if text.contains("<|im_end|>") {
                text.truncate(text.find("<|im_end|>").unwrap_or(text.len()));
                break;
            }
            let _ = job.events.send(Event::Token(piece));
        }
        batch.clear();
        batch
            .add(tok, pos, &[0], true)
            .map_err(|e| fail("batch", &e))?;
        ctx.decode(&mut batch).map_err(|e| fail("decode", &e))?;
    }
    let reply = text.trim().to_string();
    if reply.is_empty() {
        return Err(RouterError::LocalUnavailable(
            "the model returned an empty reply".into(),
        ));
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;

    #[test]
    fn context_follows_memory() {
        const GB: u64 = 1024 * 1024 * 1024;
        assert_eq!(context_for_memory(4 * GB), 4_096);
        assert_eq!(context_for_memory(8 * GB), 8_192);
        assert_eq!(context_for_memory(24 * GB), 16_384);
    }

    #[test]
    fn missing_model_is_reported() {
        let err = LocalEngine::start(Path::new("/nonexistent/model.gguf"), 4096).unwrap_err();
        assert!(matches!(err, RouterError::LocalUnavailable(_)));
    }

    fn timed_run(gpu_layers: u32) {
        let path = std::env::var("NEXONIC_MODEL").expect("set NEXONIC_MODEL");
        let t0 = std::time::Instant::now();
        let engine =
            LocalEngine::start_with(Path::new(&path), 4096, gpu_layers).expect("model loads");
        let load = t0.elapsed();
        let mut streamed = String::new();
        let mut first: Option<std::time::Duration> = None;
        let mut pieces = 0u32;
        let t1 = std::time::Instant::now();
        let reply = engine
            .chat(
                vec![
                    Message::new(Role::System, "You are Nexa. Answer in two short sentences."),
                    Message::new(
                        Role::User,
                        "What is the capital of France, and one thing it is famous for?",
                    ),
                ],
                GenOptions {
                    max_tokens: 80,
                    temperature: 0.2,
                },
                Arc::new(StopFlag::default()),
                &mut |t| {
                    first.get_or_insert_with(|| t1.elapsed());
                    pieces += 1;
                    streamed.push_str(t);
                },
            )
            .expect("model replies");
        let total = t1.elapsed();
        let first = first.unwrap_or(total);
        let gen = total.saturating_sub(first).as_secs_f64().max(1e-3);
        println!(
            "gpu_layers={gpu_layers} load={load:?} first_token={first:?} total={total:?} pieces={pieces} ~{:.1} tok/s\nreply: {reply}",
            f64::from(pieces.saturating_sub(1)) / gen
        );
        assert!(reply.to_lowercase().contains("paris"), "{reply}");
        assert_eq!(streamed.trim(), reply);
        assert!(!reply.contains("<think>"));
    }

    /// Runs the real model. CI sets NEXONIC_MODEL to the bundled GGUF on the Mac runner.
    #[test]
    #[ignore = "needs the model file (set NEXONIC_MODEL)"]
    fn real_model_answers_gpu() {
        timed_run(1000);
    }

    #[test]
    #[ignore = "needs the model file (set NEXONIC_MODEL)"]
    fn real_model_answers_cpu() {
        timed_run(0);
    }
}
