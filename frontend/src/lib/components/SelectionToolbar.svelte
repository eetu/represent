<script lang="ts">
	// Floating quick-edit toolbar, shown above the active text selection.
	// pointerdown is preventDefault'ed on the whole bar so tapping a tool
	// doesn't dismiss the selection before the action reads it. The note tool
	// only signals the page — the text input lives in a fixed bar there,
	// independent of the (collapsing) selection.
	let {
		x,
		y,
		onHighlight,
		onStrike,
		onNote
	}: {
		x: number;
		y: number;
		onHighlight: () => void;
		onStrike: () => void;
		onNote: () => void;
	} = $props();
</script>

<div
	class="bar"
	style:left="{x}px"
	style:top="{y}px"
	role="toolbar"
	tabindex="-1"
	onpointerdown={(e) => e.preventDefault()}
>
	<button onclick={onHighlight}><span class="hl">mark</span></button>
	<button onclick={onStrike}><del>strike</del></button>
	<button onclick={onNote}>note</button>
</div>

<style>
	.bar {
		position: fixed;
		transform: translate(-50%, -100%);
		display: flex;
		gap: 0.25rem;
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		box-shadow: var(--halo-shadow);
		padding: 0.3rem;
		z-index: 10;
	}
	button {
		font-family: var(--halo-font-heading);
		font-size: 0.9rem;
		color: var(--halo-text-main);
		background: none;
		border: none;
		border-radius: var(--halo-radius-pill);
		padding: 0.35rem 0.6rem;
		cursor: pointer;
	}
	button:hover {
		background: var(--halo-accent-soft);
	}
	.hl {
		background: var(--halo-accent-bg);
		border-radius: var(--halo-radius-pill);
		padding: 0 0.15em;
	}
	del {
		color: var(--halo-text-muted);
	}
</style>
