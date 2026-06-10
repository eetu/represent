// Markdown pipeline: frontmatter → per-block rendering with source offsets →
// sanitized HTML. Blocks keep their [start, end) range in the body so the
// quick-edit toolbar can map a DOM selection back to the markdown source and
// rewrite it (==highlight==, ~~strike~~, inserted notes).

import DOMPurify from 'dompurify';
import { Marked, type TokenizerAndRendererExtension } from 'marked';

/** Demo-wizard config carried in the file's own frontmatter. */
export type Frontmatter = {
	/** Seconds for this file's demo timer; null = no timer. */
	timer: number | null;
	title: string | null;
};

export type Block = {
	html: string;
	/** Byte^W char offsets into the body (frontmatter excluded). */
	start: number;
	end: number;
	/** True for `> **note:** …` blockquotes — the removable note artifact. */
	note: boolean;
};

// ==text== → <mark>: the syntax the highlight tool writes. Not GFM — a small
// inline extension, so highlights survive the markdown round-trip as plain
// text in any other renderer.
const highlight: TokenizerAndRendererExtension = {
	name: 'highlight',
	level: 'inline',
	start: (src: string) => src.indexOf('=='),
	tokenizer(src) {
		const m = /^==([^=\n]+(?:=[^=\n]+)*)==/.exec(src);
		if (!m) return undefined;
		return {
			type: 'highlight',
			raw: m[0],
			text: m[1],
			tokens: this.lexer.inlineTokens(m[1])
		};
	},
	renderer(token) {
		return `<mark>${this.parser.parseInline(token.tokens ?? [])}</mark>`;
	}
};

const marked = new Marked({ gfm: true, extensions: [highlight] });

/** `"90"` → 90 seconds, `"1:30"` → 90 seconds. */
export function parseTimer(v: string): number | null {
	const clock = /^(\d+):([0-5]\d)$/.exec(v);
	if (clock) return Number(clock[1]) * 60 + Number(clock[2]);
	const secs = /^\d+$/.exec(v);
	return secs ? Number(v) : null;
}

/**
 * Minimal frontmatter: a leading `---` fence with `key: value` lines. Not
 * YAML — only the flat keys the wizard reads (`timer`, `title`), so no parser
 * dependency. `bodyOffset` is where the body starts in the original source;
 * edits are applied to the body and re-prefixed with the untouched head.
 */
export function parseFrontmatter(src: string): {
	meta: Frontmatter;
	body: string;
	bodyOffset: number;
} {
	const meta: Frontmatter = { timer: null, title: null };
	const none = { meta, body: src, bodyOffset: 0 };
	if (!/^---\r?\n/.test(src)) return none;
	const close = /\r?\n---[ \t]*(\r?\n|$)/.exec(src.slice(3));
	if (!close) return none;
	const head = src.slice(3, 3 + close.index);
	const bodyOffset = 3 + close.index + close[0].length;
	for (const line of head.split(/\r?\n/)) {
		const m = /^([A-Za-z_]+):\s*(.+?)\s*$/.exec(line);
		if (!m) continue;
		if (m[1] === 'title') meta.title = m[2];
		if (m[1] === 'timer') meta.timer = parseTimer(m[2]);
	}
	return { meta, body: src.slice(bodyOffset), bodyOffset };
}

/**
 * Render the body block-by-block. marked's lexer guarantees the concatenation
 * of top-level `token.raw` reproduces its input, so cumulative raw lengths
 * give each block's source range.
 */
export function renderBlocks(body: string): Block[] {
	const tokens = marked.lexer(body);
	const blocks: Block[] = [];
	let pos = 0;
	for (const token of tokens) {
		const start = pos;
		pos += token.raw.length;
		if (token.type === 'space') continue;
		const html = marked.parser([token]) as string;
		const note = token.type === 'blockquote' && /^>\s*\*\*note:\*\*/.test(token.raw);
		blocks.push({ html: DOMPurify.sanitize(html), start, end: pos, note });
	}
	return blocks;
}

/**
 * Absolute body offset of the `occurrence`-th appearance of `text` inside the
 * block (0-based). The occurrence index comes from counting in the *rendered*
 * text, which can disagree with the raw source when inline marks intervene —
 * fall back to the first raw occurrence rather than failing the edit.
 */
export function findInBlock(
	body: string,
	block: Block,
	text: string,
	occurrence: number
): number | null {
	const raw = body.slice(block.start, block.end);
	const hits: number[] = [];
	for (let i = raw.indexOf(text); i !== -1; i = raw.indexOf(text, i + 1)) {
		hits.push(i);
	}
	if (hits.length === 0) return null;
	return block.start + (hits[occurrence] ?? hits[0]);
}

/**
 * Wrap `[at, at+len)` with `mark` (`==` or `~~`), or unwrap when the range is
 * already wrapped — tapping highlight twice undoes it.
 */
export function toggleWrap(body: string, at: number, len: number, mark: string): string {
	const before = body.slice(at - mark.length, at);
	const after = body.slice(at + len, at + len + mark.length);
	if (before === mark && after === mark) {
		return (
			body.slice(0, at - mark.length) +
			body.slice(at, at + len) +
			body.slice(at + len + mark.length)
		);
	}
	return body.slice(0, at) + mark + body.slice(at, at + len) + mark + body.slice(at + len);
}

/**
 * Remove a `==text==` / `~~text~~` wrapper, keeping the text. `text` is the
 * rendered textContent of the tapped element; nested inline formatting makes
 * it differ from the raw source, in which case the edit is a no-op (null)
 * rather than a wrong guess.
 */
export function unwrapArtifact(
	body: string,
	block: Block,
	kind: 'mark' | 'del',
	text: string,
	occurrence: number
): string | null {
	const t = kind === 'mark' ? '==' : '~~';
	const needle = t + text + t;
	const raw = body.slice(block.start, block.end);
	const hits: number[] = [];
	for (let i = raw.indexOf(needle); i !== -1; i = raw.indexOf(needle, i + 1)) {
		hits.push(i);
	}
	if (hits.length === 0) return null;
	const at = block.start + (hits[occurrence] ?? hits[0]);
	return body.slice(0, at) + text + body.slice(at + needle.length);
}

/** Remove a whole block (a note), collapsing the blank lines around it. */
export function removeBlock(body: string, block: Block): string {
	const head = body.slice(0, block.start).replace(/\n{3,}$/, '\n\n');
	const tail = body.slice(block.end).replace(/^\n+/, '');
	if (!head) return tail;
	return head.endsWith('\n') ? head + tail : `${head}\n\n${tail}`;
}

/** The editable text of a `> **note:** …` block (markers stripped). */
export function getNoteText(body: string, block: Block): string {
	return body
		.slice(block.start, block.end)
		.split(/\r?\n/)
		.map((l) => l.replace(/^>\s?/, ''))
		.join('\n')
		.replace(/^\s*\*\*note:\*\*\s*/, '')
		.trim();
}

/** Replace an existing note block with new text, keeping its position. */
export function replaceNote(body: string, block: Block, text: string): string {
	const head = body.slice(0, block.start);
	const tail = body.slice(block.end).replace(/^\n+/, '');
	return `${head}> **note:** ${text.trim()}\n\n${tail}`;
}

/** Insert a note blockquote right after the block the selection sits in. */
export function insertNote(body: string, block: Block, text: string): string {
	const head = body.slice(0, block.end);
	const tail = body.slice(block.end);
	const sep = head.endsWith('\n') ? '\n' : '\n\n';
	return `${head}${sep}> **note:** ${text.trim()}\n\n${tail.replace(/^\n+/, '')}`;
}
