<script lang="ts">
	// represent brand mark: a script page (rounded outline + text lines) whose
	// active line is lit by the warm dot — the line you're presenting right now.
	// Single-stroke, currentColor + accent, in the family glyph style. See the
	// represent-design skill.
	let { size = 26 }: { size?: number } = $props();
</script>

<span class="halo-wordmark wordmark">
	<svg
		width={size}
		height={size}
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.8"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
	>
		<path d="M6 2.5h9.5L19 6v15.5H6z" />
		<path d="M9 8.5h6" />
		<path d="M12.6 12.5h2.4" />
		<path d="M9 16.5h3.5" />
		<circle class="dot" cx="9.9" cy="12.5" r="1.4" fill="var(--halo-accent)" stroke="none" />
	</svg>
	<span>re<span class="accent">present</span></span>
</span>

<style>
	.wordmark {
		display: inline-flex;
		align-items: center;
		gap: 0.5em;
		font-family: var(--halo-font-body);
		font-size: 1.4rem;
		font-weight: 600;
		letter-spacing: -0.04em;
	}
	.accent {
		color: var(--halo-accent);
	}
</style>
