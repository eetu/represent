<script lang="ts">
	import { flip } from 'svelte/animate';

	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { api, type FileInfo } from '$lib/api';

	const project = $derived(page.params.project ?? '');

	let files = $state<FileInfo[] | null>(null);
	let error = $state<string | null>(null);
	let newFile = $state('');

	const load = async () => {
		try {
			files = (await api.files(project)).files;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	};

	$effect(() => {
		void project;
		void load();
	});

	/** Copy-in: read each picked .md as text and PUT it into the project. */
	const upload = async (e: Event) => {
		const input = e.currentTarget as HTMLInputElement;
		try {
			for (const f of input.files ?? []) {
				const name = f.name.endsWith('.md') ? f.name : `${f.name}.md`;
				await api.saveFile(project, name, await f.text());
			}
			await load();
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			input.value = '';
		}
	};

	const create = async () => {
		let name = newFile.trim();
		if (!name) return;
		if (!name.endsWith('.md')) name += '.md';
		try {
			await api.saveFile(project, name, '');
			newFile = '';
			await goto(resolve('/p/[project]/f/[file]', { project, file: name }));
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	};

	const remove = async (name: string) => {
		if (!confirm(`delete "${name}"?`)) return;
		try {
			await api.deleteFile(project, name);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	};

	// ---------- inline rename ----------

	let renaming = $state<string | null>(null);
	let renameTo = $state('');

	const startRename = (name: string) => {
		renaming = name;
		renameTo = name;
	};

	const commitRename = async () => {
		const from = renaming;
		if (from === null) return;
		let to = renameTo.trim();
		renaming = null;
		if (!to || to === from) return;
		if (!to.endsWith('.md')) to += '.md';
		try {
			await api.renameFile(project, from, to);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	};

	// ---------- drag to reorder (pointer-based: mouse + touch) ----------

	// On drop the order is persisted server-side as metadata (a .order
	// sidecar) — file names are never touched.
	//
	// Geometry is sampled ONCE at drag start (row midpoints in viewport
	// coords) and the preview is always re-derived from the original order +
	// (from, to) — never spliced incrementally. Reading the live DOM during
	// the drag is what made the list oscillate: every reorder moved the rows
	// under the pointer, which changed the hit-test, which reordered again.
	let dragFrom = $state<number | null>(null);
	let dragTo = $state<number | null>(null);
	let reordering = $state(false);
	let rowMids: number[] = [];
	let dragStartY = 0;

	const display = $derived.by(() => {
		if (files === null || dragFrom === null || dragTo === null || dragFrom === dragTo) {
			return files;
		}
		const next = [...files];
		const [moved] = next.splice(dragFrom, 1);
		next.splice(dragTo, 0, moved);
		return next;
	});

	// Listeners live on window for the duration of the drag: the dragged <li>
	// is MOVED in the DOM when the preview reorders, which cancels pointer
	// capture on the handle — captured-on-handle events simply stop arriving
	// (stale drag, missed pointerup, nothing saved).
	const onDragUp = () => {
		stopDragListeners();
		void endDrag();
	};
	const onDragCancel = () => {
		stopDragListeners();
		cancelDrag();
	};
	const stopDragListeners = () => {
		window.removeEventListener('pointermove', moveDrag);
		window.removeEventListener('pointerup', onDragUp);
		window.removeEventListener('pointercancel', onDragCancel);
	};

	const startDrag = (e: PointerEvent, i: number) => {
		if (files === null || files.length < 2 || reordering) return;
		// No text selection while dragging.
		e.preventDefault();
		rowMids = [...document.querySelectorAll('[data-row]')].map((el) => {
			const r = el.getBoundingClientRect();
			return r.top + r.height / 2;
		});
		dragStartY = e.clientY;
		dragFrom = i;
		dragTo = i;
		window.addEventListener('pointermove', moveDrag);
		window.addEventListener('pointerup', onDragUp);
		window.addEventListener('pointercancel', onDragCancel);
	};

	const moveDrag = (e: PointerEvent) => {
		if (dragFrom === null || rowMids.length === 0) return;
		// Where the dragged row's own midpoint has virtually travelled —
		// anchoring to it (not the raw pointer) removes the grab-point bias:
		// one row of finger travel is one slot no matter where on the handle
		// the drag started. Nearest midpoint wins, so the flip threshold is
		// half a row in either direction (built-in hysteresis).
		const virtualMid = rowMids[dragFrom] + (e.clientY - dragStartY);
		let nearest = 0;
		for (let k = 1; k < rowMids.length; k++) {
			if (Math.abs(rowMids[k] - virtualMid) < Math.abs(rowMids[nearest] - virtualMid)) {
				nearest = k;
			}
		}
		dragTo = nearest;
	};

	const endDrag = async () => {
		if (dragFrom === null || dragTo === null || files === null) return;
		const order = (display ?? []).map((f) => f.name);
		const changed = dragFrom !== dragTo;
		dragFrom = null;
		dragTo = null;
		rowMids = [];
		if (!changed) return;
		reordering = true;
		try {
			files = (await api.reorder(project, order)).files;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			reordering = false;
		}
	};

	const cancelDrag = () => {
		dragFrom = null;
		dragTo = null;
		rowMids = [];
	};
</script>

<svelte:head>
	<title>{project} — represent</title>
</svelte:head>

<div class="shell">
	<header class="top">
		<a class="back" href={resolve('/')}>←</a>
		<h1>{project}</h1>
		<div class="actions">
			<label class="btn">
				upload
				<input type="file" accept=".md,.markdown,text/markdown" multiple onchange={upload} />
			</label>
			<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- API download URL, not an app route -->
			<a class="btn" href={api.bundleUrl(project)} download>bundle</a>
		</div>
	</header>

	{#if error}
		<p class="err">{error}</p>
	{/if}

	{#if files === null}
		<p class="muted">loading…</p>
	{:else if files.length === 0}
		<p class="muted">empty — upload files or add one below. drag rows to set the demo order.</p>
	{:else}
		<ol class="files" class:busy={reordering} class:lifting={dragFrom !== null}>
			{#each display ?? [] as f, i (f.name)}
				<li
					class="halo-card"
					class:dragging={dragFrom !== null && dragTo === i}
					data-row={i}
					animate:flip={{ duration: 150 }}
				>
					{#if (display ?? []).length > 1}
						<button class="handle" title="drag to reorder" onpointerdown={(e) => startDrag(e, i)}
							>≡</button
						>
					{/if}
					{#if renaming === f.name}
						<span class="renamer">
							<span class="pos">{i + 1}</span>
							<!-- the input *is* the requested action -->
							<!-- svelte-ignore a11y_autofocus -->
							<input
								autofocus
								size={Math.max(renameTo.length, 4)}
								bind:value={renameTo}
								onblur={() => void commitRename()}
								onkeydown={(e) => {
									if (e.key === 'Enter') void commitRename();
									if (e.key === 'Escape') renaming = null;
								}}
							/>
						</span>
					{:else}
						<a href={resolve('/p/[project]/f/[file]', { project, file: f.name })}>
							<span class="pos">{i + 1}</span>
							<span class="name">{f.name}</span>
						</a>
						<button class="ghost" title="rename file" onclick={() => startRename(f.name)}>✎</button>
						<button class="ghost" title="delete file" onclick={() => remove(f.name)}>✕</button>
					{/if}
				</li>
			{/each}
		</ol>
	{/if}

	<form
		class="create"
		onsubmit={(e) => {
			e.preventDefault();
			void create();
		}}
	>
		<input placeholder="new file (.md)" bind:value={newFile} />
		<button type="submit" disabled={!newFile.trim()}>add</button>
	</form>
</div>

<style>
	.shell {
		max-width: 720px;
		margin: 0 auto;
		padding: 2rem 1.25rem 4rem;
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}
	.top {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
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
		font-size: 1.3rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.actions {
		display: flex;
		gap: 0.5rem;
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
		text-decoration: none;
	}
	.btn:hover {
		border-color: var(--halo-accent);
		color: var(--halo-accent);
	}
	.btn input[type='file'] {
		display: none;
	}
	.muted {
		color: var(--halo-text-muted);
	}
	.err {
		color: var(--halo-error);
	}
	.files {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}
	/* Rows carry no padding of their own — every actionable child (handle,
	   link, delete) stretches the full card height so taps can't fall into
	   dead zones between them. */
	.files li {
		display: flex;
		align-items: stretch;
		padding: 0;
	}
	.files.busy {
		opacity: 0.6;
		pointer-events: none;
	}
	.files.lifting {
		user-select: none;
		-webkit-user-select: none;
	}
	.files li.dragging {
		outline: 1px solid var(--halo-accent);
		transform: scale(1.01);
		z-index: 1;
	}
	.handle {
		/* touch-action: none — dragging from the handle must not scroll. */
		touch-action: none;
		cursor: grab;
		background: none;
		border: none;
		color: var(--halo-text-muted);
		font-size: 1rem;
		display: flex;
		align-items: center;
		padding: 0 0.5rem 0 1.1rem;
	}
	.handle:active {
		cursor: grabbing;
		color: var(--halo-accent);
	}
	.files a {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: baseline;
		gap: 0.7rem;
		text-decoration: none;
		color: inherit;
		padding: 0.8rem 0.4rem;
	}
	/* No handle (single-file project) → the link owns the left edge. */
	.files a:first-child {
		padding-left: 1.1rem;
	}
	.pos {
		font-family: var(--halo-font-heading);
		color: var(--halo-text-muted);
		font-size: 0.85rem;
		min-width: 1.2em;
	}
	.name {
		flex: 1;
		min-width: 0;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* Mirrors the link row's geometry exactly (same paddings, gap, baseline
	   alignment) and the input contributes zero extra box: no border, no
	   padding — the accent underline is a box-shadow, which doesn't affect
	   layout. Nothing moves when the name swaps to an input. */
	.renamer {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: baseline;
		gap: 0.7rem;
		padding: 0.8rem 1.1rem 0.8rem 0.4rem;
	}
	.renamer:first-child {
		padding-left: 1.1rem;
	}
	.renamer input {
		/* Underline only as wide as the value: field-sizing where supported,
		   the char-count size attribute as the fallback. A shrinkable flex
		   item (explicit min-width overrides the min-content floor) so a long
		   name compresses inside the row instead of spilling out of the card. */
		flex: 0 1 auto;
		field-sizing: content;
		min-width: 4ch;
		font: inherit;
		font-weight: 500;
		color: var(--halo-text-main);
		background: none;
		border: none;
		border-radius: 0;
		padding: 0;
		outline: none;
		box-shadow: 0 1px 0 0 var(--halo-accent);
	}
	.renamer input:focus-visible {
		box-shadow: 0 2px 0 0 var(--halo-accent);
	}
	.ghost {
		background: none;
		border: none;
		color: var(--halo-text-muted);
		font-size: 1rem;
		cursor: pointer;
		display: flex;
		align-items: center;
		padding: 0 0.7rem;
	}
	.ghost:last-child {
		padding-right: 1.1rem;
	}
	.ghost:hover {
		color: var(--halo-error);
		background: var(--halo-accent-soft);
	}
	.ghost[title='rename file']:hover {
		color: var(--halo-accent);
	}
	.create {
		display: flex;
		gap: 0.5rem;
	}
	.create input {
		flex: 1;
		font: inherit;
		color: var(--halo-text-main);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 0.55rem 0.8rem;
	}
	.create button {
		font-family: var(--halo-font-heading);
		font-size: 0.95rem;
		color: var(--halo-text-main);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 0.55rem 1rem;
		cursor: pointer;
	}
	.create button:not(:disabled):hover {
		border-color: var(--halo-accent);
		color: var(--halo-accent);
	}
	.create button:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
