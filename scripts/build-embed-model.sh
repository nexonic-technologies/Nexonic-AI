#!/usr/bin/env bash
# Builds the on-device embedding model (multilingual-E5-small, MIT) as GGUF Q8_0
# from the official files at a pinned revision, with llama.cpp's own converter
# at the exact llama.cpp commit our Rust engine (llama-cpp-sys-2) is built from.
#
#   scripts/build-embed-model.sh <out.gguf>
#
# Needs: curl, git, uv (or python3.12), and GH_TOKEN (or gh logged in) to look up the
# llama.cpp submodule commit. Everything downloaded is pinned or verified.
set -euo pipefail

OUT="${1:?usage: build-embed-model.sh <out.gguf>}"
REPO="intfloat/multilingual-e5-small"
REV="fd1525a9fd15316a2d503bf26ab031a61d056e98"
WEIGHTS_SHA="1a55775f53449dac10a2bcbc312469fac40b96d53198c407081a831f81c98477"
# llama-cpp-sys-2 0.1.161 (Cargo.lock) was published from this llama-cpp-rs commit.
LLAMA_RS_COMMIT="e7af607ebd5ec2932709ef918c17cca2f9b0fce4"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
SRC="$WORK/model"
mkdir -p "$SRC/1_Pooling"

echo "== official model files @ $REV"
for f in config.json model.safetensors sentencepiece.bpe.model special_tokens_map.json \
         tokenizer.json tokenizer_config.json modules.json sentence_bert_config.json \
         1_Pooling/config.json; do
  curl -fsSL --retry 3 -o "$SRC/$f" "https://huggingface.co/$REPO/resolve/$REV/$f"
done
echo "$WEIGHTS_SHA  $SRC/model.safetensors" | shasum -a 256 -c -

echo "== llama.cpp converter at the engine's commit"
LLAMA_CPP_COMMIT="$(gh api "repos/utilityai/llama-cpp-rs/contents/llama-cpp-sys-2/llama.cpp?ref=$LLAMA_RS_COMMIT" --jq .sha)"
echo "llama.cpp $LLAMA_CPP_COMMIT"
git init -q "$WORK/llama.cpp"
git -C "$WORK/llama.cpp" fetch -q --depth 1 https://github.com/ggml-org/llama.cpp "$LLAMA_CPP_COMMIT"
git -C "$WORK/llama.cpp" checkout -q FETCH_HEAD

echo "== converter dependencies (Python 3.12, the converter's pinned torch/numpy need it)"
REQ="$WORK/llama.cpp/requirements/requirements-convert_hf_to_gguf.txt"
if command -v uv >/dev/null; then
  uv venv -q --python 3.12 "$WORK/venv"
  (cd "$WORK/llama.cpp/requirements" && uv pip install -q --python "$WORK/venv/bin/python" -r "$REQ")
  # sentencepiece's model proto needs protobuf.runtime_version (protobuf 5.26+).
  uv pip install -q --python "$WORK/venv/bin/python" "protobuf>=5.26,<7"
else
  python3.12 -m venv "$WORK/venv"
  "$WORK/venv/bin/pip" install -q -r "$REQ"
  "$WORK/venv/bin/pip" install -q "protobuf>=5.26,<7"
fi

echo "== convert"
mkdir -p "$(dirname "$OUT")"
# convert_e5.py: BERT tensors + the converter's own XLM-R tokenizer routine.
"$WORK/venv/bin/python" "$(dirname "$0")/convert_e5.py" "$WORK/llama.cpp" "$SRC" --outtype q8_0 --outfile "$OUT"
ls -l "$OUT"
shasum -a 256 "$OUT"
