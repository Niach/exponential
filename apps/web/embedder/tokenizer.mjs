// #NEW-134: the XLM-R (SentencePiece Unigram) tokenizer of the embedding
// model, on @huggingface/tokenizers, with ONE change: the vocabulary lookup.
//
// The library builds a trie with a `Map` per node over the 250 002 pieces:
// ~370 MB of resident memory in the web image for a tokenizer. Its Viterbi
// search only ever asks the trie for "every piece that is a prefix of the
// text from here", and the same question is answered by probing the
// token → id map the model keeps anyway, bounded by the longest piece. So the
// model is built on a four-piece vocabulary (a four-node trie) and then handed
// the full vocabulary and a map-backed prefix search: the same tokens (the
// parity test compares both on DE/EN/CJK/emoji text), ~30 MB instead of ~370.
//
// Pinned to @huggingface/tokenizers 0.2.0 exactly: this reaches into the
// Unigram model's fields, and `assertUnigramShape` fails loudly on a bump
// that moved them.
import { Tokenizer } from "@huggingface/tokenizers"

class PrefixIndex {
  constructor(tokensToIds, maxPieceLength) {
    this.tokensToIds = tokensToIds
    this.maxPieceLength = maxPieceLength
  }
  *common_prefix_search(chars, start) {
    let prefix = ``
    const end = Math.min(chars.length, start + this.maxPieceLength)
    for (let i = start; i < end; i++) {
      prefix += chars[i]
      if (this.tokensToIds.has(prefix)) yield prefix
    }
  }
}

function assertUnigramShape(model) {
  for (const field of [`vocab`, `scores`, `tokens_to_ids`, `trie`, `unk_token_id`, `unk_score`, `min_score`]) {
    if (!(field in model)) throw new Error(`@huggingface/tokenizers: the Unigram model has no "${field}" any more`)
  }
  if (typeof model.trie.common_prefix_search !== `function`) {
    throw new Error(`@huggingface/tokenizers: the Unigram trie has no common_prefix_search any more`)
  }
}

/**
 * @param {any} tokenizerJson the parsed tokenizer.json (its vocabulary is consumed)
 * @param {{ maxTokens: number }} options
 */
export function buildTokenizer(tokenizerJson, { maxTokens }) {
  if (tokenizerJson.model?.type !== `Unigram`) throw new Error(`expected a Unigram tokenizer.json`)
  const fullVocab = tokenizerJson.model.vocab
  const unkId = tokenizerJson.model.unk_id
  // The stub must hold the unk piece (the constructor reads it).
  const stub = fullVocab.slice(0, Math.max(4, unkId + 1))
  const tokenizer = new Tokenizer(
    { ...tokenizerJson, model: { ...tokenizerJson.model, vocab: stub } },
    { model_max_length: maxTokens }
  )
  const model = tokenizer.model
  assertUnigramShape(model)

  const size = fullVocab.length
  const vocab = new Array(size)
  const scores = new Float64Array(size)
  const tokensToIds = new Map()
  let minScore = Infinity
  let maxPieceLength = 1
  for (let i = 0; i < size; i++) {
    const [piece, score] = fullVocab[i]
    vocab[i] = piece
    scores[i] = score
    tokensToIds.set(piece, i)
    if (score < minScore) minScore = score
    // Code points, the unit the lattice walks (`Array.from(sentence)`).
    let length = 0
    for (const _ of piece) length++
    if (length > maxPieceLength) maxPieceLength = length
  }
  // The constructor's own arithmetic, over the full vocabulary.
  model.vocab = vocab
  model.scores = scores
  model.tokens_to_ids = tokensToIds
  model.min_score = minScore
  model.unk_score = minScore - 10
  model.scores[unkId] = model.unk_score
  model.unk_token = vocab[unkId]
  model.bos_token_id = tokensToIds.get(model.bos_token)
  model.eos_token_id = tokensToIds.get(model.eos_token)
  model.trie = new PrefixIndex(tokensToIds, maxPieceLength)
  return tokenizer
}

/** `<s> … </s>` ids, cut to the model's position limit (the last id stays `</s>`). */
export function encodeForModel(tokenizer, text, maxTokens) {
  let ids = tokenizer.encode(text).ids
  if (ids.length > maxTokens) {
    const eos = ids[ids.length - 1]
    ids = ids.slice(0, maxTokens)
    ids[maxTokens - 1] = eos
  }
  return ids
}
