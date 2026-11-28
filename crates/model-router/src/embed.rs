//! On-device text embeddings for memory recall by meaning.
//!
//! multilingual-E5-small (MIT, ~100 languages) converted to GGUF at build time
//! from the official files. Runs on its own thread; vectors are mean-pooled
//! and L2-normalised, so the dot product is the cosine similarity.

use crate::local::backend;
use crate::RouterError;
use llama_cpp_2::context::params::{LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

/// The model's longest input; longer texts are cut.
const MAX_TOKENS: usize = 512;

/// E5 expects these prefixes, even for non-English text.
pub fn as_query(text: &str) -> String {
    format!("query: {text}")
}

pub fn as_passage(text: &str) -> String {
    format!("passage: {text}")
}

/// Cosine similarity of two normalised vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normalise(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
}

struct Job {
    texts: Vec<String>,
    reply: mpsc::Sender<Result<Vec<Vec<f32>>, RouterError>>,
}

/// A loaded embedding model served by one worker thread.
pub struct Embedder {
    jobs: mpsc::Sender<Job>,
    path: PathBuf,
    dims: usize,
}

impl std::fmt::Debug for Embedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embedder")
            .field("path", &self.path)
            .field("dims", &self.dims)
            .finish()
    }
}

impl Embedder {
    /// Loads the model on the CPU: it is small enough to be quick there and
    /// keeps the GPU free for the brain (`NEXONIC_EMBED_GPU_LAYERS` overrides).
    pub fn start(path: &Path) -> Result<Self, RouterError> {
        if !path.is_file() {
            return Err(RouterError::LocalUnavailable(format!(
                "embedding model not found at {}",
                path.display()
            )));
        }
        let gpu_layers = std::env::var("NEXONIC_EMBED_GPU_LAYERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let (jobs, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<usize, RouterError>>();
        let model_path = path.to_path_buf();
        thread::Builder::new()
            .name("nexonic-embedder".into())
            .spawn(move || worker(&model_path, gpu_layers, &rx, &ready_tx))
            .map_err(|e| RouterError::LocalUnavailable(format!("could not start embedder: {e}")))?;
        let dims = ready_rx.recv().map_err(|_| {
            RouterError::LocalUnavailable("embedder stopped while loading".into())
        })??;
        Ok(Self {
            jobs,
            path: path.to_path_buf(),
            dims,
        })
    }

    pub fn dims(&self) -> usize {
        self.dims
    }

    /// One normalised vector per text, in order.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, RouterError> {
        let (reply, rx) = mpsc::channel();
        self.jobs
            .send(Job {
                texts: texts.to_vec(),
                reply,
            })
            .map_err(|_| RouterError::LocalUnavailable("the embedder has stopped".into()))?;
        rx.recv()
            .map_err(|_| RouterError::LocalUnavailable("the embedder has stopped".into()))?
    }
}

