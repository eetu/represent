<script lang="ts">
	import '$lib/styles/halo.css';

	import { initOffline } from '$lib/api';
	import OfflineBadge from '$lib/components/OfflineBadge.svelte';

	// Deliberately chrome-less: the viewer route goes full-screen in demo mode,
	// so each page owns its own header instead of a shared shell.
	let { children } = $props();

	// Offline layer: restore the queued-edit count and flush it whenever
	// connectivity returns.
	$effect(() => initOffline());
</script>

<svelte:head>
	<title>represent</title>
</svelte:head>

{@render children()}

<OfflineBadge />
