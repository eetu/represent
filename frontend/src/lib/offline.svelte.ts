// Reactive network state, fed by the api layer (see api.ts): `offline` flips
// when a fetch fails and a cached copy is served instead; `pending` counts
// queued edits waiting for the backend to come back.

let offline = $state(false);
let pending = $state(0);

export const netState = {
	get offline() {
		return offline;
	},
	get pending() {
		return pending;
	},
	setOffline(v: boolean) {
		offline = v;
	},
	setPending(n: number) {
		pending = n;
	}
};
