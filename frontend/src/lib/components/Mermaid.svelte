<script lang="ts" module>
	// Lazy-loaded mermaid (same recipe as chat's Markdown.tsx): the package is
	// ~1 MB and only lands when a ```mermaid fence is actually on screen. One
	// module-level promise + a theme marker so re-initialize happens only on a
	// light/dark flip; a counter keeps render ids unique.
	let mermaidPromise: Promise<typeof import('mermaid').default> | null = null;
	let lastTheme: 'default' | 'dark' | null = null;
	let seq = 0;

	const loadMermaid = async (theme: 'default' | 'dark') => {
		mermaidPromise ??= import('mermaid').then((m) => m.default);
		const mermaid = await mermaidPromise;
		if (lastTheme !== theme) {
			// strict: the diagram source is arbitrary file content — scripts and
			// event handlers in the emitted SVG are sandboxed away.
			mermaid.initialize({ startOnLoad: false, theme, securityLevel: 'strict' });
			lastTheme = theme;
		}
		return mermaid;
	};
</script>

<script lang="ts">
	// `fallback` is the DOMPurify-sanitized code block from renderBlocks() —
	// shown while loading and when the diagram fails to parse, so a typo'd
	// fence still reads as code instead of breaking the script.
	let { source, fallback }: { source: string; fallback: string } = $props();

	let svg = $state<string | null>(null);
	let failed = $state(false);
	let dark = $state(matchMedia('(prefers-color-scheme: dark)').matches);

	$effect(() => {
		const mq = matchMedia('(prefers-color-scheme: dark)');
		const onChange = () => (dark = mq.matches);
		mq.addEventListener('change', onChange);
		return () => mq.removeEventListener('change', onChange);
	});

	$effect(() => {
		const src = source;
		const isDark = dark;
		let cancelled = false;
		void (async () => {
			try {
				const mermaid = await loadMermaid(isDark ? 'dark' : 'default');
				const { svg: rendered } = await mermaid.render(`mmd-${++seq}`, src);
				if (cancelled) return;
				svg = rendered;
				failed = false;
			} catch {
				if (cancelled) return;
				svg = null;
				failed = true;
			}
		})();
		return () => {
			cancelled = true;
		};
	});
</script>

{#if svg}
	<!-- eslint-disable-next-line svelte/no-at-html-tags -- mermaid output with securityLevel strict (scripts/handlers sandboxed) -->
	<div class="diagram">{@html svg}</div>
{:else}
	<div class="fallback">
		<!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify in renderBlocks() -->
		{@html fallback}
		<span class="tag" class:failed>{failed ? 'mermaid error' : 'mermaid'}</span>
	</div>
{/if}

<style>
	.diagram {
		margin: 0.6em 0;
		padding: 0.85em 1em;
		background: var(--halo-bg-light);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius);
		overflow-x: auto;
	}
	.diagram :global(svg) {
		max-width: 100%;
		height: auto;
		display: block;
		margin: 0 auto;
	}
	.fallback {
		position: relative;
	}
	.tag {
		position: absolute;
		top: 0.4em;
		right: 0.6em;
		font-family: var(--halo-font-heading);
		font-size: 0.65rem;
		color: var(--halo-text-muted);
		background: var(--halo-bg-main);
		border: 1px solid var(--halo-border);
		border-radius: var(--halo-radius-pill);
		padding: 0.05rem 0.4rem;
	}
	.tag.failed {
		color: var(--halo-error);
		border-color: var(--halo-error);
	}
</style>
