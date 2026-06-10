/// <reference types="@sveltejs/kit" />
/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />

// App-shell offline cache. Build assets + static files are cached on install
// (cache-first — they're content-hashed); navigations fall back to the cached
// shell when the backend is unreachable, so the SPA itself opens offline.
// `/api` + `/status` are deliberately NOT cached here — content offline
// fallback lives in the api layer (localStorage), which also owns the
// queued-edit story.

import { build, files, version } from '$service-worker';

const sw = self as unknown as ServiceWorkerGlobalScope;

const CACHE = `represent-${version}`;
// '/' is the SPA shell (the backend serves index.html for it).
const ASSETS = [...build, ...files, '/'];

sw.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(CACHE)
			.then((cache) => cache.addAll(ASSETS))
			.then(() => sw.skipWaiting())
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		caches
			.keys()
			.then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
			.then(() => sw.clients.claim())
	);
});

sw.addEventListener('fetch', (event) => {
	const { request } = event;
	if (request.method !== 'GET') return;
	const url = new URL(request.url);
	if (url.origin !== sw.location.origin) return;
	if (url.pathname.startsWith('/api') || url.pathname === '/status') return;

	event.respondWith(
		(async () => {
			const hit = await caches.match(request);
			if (hit) return hit;
			try {
				return await fetch(request);
			} catch (err) {
				// Offline navigation to a client route → serve the cached shell;
				// the SvelteKit router takes it from there.
				const shell = await caches.match('/');
				if (shell) return shell;
				throw err;
			}
		})()
	);
});
