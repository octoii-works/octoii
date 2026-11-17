import type { PendingThread, PendingThreadStore } from "../../core/new-thread.ts";

const DATABASE = "octoii.pending-threads";
const STORE = "submissions";

function open(): Promise<IDBDatabase> {
	return new Promise((resolve, reject) => {
		const request = indexedDB.open(DATABASE, 1);
		request.onupgradeneeded = () => request.result.createObjectStore(STORE);
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
		request.onblocked = () => reject(new Error("Close other Octoii tabs to open saved tasks."));
	});
}

/** IndexedDB keeps attachment Blobs alongside the immutable submission, scoped to its paired session. */
export function pendingThreadStore(session: string, project: string | undefined): PendingThreadStore {
	const key = JSON.stringify([session, project ?? null]);
	async function transaction<T>(mode: IDBTransactionMode, action: (store: IDBObjectStore, result: (value: T) => void) => void): Promise<T> {
		const database = await open();
		try {
			return await new Promise<T>((resolve, reject) => {
				const tx = database.transaction(STORE, mode);
				let result: T;
				tx.oncomplete = () => resolve(result);
				tx.onabort = () => reject(tx.error ?? new Error("Could not save the pending task."));
				action(tx.objectStore(STORE), (value) => {
					result = value;
				});
			});
		} finally {
			database.close();
		}
	}
	function read(): Promise<PendingThread | undefined> {
		return transaction("readonly", (store, result) => {
			const request = store.get(key);
			// SAFETY: this database is written only with PendingThread records by this module.
			request.onsuccess = () => result(request.result as PendingThread | undefined);
		});
	}
	return {
		read,
		exclusive: (action) => navigator.locks.request(`${DATABASE}:${key}`, action),
		begin: (pending) =>
			transaction("readwrite", (store, result) => {
				const request = store.get(key);
				request.onsuccess = () => {
					if (request.result !== undefined) {
						store.transaction.abort();
						return;
					}
					store.add(pending, key);
					result(pending);
				};
			}),
		write: (pending) =>
			transaction("readwrite", (store, result) => {
				store.put(pending, key);
				result(undefined);
			}),
		remove: (submitted) =>
			transaction("readwrite", (store, result) => {
				const request = store.get(key);
				request.onsuccess = () => {
					// SAFETY: see read above.
					const pending = request.result as PendingThread | undefined;
					if (pending?.key === submitted) store.delete(key);
					result(undefined);
				};
			}),
	};
}
