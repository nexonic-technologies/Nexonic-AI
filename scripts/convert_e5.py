"""Runs llama.cpp's convert_hf_to_gguf.py for multilingual-E5.

multilingual-E5-small is a BERT model (standard position table) that uses the
XLM-R SentencePiece tokenizer. llama.cpp's BertModel converter expects a
WordPiece vocabulary, and its XLMRobertaModel converter would shift the
position table, so neither fits as is. This keeps BertModel's tensor handling
and uses the converter's own XLM-R vocabulary routine when the model ships
sentencepiece.bpe.model. Nothing else is changed.

    python convert_e5.py <llama.cpp dir> <convert_hf_to_gguf.py arguments...>
"""

import runpy
import sys
from pathlib import Path

llama_cpp = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(llama_cpp))
sys.path.insert(1, str(llama_cpp / "gguf-py"))

from conversion import bert  # noqa: E402

_bert_vocab = bert.BertModel.set_vocab


def _set_vocab(self):
    if type(self) is bert.BertModel and (self.dir_model / "sentencepiece.bpe.model").is_file():
        self._xlmroberta_set_vocab()
    else:
        _bert_vocab(self)


bert.BertModel.set_vocab = _set_vocab

script = llama_cpp / "convert_hf_to_gguf.py"
sys.argv = [str(script), *sys.argv[2:]]
runpy.run_path(str(script), run_name="__main__")
