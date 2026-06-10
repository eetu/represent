<script lang="ts">
	import type { Block } from '$lib/markdown';

	export type ArtifactHit = {
		kind: 'mark' | 'del';
		idx: number;
		text: string;
		occurrence: number;
		x: number;
		y: number;
	};

	// Each block carries its index so a DOM selection (or a tapped artifact) can
	// be mapped back to a source range (see the viewer route). Block HTML is
	// DOMPurify-sanitized in renderBlocks() — the only place {@html} is fed from.
	// `onArtifact`/`onRemoveNote` switch the edit affordances on (read mode);
	// omit them for a passive render (demo mode).
	let {
		blocks,
		large = false,
		onArtifact,
		onArtifactLeave,
		onEditNote,
		onRemoveNote
	}: {
		blocks: Block[];
		large?: boolean;
		onArtifact?: (hit: ArtifactHit) => void;
		onArtifactLeave?: () => void;
		onEditNote?: (idx: number) => void;
		onRemoveNote?: (idx: number) => void;
	} = $props();

	/** Map a rendered ==mark== / ~~del~~ element to its source artifact. */
	const hitFrom = (target: Element): ArtifactHit | null => {
		const el = target.closest<HTMLElement>('mark, del');
		const blockEl = el?.closest('[data-idx]');
		if (!el || !blockEl) return null;
		const kind = el.tagName === 'MARK' ? 'mark' : 'del';
		const text = el.textContent ?? '';
		// nth same-text artifact of this kind within the block — disambiguates
		// repeated phrases the same way the selection toolbar does.
		const same = [...blockEl.querySelectorAll(kind)].filter((n) => n.textContent === text);
		const rect = el.getBoundingClientRect();
		return {
			kind,
			idx: Number(blockEl.getAttribute('data-idx')),
			text,
			occurrence: Math.max(0, same.indexOf(el)),
			// Anchor: the artifact's top-right corner.
			x: Math.min(Math.max(rect.right, 30), window.innerWidth - 30),
			y: Math.max(rect.top, 40)
		};
	};

	/** Tap (mobile + desktop): show the remove fab. */
	const onClick = (e: MouseEvent) => {
		if (!onArtifact) return;
		const hit = hitFrom(e.target as Element);
		if (hit) onArtifact(hit);
	};

	// Hover (mouse only — touch taps are handled by click): the fab appears on
	// enter and is retracted on leave, the viewer debounces so the pointer can
	// travel onto the fab itself.
	const onPointerOver = (e: PointerEvent) => {
		if (!onArtifact || e.pointerType !== 'mouse') return;
		const hit = hitFrom(e.target as Element);
		if (hit) onArtifact(hit);
	};
	const onPointerOut = (e: PointerEvent) => {
		if (e.pointerType !== 'mouse') return;
		if ((e.target as Element).closest('mark, del')) onArtifactLeave?.();
	};
</script>

<div
	class="md"
	class:large
	class:editable={!!onArtifact}
	role="presentation"
	onclick={onClick}
	onpointerover={onPointerOver}
	onpointerout={onPointerOut}
>
	{#each blocks as block, idx (block.start)}
		{#if block.note && onRemoveNote}
			<div class="block note" data-idx={idx}>
				<!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify in renderBlocks() -->
				{@html block.html}
				<span class="notetools">
					{#if onEditNote}
						<button class="notex" title="edit note" onclick={() => onEditNote(idx)}>✎</button>
					{/if}
					<button class="notex" title="remove note" onclick={() => onRemoveNote(idx)}>✕</button>
				</span>
			</div>
		{:else}
			<!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify in renderBlocks() -->
			<div class="block" data-idx={idx}>{@html block.html}</div>
		{/if}
	{/each}
</div>

<style>
	.md {
		font-size: 1.05rem;
		line-height: 1.65;
		overflow-wrap: break-word;
	}
	.md.large {
		font-size: 1.25rem;
		line-height: 1.7;
	}

	.note {
		position: relative;
	}
	.notetools {
		position: absolute;
		top: 0.35rem;
		right: 0.25rem;
		display: flex;
		gap: 0.1rem;
	}
	.notex {
		background: none;
		border: none;
		color: var(--halo-text-muted);
		font-size: 0.85rem;
		line-height: 1;
		padding: 0.3rem;
		cursor: pointer;
		border-radius: var(--halo-radius-pill);
	}
	.notex:hover {
		color: var(--halo-error);
		background: var(--halo-accent-soft);
	}
	.notex[title='edit note']:hover {
		color: var(--halo-accent);
	}

	/* Injected markup — scoped selectors can't reach it without :global. */
	.md :global(h1),
	.md :global(h2),
	.md :global(h3),
	.md :global(h4) {
		font-family: var(--halo-font-heading);
		line-height: 1.25;
		margin: 1.2em 0 0.4em;
	}
	.md :global(h1:first-child),
	.md :global(h2:first-child) {
		margin-top: 0;
	}
	.md :global(h1) {
		font-size: 1.6em;
	}
	.md :global(h2) {
		font-size: 1.3em;
	}
	.md :global(h3) {
		font-size: 1.1em;
	}
	.md :global(p) {
		margin: 0.6em 0;
	}
	.md :global(a) {
		color: var(--halo-accent);
	}
	.md :global(code) {
		font-size: 0.9em;
		background: var(--halo-bg-light);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius-pill);
		padding: 0.1em 0.35em;
	}
	.md :global(pre) {
		background: var(--halo-bg-light);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 0.85em 1em;
		overflow-x: auto;
	}
	.md :global(pre code) {
		background: none;
		border: none;
		padding: 0;
	}
	.md :global(table) {
		border-collapse: collapse;
	}
	.md :global(th),
	.md :global(td) {
		border: 1px solid var(--halo-border);
		padding: 0.3em 0.7em;
		text-align: left;
	}
	.md :global(img) {
		max-width: 100%;
		border-radius: var(--halo-radius);
	}
	.md :global(hr) {
		border: none;
		border-top: 1px solid var(--halo-border);
		margin: 1.5em 0;
	}

	/* The three quick-edit artifacts. */
	.md :global(mark) {
		background: var(--halo-accent-bg);
		color: inherit;
		border-radius: var(--halo-radius-pill);
		padding: 0 0.15em;
	}
	.md :global(del) {
		color: var(--halo-text-muted);
	}
	.md :global(blockquote) {
		margin: 0.8em 0;
		padding: 0.4em 3.8em 0.4em 1em;
		border-left: 3px solid var(--halo-accent);
		background: var(--halo-bg-light);
		border-radius: 0 var(--halo-radius) var(--halo-radius) 0;
	}
	.md :global(blockquote p) {
		margin: 0.2em 0;
	}

	/* Edit affordances (read mode only): hovering or tapping an artifact pops
	   the floating remove ✕ (rendered by the viewer route, absolutely
	   positioned so nothing in the text shifts). */
	.md.editable :global(mark),
	.md.editable :global(del) {
		cursor: pointer;
	}
</style>
