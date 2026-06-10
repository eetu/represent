// Thin fetch layer over the backend's JSON API. Types are hand-written to match
// the Rust structs (no codegen — see sibling-app). Keep them in sync with
// backend/src/{store,routes}.rs.
//
// Offline ("demo-effect") layer: every successful GET is mirrored into
// localStorage; when the backend is unreachable the cached copy is served and
// `netState.offline` flips so the UI can say so. Saves made while offline are
// queued (latest edit per file wins) and flushed when connectivity returns.
// The service worker covers the app shell; this layer covers the content.

import { netState } from './offline.svelte';

export type StatusResponse = {
	service: string;
	version: string;
	data_dir_healthy: boolean;
	project_count: number | null;
};

export type ProjectInfo = {
	name: string;
	file_count: number;
	updated_at: string | null;
};

export type FileInfo = {
	name: string;
	size: number;
	modified: string | null;
};

export type FileContent = {
	name: string;
	content: string;
};

/** Thrown for any non-2xx response; carries the HTTP status. */
export class ApiError extends Error {
	status: number;
	constructor(status: number, message: string) {
		super(message);
		this.status = status;
		this.name = 'ApiError';
	}
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
	const res = await fetch(path, {
		headers: {
			accept: 'application/json',
			...(init?.body ? { 'content-type': 'application/json' } : {})
		},
		...init
	});
	if (!res.ok) {
		let detail = res.statusText;
		try {
			const body = await res.json();
			if (body && typeof body.detail === 'string') detail = body.detail;
		} catch {
			// non-JSON error body — keep statusText
		}
		throw new ApiError(res.status, detail);
	}
	if (res.status === 204) return undefined as T;
	return (await res.json()) as T;
}

// ---------- offline cache + pending-edit queue (localStorage) ----------

const CACHE_PREFIX = 'represent:cache:';
const PENDING_KEY = 'represent:pending';

type PendingEdit = { project: string; file: string; content: string };

const readPending = (): PendingEdit[] => {
	try {
		return JSON.parse(localStorage.getItem(PENDING_KEY) ?? '[]') as PendingEdit[];
	} catch {
		return [];
	}
};

const writePending = (edits: PendingEdit[]) => {
	localStorage.setItem(PENDING_KEY, JSON.stringify(edits));
	netState.setPending(edits.length);
};

const cacheSet = (path: string, value: unknown) => {
	try {
		localStorage.setItem(CACHE_PREFIX + path, JSON.stringify(value));
	} catch {
		// quota — the demo copy is best-effort
	}
};

/** GET with offline fallback: network first, cached copy when unreachable. */
async function cachedGet<T>(path: string): Promise<T> {
	try {
		const value = await request<T>(path);
		cacheSet(path, value);
		netState.setOffline(false);
		void flushPending();
		return value;
	} catch (e) {
		// ApiError = the server answered (a real 4xx/5xx) — not an offline case.
		if (e instanceof ApiError) throw e;
		const hit = localStorage.getItem(CACHE_PREFIX + path);
		if (hit === null) throw e;
		netState.setOffline(true);
		return JSON.parse(hit) as T;
	}
}

/** Re-send queued offline edits; latest edit per file already collapsed. */
export async function flushPending(): Promise<void> {
	const pending = readPending();
	if (pending.length === 0) return;
	const remaining: PendingEdit[] = [];
	for (const edit of pending) {
		try {
			await request<void>(fileBase(edit.project, edit.file), {
				method: 'PUT',
				body: JSON.stringify({ content: edit.content })
			});
		} catch (e) {
			// Still offline → keep and retry later. A server-side rejection
			// (4xx, e.g. the project was deleted) is dropped — re-PUTting it
			// forever would never succeed.
			if (!(e instanceof ApiError)) remaining.push(edit);
		}
	}
	writePending(remaining);
	if (remaining.length === 0) netState.setOffline(false);
}

/** Call once at app start: restore the queue count + flush on reconnect. */
export function initOffline(): () => void {
	netState.setPending(readPending().length);
	const onOnline = () => void flushPending();
	window.addEventListener('online', onOnline);
	void flushPending();
	return () => window.removeEventListener('online', onOnline);
}

const fileBase = (project: string, file: string) =>
	`/api/projects/${encodeURIComponent(project)}/files/${encodeURIComponent(file)}`;

export const api = {
	status: () => request<StatusResponse>('/status'),
	projects: () => cachedGet<{ projects: ProjectInfo[] }>('/api/projects'),
	createProject: (name: string) =>
		request<{ name: string }>('/api/projects', {
			method: 'POST',
			body: JSON.stringify({ name })
		}),
	deleteProject: (name: string) =>
		request<void>(`/api/projects/${encodeURIComponent(name)}`, {
			method: 'DELETE'
		}),
	files: (project: string) =>
		cachedGet<{ files: FileInfo[] }>(`/api/projects/${encodeURIComponent(project)}/files`),
	readFile: (project: string, file: string) => cachedGet<FileContent>(fileBase(project, file)),
	/**
	 * Upsert. Offline, the edit is applied to the local copy and queued —
	 * the file stays editable at the table even with the backend gone.
	 */
	saveFile: async (project: string, file: string, content: string): Promise<void> => {
		try {
			await request<void>(fileBase(project, file), {
				method: 'PUT',
				body: JSON.stringify({ content })
			});
			cacheSet(fileBase(project, file), { name: file, content });
			netState.setOffline(false);
		} catch (e) {
			if (e instanceof ApiError) throw e;
			cacheSet(fileBase(project, file), { name: file, content });
			const pending = readPending().filter((p) => p.project !== project || p.file !== file);
			pending.push({ project, file, content });
			writePending(pending);
			netState.setOffline(true);
		}
	},
	deleteFile: (project: string, file: string) =>
		request<void>(fileBase(project, file), { method: 'DELETE' }),
	/** Rename keeps the file's demo-order slot; 409 if the target exists. */
	renameFile: (project: string, from: string, to: string) =>
		request<void>(`${fileBase(project, from)}/rename`, {
			method: 'POST',
			body: JSON.stringify({ to })
		}),
	/** New full ordering — stored server-side as metadata; names untouched. */
	reorder: (project: string, files: string[]) =>
		request<{ files: FileInfo[] }>(`/api/projects/${encodeURIComponent(project)}/reorder`, {
			method: 'POST',
			body: JSON.stringify({ files })
		}),
	/** Plain link target — the browser handles the download itself. */
	bundleUrl: (project: string) => `/api/projects/${encodeURIComponent(project)}/bundle`
};