fn worker(
    path: &Path,
    gpu_layers: u32,
    jobs: &mpsc::Receiver<Job>,
    ready: &mpsc::Sender<Result<usize, RouterError>>,
) {
    let fail = |what: &str, e: &dyn std::fmt::Display| {
        RouterError::LocalUnavailable(format!("{what}: {e}"))
    };
    let backend = match backend() {
        Ok(b) => b,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
    let model = match LlamaModel::load_from_file(backend, path, &params) {
        Ok(m) => m,
        Err(e) => {
            let _ = ready.send(Err(fail("could not load embedding model", &e)));
            return;
        }
    };
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(MAX_TOKENS as u32))
        .with_n_batch(MAX_TOKENS as u32)
        .with_n_ubatch(MAX_TOKENS as u32)
        .with_embeddings(true)
        .with_pooling_type(LlamaPoolingType::Mean);
    let mut ctx = match model.new_context(backend, ctx_params) {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(fail("could not create embedding context", &e)));
            return;
        }
    };
    let dims = model.n_embd() as usize;
    let _ = ready.send(Ok(dims));
    let vocab = model.vocab();
    let mut batch = LlamaBatch::new(MAX_TOKENS, 1);
    for job in jobs {
        let mut out = Vec::with_capacity(job.texts.len());
        let mut result = Ok(());
        for text in &job.texts {
            let mut tokens = vocab.tokenize(text.as_bytes(), true, false);
            if tokens.len() > MAX_TOKENS {
                // Keep the closing [SEP]-style token the tokenizer added.
                let end = *tokens.last().expect("non-empty");
                tokens.truncate(MAX_TOKENS - 1);
                tokens.push(end);
            }
            if tokens.is_empty() {
                out.push(vec![0.0; dims]);
                continue;
            }
            ctx.clear_kv_cache();
            batch.clear();
            let mut step = || -> Result<Vec<f32>, RouterError> {
                for (i, tok) in tokens.iter().enumerate() {
                    batch
                        .add(*tok, i as i32, &[0], true)
                        .map_err(|e| fail("batch", &e))?;
                }
                ctx.decode(&mut batch).map_err(|e| fail("embed", &e))?;
                let mut v = ctx
                    .embeddings_seq_ith(0)
                    .map_err(|e| fail("embeddings", &e))?
                    .to_vec();
                normalise(&mut v);
                Ok(v)
            };
            match step() {
                Ok(v) => out.push(v),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        let _ = job.reply.send(result.map(|()| out));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_follow_e5() {
        assert_eq!(as_query("hi"), "query: hi");
        assert_eq!(as_passage("hi"), "passage: hi");
    }

    #[test]
    fn cosine_of_normalised_vectors() {
        let mut a = vec![3.0, 4.0];
        normalise(&mut a);
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-6);
        assert!(cosine(&a, &[-0.6, -0.8]) < -0.99);
    }

    #[test]
    fn missing_model_is_reported() {
        assert!(Embedder::start(Path::new("/nope/e5.gguf")).is_err());
    }

    /// Real run: `NEXONIC_EMBED_MODEL=/path/e5.gguf cargo test --features local --release -- --ignored embeds_by_meaning`
    #[test]
    #[ignore = "needs the embedding model (NEXONIC_EMBED_MODEL)"]
    fn embeds_by_meaning() {
        let path = std::env::var("NEXONIC_EMBED_MODEL").expect("NEXONIC_EMBED_MODEL");
        let t0 = std::time::Instant::now();
        let e = Embedder::start(Path::new(&path)).expect("load");
        println!("load {:?}, dims {}", t0.elapsed(), e.dims());
        let texts: Vec<String> = [
            as_passage("My sister is called Ayesha and she lives in Lahore."),
            as_passage("The deployment runs every Friday at 5 pm."),
            as_passage("I prefer green tea over coffee in the morning."),
            as_query("What is my sibling's name?"),
            as_query("When do we ship releases?"),
            as_query("صبح میں کیا پینا پسند کرتا ہوں؟"),
            // Roman Urdu is reported, not asserted: E5 understands it poorly.
            as_query("subah main kya peena pasand karta hoon?"),
        ]
        .into_iter()
        .collect();
        let t1 = std::time::Instant::now();
        let v = e.embed(&texts).expect("embed");
        let took = t1.elapsed();
        println!(
            "embed {} texts in {took:?} ({:?} per text)",
            texts.len(),
            took / texts.len() as u32
        );
        assert_eq!(v.len(), texts.len());
        assert!(v.iter().all(|x| x.len() == e.dims()));
        let best = |q: usize| {
            (0..3)
                .max_by(|&a, &b| cosine(&v[q], &v[a]).total_cmp(&cosine(&v[q], &v[b])))
                .unwrap()
        };
        for q in 3..texts.len() {
            println!(
                "{} -> {:?} best {}",
                texts[q],
                (0..3).map(|p| cosine(&v[q], &v[p])).collect::<Vec<_>>(),
                best(q)
            );
        }
        assert_eq!(best(3), 0, "sibling → sister");
        assert_eq!(best(4), 1, "ship releases → deployment");
        assert_eq!(best(5), 2, "Urdu drink → green tea");
    }
}
