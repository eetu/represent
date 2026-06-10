<script lang="ts">
	// Demo countdown: a slim bar that drains over `total` seconds, restarted
	// whenever `resetKey` changes (i.e. on every file swipe). Purely visual —
	// it never auto-advances; the presenter owns the pace.
	let { total, resetKey }: { total: number; resetKey: string } = $props();

	let remaining = $state(0);

	$effect(() => {
		void resetKey;
		remaining = total;
		const started = performance.now();
		const tick = setInterval(() => {
			remaining = Math.max(0, total - (performance.now() - started) / 1000);
		}, 250);
		return () => clearInterval(tick);
	});

	const mmss = $derived.by(() => {
		const s = Math.ceil(remaining);
		return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
	});
</script>

<div class="timer" class:done={remaining <= 0}>
	<div class="track">
		<div class="fill" style:width="{(remaining / total) * 100}%"></div>
	</div>
	<span class="clock">{mmss}</span>
</div>

<style>
	.timer {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		flex: 1;
		min-width: 0;
	}
	.track {
		flex: 1;
		height: 4px;
		background: var(--halo-off-bg);
		border-radius: 2px;
		overflow: hidden;
	}
	.fill {
		height: 100%;
		background: var(--halo-accent);
		border-radius: 2px;
		transition: width 0.25s linear;
	}
	.clock {
		font-family: var(--halo-font-heading);
		font-variant-numeric: tabular-nums;
		font-size: 0.85rem;
		color: var(--halo-text-muted);
	}
	.done .clock {
		color: var(--halo-error);
		animation: pulse 1.2s ease-in-out infinite;
	}
	@keyframes pulse {
		50% {
			opacity: 0.35;
		}
	}
</style>
