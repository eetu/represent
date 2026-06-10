<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { api, type FileInfo } from '$lib/api';
	import MarkdownView, { type ArtifactHit } from '$lib/components/MarkdownView.svelte';
	import SelectionToolbar from '$lib/components/SelectionToolbar.svelte';
	import TimerBar from '$lib/components/TimerBar.svelte';
	import {
		findInBlock,
		insertNote,
		parseFrontmatter,
		removeBlock,
		renderBlocks,
		toggleWrap,
		unwrapArtifact
	} from '$lib/markdown';

	const project = $derived(page.params.project ?? '');
	const file = $derived(page.params.file ?? '');

	let src = $state<string | null>(null);
	let files = $state<FileInfo[]>([]);
	let error = $state<string | null>(null);
	// read = render + quick-edit tools; demo = wizard chrome (timer, swipe);
	// source = raw markdown textarea, the JIT fallback when a structural edit
	// is needed minutes before the talk.
	let mode = $state<'read' | 'demo' | 'source'>('read');
	let draft = $state('');
	let saving = $state(false);

	const parsed = $derived(src === null ? null : parseFrontmatter(src));
	const blocks = $derived(parsed === null ? [] : renderBlocks(parsed.body));
	const idx = $derived(files.findIndex((f) => f.name === file));
	const title = $derived(parsed?.meta.title ?? file.replace(/\.md$/, ''));

	$effect(() => {
		const p = project;
		const f = file;
		void (async () => {
			try {
				const [content, list] = await Promise.all([api.readFile(p, f), api.files(p)]);
				src = content.content;
				files = list.files;
				error = null;
				// Sync the rest of the project into the offline cache so the
				// whole demo survives the backend disappearing mid-talk.
				void Promise.allSettled(
					list.files.filter((x) => x.name !== f).map((x) => api.readFile(p, x.name))
				);
			} catch (e) {
				error = e instanceof Error ? e.message : String(e);
			}
		})();
	});

	// ---------- persistence ----------

	const persist = async (next: string) => {
		src = next;
		saving = true;
		try {
			await api.saveFile(project, file, next);
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	};

	const persistBody = (newBody: string) => {
		if (src === null || parsed === null) return;
		void persist(src.slice(0, parsed.bodyOffset) + newBody);
	};

	// ---------- selection → quick edits ----------

	type Sel = { idx: number; text: string; occurrence: number; x: number; y: number };
	let sel = $state<Sel | null>(null);
	let noteTarget = $state<Sel | null>(null);
	let noteText = $state('');

	const readSelection = (): Sel | null => {
		const s = window.getSelection();
		if (!s || s.isCollapsed || s.rangeCount === 0) return null;
		const text = s.toString();
		if (!text.trim() || text.includes('\n')) return null;
		const range = s.getRangeAt(0);
		const node = range.commonAncestorContainer;
		const el = (node instanceof Element ? node : node.parentElement)?.closest('[data-idx]');
		if (!el) return null;
		// Count how many times the selected string appears in the block's
		// rendered text before the selection — disambiguates repeated words.
		const pre = document.createRange();
		pre.selectNodeContents(el);
		pre.setEnd(range.startContainer, range.startOffset);
		const preText = pre.toString();
		let occurrence = 0;
		for (let i = preText.indexOf(text); i !== -1; i = preText.indexOf(text, i + 1)) occurrence++;
		const rect = range.getBoundingClientRect();
		return {
			idx: Number(el.getAttribute('data-idx')),
			text,
			occurrence,
			x: Math.min(Math.max(rect.left + rect.width / 2, 90), window.innerWidth - 90),
			y: Math.max(rect.top - 10, 56)
		};
	};

	$effect(() => {
		const onChange = () => {
			if (mode !== 'read') {
				sel = null;
				return;
			}
			sel = readSelection();
		};
		document.addEventListener('selectionchange', onChange);
		return () => document.removeEventListener('selectionchange', onChange);
	});

	const applyWrap = (mark: '==' | '~~') => {
		if (!sel || parsed === null) return;
		const block = blocks[sel.idx];
		const at = block ? findInBlock(parsed.body, block, sel.text, sel.occurrence) : null;
		if (at === null) return;
		persistBody(toggleWrap(parsed.body, at, sel.text.length, mark));
		window.getSelection()?.removeAllRanges();
	};

	const addNote = () => {
		if (parsed === null || noteTarget === null) return;
		const block = blocks[noteTarget.idx];
		if (block && noteText.trim()) persistBody(insertNote(parsed.body, block, noteText));
		noteTarget = null;
		noteText = '';
	};

	// ---------- artifact removal (tap a mark/strike, ✕ on a note) ----------

	let artifact = $state<ArtifactHit | null>(null);
	let fabHover = $state(false);
	let fabHide: ReturnType<typeof setTimeout> | null = null;

	const showArtifact = (hit: ArtifactHit) => {
		if (fabHide) clearTimeout(fabHide);
		artifact = hit;
	};

	// Mouse left the artifact — give the pointer a beat to reach the fab.
	const leaveArtifact = () => {
		if (fabHide) clearTimeout(fabHide);
		fabHide = setTimeout(() => {
			if (!fabHover) artifact = null;
		}, 250);
	};

	const removeArtifact = () => {
		if (artifact === null || parsed === null) return;
		const block = blocks[artifact.idx];
		const next = block
			? unwrapArtifact(parsed.body, block, artifact.kind, artifact.text, artifact.occurrence)
			: null;
		if (next !== null) persistBody(next);
		artifact = null;
	};

	const removeNote = (idx: number) => {
		if (parsed === null) return;
		const block = blocks[idx];
		if (block) persistBody(removeBlock(parsed.body, block));
	};

	// Dismiss the remove-chip on any tap outside it.
	$effect(() => {
		if (artifact === null) return;
		const onDown = (e: PointerEvent) => {
			if (!(e.target as Element).closest('.chip')) artifact = null;
		};
		document.addEventListener('pointerdown', onDown, true);
		return () => document.removeEventListener('pointerdown', onDown, true);
	});

	// ---------- navigation (swipe + keys) ----------

	const nav = async (delta: number) => {
		const target = idx === -1 ? undefined : files[idx + delta];
		if (!target) return;
		await goto(resolve('/p/[project]/f/[file]', { project, file: target.name }));
	};

	let touch: { x: number; y: number } | null = null;
	const onTouchStart = (e: TouchEvent) => {
		touch = { x: e.touches[0].clientX, y: e.touches[0].clientY };
	};
	const onTouchEnd = (e: TouchEvent) => {
		if (!touch) return;
		const dx = e.changedTouches[0].clientX - touch.x;
		const dy = e.changedTouches[0].clientY - touch.y;
		touch = null;
		// A live selection means the user was adjusting handles, not swiping.
		if (!window.getSelection()?.isCollapsed) return;
		if (mode === 'source') return;
		if (Math.abs(dx) > 64 && Math.abs(dy) < 48) void nav(dx < 0 ? 1 : -1);
	};

	const onKey = (e: KeyboardEvent) => {
		if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
		if (e.key === 'ArrowRight') void nav(1);
		if (e.key === 'ArrowLeft') void nav(-1);
		if (e.key === 'Escape' && mode === 'demo') mode = 'read';
	};

	// ---------- demo mode ----------

	// Keep the table device awake for the length of the demo (only — reading
	// or editing outside demo mode lets the phone manage itself). Best-effort:
	// the lock dies on tab-hide, so re-acquire on return.
	$effect(() => {
		if (mode !== 'demo' || !('wakeLock' in navigator)) return;
		let lock: WakeLockSentinel | null = null;
		const acquire = async () => {
			try {
				lock = await navigator.wakeLock.request('screen');
			} catch {
				// denied (low battery etc.) — the demo still works
			}
		};
		const onVisible = () => {
			if (document.visibilityState === 'visible') void acquire();
		};
		void acquire();
		document.addEventListener('visibilitychange', onVisible);
		return () => {
			document.removeEventListener('visibilitychange', onVisible);
			void lock?.release();
		};
	});

	const enterSource = () => {
		draft = src ?? '';
		mode = 'source';
	};

	const saveSource = async () => {
		await persist(draft);
		mode = 'read';
	};

	// Source-mode helper: the draft has a frontmatter fence iff bodyOffset > 0.
	const draftHasMeta = $derived(parseFrontmatter(draft).bodyOffset > 0);

	/** Prepend a title + timer frontmatter block, title prefilled from the
	 * file name (`02-the colony.md` → `the colony`). */
	const addMeta = () => {
		const guess = file
			.replace(/\.md$/, '')
			.replace(/^\d+[-_ ]*/, '')
			.trim();
		draft = `---\ntitle: ${guess || 'title'}\ntimer: 1:00\n---\n\n${draft.replace(/^\n+/, '')}`;
	};
</script>

<svelte:head>
	<title>{title} — represent</title>
</svelte:head>

<svelte:window onkeydown={onKey} />

<div
	class="viewer"
	class:demo={mode === 'demo'}
	role="presentation"
	ontouchstart={onTouchStart}
	ontouchend={onTouchEnd}
>
	{#if mode === 'demo'}
		<header class="demobar">
			{#if parsed?.meta.timer}
				<TimerBar total={parsed.meta.timer} resetKey={file} />
			{/if}
			<span class="pos">{idx + 1} / {files.length}</span>
			<button class="ghost" onclick={() => (mode = 'read')}>exit</button>
		</header>
	{:else}
		<header class="top">
			<a class="back" href={resolve('/p/[project]', { project })}>←</a>
			<h1>{title}</h1>
			<span class="pos">{idx === -1 ? '' : `${idx + 1} / ${files.length}`}</span>
			{#if mode === 'source'}
				<button class="btn" onclick={() => (mode = 'read')}>cancel</button>
				<button class="btn primary" disabled={saving} onclick={() => void saveSource()}>save</button
				>
			{:else}
				<button class="btn" onclick={enterSource}>source</button>
				<button class="btn primary" onclick={() => (mode = 'demo')}>demo</button>
			{/if}
		</header>
	{/if}

	{#if error}
		<p class="err">{error}</p>
	{/if}

	<main>
		{#if src === null}
			<p class="muted">loading…</p>
		{:else if mode === 'source'}
			{#if !draftHasMeta}
				<div class="helpers">
					<button class="btn" onclick={addMeta}>+ title & timer</button>
					<span class="muted">frontmatter the demo wizard reads</span>
				</div>
			{/if}
			<textarea bind:value={draft} spellcheck="false"></textarea>
		{:else if mode === 'demo'}
			<MarkdownView {blocks} large />
		{:else}
			<MarkdownView
				{blocks}
				onArtifact={showArtifact}
				onArtifactLeave={leaveArtifact}
				onRemoveNote={removeNote}
			/>
		{/if}
	</main>

	{#if mode !== 'demo' && files.length > 1}
		<footer class="hint muted">swipe or ←/→ to move between files</footer>
	{/if}
</div>

{#if sel && mode === 'read' && !noteTarget}
	<SelectionToolbar
		x={sel.x}
		y={sel.y}
		onHighlight={() => applyWrap('==')}
		onStrike={() => applyWrap('~~')}
		onNote={() => {
			noteTarget = sel;
			window.getSelection()?.removeAllRanges();
		}}
	/>
{/if}

{#if artifact}
	<button
		class="chip"
		style:left="{artifact.x}px"
		style:top="{artifact.y}px"
		title="remove {artifact.kind === 'mark' ? 'highlight' : 'strikethrough'}"
		onclick={removeArtifact}
		onpointerenter={() => (fabHover = true)}
		onpointerleave={() => {
			fabHover = false;
			leaveArtifact();
		}}
	>
		✕
	</button>
{/if}

{#if noteTarget}
	<div class="notebar">
		<!-- the input *is* the requested action -->
		<!-- svelte-ignore a11y_autofocus -->
		<input
			autofocus
			placeholder="note…"
			bind:value={noteText}
			onkeydown={(e) => {
				if (e.key === 'Enter') addNote();
				if (e.key === 'Escape') noteTarget = null;
			}}
		/>
		<button class="btn primary" onclick={addNote}>add</button>
		<button class="btn" onclick={() => (noteTarget = null)}>cancel</button>
	</div>
{/if}

<style>
	.viewer {
		max-width: 720px;
		margin: 0 auto;
		min-height: 100dvh;
		padding: 1.25rem 1.25rem 4rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.viewer.demo {
		padding-top: 0.75rem;
	}
	.top,
	.demobar {
		display: flex;
		align-items: center;
		gap: 0.7rem;
	}
	.back {
		text-decoration: none;
		color: var(--halo-text-muted);
		font-size: 1.3rem;
	}
	h1 {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-family: var(--halo-font-heading);
		font-size: 1.15rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.pos {
		font-family: var(--halo-font-heading);
		font-variant-numeric: tabular-nums;
		color: var(--halo-text-muted);
		font-size: 0.85rem;
		white-space: nowrap;
	}
	.btn {
		font-family: var(--halo-font-heading);
		font-size: 0.9rem;
		color: var(--halo-text-main);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 0.45rem 0.85rem;
		cursor: pointer;
	}
	.btn:hover:not(:disabled) {
		border-color: var(--halo-accent);
		color: var(--halo-accent);
	}
	.btn:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.btn.primary {
		color: var(--halo-accent);
		border-color: var(--halo-accent);
	}
	.ghost {
		background: none;
		border: none;
		color: var(--halo-text-muted);
		font-family: var(--halo-font-heading);
		font-size: 0.9rem;
		cursor: pointer;
		padding: 0.35rem 0.5rem;
	}
	.ghost:hover {
		color: var(--halo-text-main);
	}
	main {
		flex: 1;
		display: flex;
		flex-direction: column;
	}
	.helpers {
		display: flex;
		align-items: center;
		gap: 0.7rem;
		margin-bottom: 0.6rem;
		font-size: 0.8rem;
	}
	textarea {
		flex: 1;
		min-height: 60dvh;
		font-family: ui-monospace, 'SF Mono', monospace;
		font-size: 0.9rem;
		line-height: 1.55;
		color: var(--halo-text-main);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 1rem;
		resize: vertical;
	}
	.hint {
		font-size: 0.8rem;
		text-align: center;
	}
	.muted {
		color: var(--halo-text-muted);
	}
	.err {
		color: var(--halo-error);
	}
	/* Floating remove ✕ — a quiet round fab sitting on the artifact's
	   top-right corner (hover or tap); absolutely positioned so the text
	   never shifts. */
	.chip {
		position: fixed;
		transform: translate(-40%, -60%);
		width: 1.5rem;
		height: 1.5rem;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.7rem;
		line-height: 1;
		color: var(--halo-text-muted);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: 50%;
		box-shadow: var(--halo-shadow);
		cursor: pointer;
		padding: 0;
		z-index: 10;
	}
	.chip:hover {
		color: var(--halo-error);
		border-color: var(--halo-error);
	}
	.notebar {
		position: fixed;
		left: 0;
		right: 0;
		bottom: 0;
		display: flex;
		gap: 0.5rem;
		padding: 0.75rem 1rem calc(0.75rem + env(safe-area-inset-bottom));
		background: var(--halo-bg-main);
		border-top: 1px solid var(--halo-border);
		box-shadow: var(--halo-shadow);
		z-index: 11;
	}
	.notebar input {
		flex: 1;
		font: inherit;
		color: var(--halo-text-main);
		background: var(--halo-bg-light);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 0.55rem 0.8rem;
	}
</style>
