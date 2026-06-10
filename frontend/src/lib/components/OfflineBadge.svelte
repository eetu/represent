<script lang="ts">
	// Quiet fixed pill, visible only when something is off: serving the local
	// copy, and/or edits queued for the backend's return.
	import { netState } from '$lib/offline.svelte';
</script>

{#if netState.offline || netState.pending > 0}
	<span class="badge">
		{#if netState.offline}offline · local copy{:else}reconnected{/if}
		{#if netState.pending > 0}· {netState.pending} queued{/if}
	</span>
{/if}

<style>
	.badge {
		position: fixed;
		right: 0.75rem;
		bottom: 0.75rem;
		z-index: 12;
		font-family: var(--halo-font-heading);
		font-size: 0.75rem;
		color: var(--halo-text-muted);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-accent);
		border-radius: var(--halo-radius);
		padding: 0.3rem 0.6rem;
		box-shadow: var(--halo-shadow);
	}
</style>
