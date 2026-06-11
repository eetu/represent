<script lang="ts">
	import '$lib/styles/halo.css';

	import { dev } from '$app/environment';
	import { initOffline } from '$lib/api';
	import OfflineBadge from '$lib/components/OfflineBadge.svelte';

	// Deliberately chrome-less: the viewer route goes full-screen in demo mode,
	// so each page owns its own header instead of a shared shell.
	let { children } = $props();

	// Offline layer: restore the queued-edit count and flush it whenever
	// connectivity returns.
	$effect(() => initOffline());

	// Prod-only SW registration (auto-registration is off in svelte.config.js):
	// a dev-registered SW persists on the localhost origin and breaks other
	// apps that later run on the same port.
	$effect(() => {
		if (!dev && 'serviceWorker' in navigator) {
			void navigator.serviceWorker.register('/service-worker.js');
		}
	});
</script>

<svelte:head>
	<title>represent</title>
</svelte:head>

{@render children()}

<OfflineBadge />
