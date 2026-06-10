<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		loading = false,
		error = null,
		actions,
		children
	}: {
		title: string;
		loading?: boolean;
		error?: string | null;
		actions?: Snippet;
		children: Snippet;
	} = $props();
</script>

<section class="halo-card panel">
	<header>
		<h2>{title}</h2>
		{#if actions}
			<div class="actions">{@render actions()}</div>
		{/if}
	</header>
	{#if error}
		<p class="state err">{error}</p>
	{:else if loading}
		<p class="state muted">Loading…</p>
	{:else}
		{@render children()}
	{/if}
</section>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
	}
	h2 {
		margin: 0;
		font-family: var(--halo-font-heading);
		font-size: 1.05rem;
		font-weight: 600;
	}
	.state {
		margin: 0;
	}
	.muted {
		color: var(--halo-text-muted);
	}
	.err {
		color: var(--halo-error);
	}
</style>
