<script lang="ts">
	import { resolve } from '$app/paths';
	import { api, type ProjectInfo } from '$lib/api';
	import Wordmark from '$lib/components/Wordmark.svelte';

	let projects = $state<ProjectInfo[] | null>(null);
	let error = $state<string | null>(null);
	let newName = $state('');
	let creating = $state(false);
	let me = $state<string | null>(null);

	const load = async () => {
		try {
			projects = (await api.projects()).projects;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	};

	$effect(() => {
		void load();
		void api.me().then(
			(m) => (me = m.email),
			() => (me = null)
		);
	});

	const create = async () => {
		const name = newName.trim();
		if (!name || creating) return;
		creating = true;
		try {
			await api.createProject(name);
			newName = '';
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			creating = false;
		}
	};

	const remove = async (name: string) => {
		if (!confirm(`delete project "${name}" and all its files?`)) return;
		try {
			await api.deleteProject(name);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	};

	const when = (iso: string | null) => (iso ? new Date(iso).toLocaleDateString() : '—');
</script>

<div class="shell">
	<header class="top">
		<Wordmark />
		{#if me}
			<span class="account">
				<span class="email">{me}</span>
				<button class="ghost" title="log out" onclick={() => void api.logout()}>logout</button>
			</span>
		{/if}
	</header>

	{#if error}
		<p class="err">{error}</p>
	{/if}

	{#if projects === null}
		<p class="muted">loading…</p>
	{:else if projects.length === 0}
		<p class="muted">no projects yet — name one below.</p>
	{:else}
		<ul class="projects">
			{#each projects as p (p.name)}
				<li class="halo-card">
					<a href={resolve('/p/[project]', { project: p.name })}>
						<span class="name">{p.name}</span>
						<span class="meta">
							{#if p.file_count === 0}empty{:else}{p.file_count} files · {when(p.updated_at)}{/if}
						</span>
					</a>
					<button class="ghost" title="delete project" onclick={() => remove(p.name)}>✕</button>
				</li>
			{/each}
		</ul>
	{/if}

	<form
		class="create"
		onsubmit={(e) => {
			e.preventDefault();
			void create();
		}}
	>
		<input placeholder="new project" bind:value={newName} />
		<button type="submit" disabled={!newName.trim() || creating}>create</button>
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
		align-items: baseline;
		gap: 1rem;
		flex-wrap: wrap;
	}
	.account {
		margin-left: auto;
		display: inline-flex;
		align-items: baseline;
		gap: 0.5rem;
		min-width: 0;
	}
	.email {
		color: var(--halo-text-muted);
		font-size: 0.8rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.muted {
		color: var(--halo-text-muted);
	}
	.err {
		color: var(--halo-error);
	}
	.projects {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	/* Rows carry no padding of their own — the link and the delete button
	   stretch the full card height so taps can't fall into dead zones. */
	.projects li {
		display: flex;
		align-items: stretch;
		padding: 0;
	}
	.projects a {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 0.15rem;
		text-decoration: none;
		color: inherit;
		padding: 1rem 0.4rem 1rem 1.25rem;
	}
	.name {
		font-family: var(--halo-font-heading);
		font-weight: 500;
		font-size: 1.05rem;
	}
	.meta {
		color: var(--halo-text-muted);
		font-size: 0.85rem;
	}
	.ghost {
		background: none;
		border: none;
		color: var(--halo-text-muted);
		font-size: 1rem;
		cursor: pointer;
		display: flex;
		align-items: center;
		padding: 0 1.25rem 0 0.7rem;
	}
	.ghost:hover {
		color: var(--halo-error);
		background: var(--halo-accent-soft);
	}
	.create {
		display: flex;
		gap: 0.5rem;
	}
	input {
		flex: 1;
		font: inherit;
		color: var(--halo-text-main);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 0.55rem 0.8rem;
	}
	button[type='submit'] {
		font-family: var(--halo-font-heading);
		font-size: 0.95rem;
		color: var(--halo-text-main);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		padding: 0.55rem 1rem;
		cursor: pointer;
	}
	button[type='submit']:not(:disabled):hover {
		border-color: var(--halo-accent);
		color: var(--halo-accent);
	}
	button[type='submit']:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
