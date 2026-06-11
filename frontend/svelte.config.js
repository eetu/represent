import adapter from '@sveltejs/adapter-static';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	compilerOptions: {
		// Force runes mode (Svelte 5). Can be removed in Svelte 6.
		runes: ({ filename }) => (filename.split(/[/\\]/).includes('node_modules') ? undefined : true)
	},
	kit: {
		// SvelteKit would otherwise auto-register the service worker in dev too —
		// a dev SW on localhost:5173 outlives the dev server and hijacks every
		// other app later served on the same port (it's scoped to the origin).
		// Registered manually, prod-only, in +layout.svelte.
		serviceWorker: {
			register: false
		},
		// Pure SPA: no server-side logic (no +*.server.ts / +server.ts). The Rust
		// backend embeds this and serves the fallback for every unmatched path, so
		// client-side routing + a hard refresh both work. Output to dist/ to match
		// the family convention (the backend's STATIC_DIR / Dockerfile expect it).
		adapter: adapter({
			pages: 'dist',
			assets: 'dist',
			// Name the SPA fallback index.html (not 200.html): in pure-SPA mode
			// adapter-static emits ONLY the fallback, and the Rust backend serves
			// index.html for "/" and every unmatched path (rust-axum embed model).
			fallback: 'index.html',
			precompress: false,
			strict: true
		})
	}
};

export default config;
