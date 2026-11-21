/** The desktop renders the node's pages in its own Chromium and relays their DevTools messages. */
import {
	type BrowserWindow,
	type DownloadItem,
	type Event,
	ipcMain,
	session,
	type IpcMainEvent,
	type IpcMainInvokeEvent,
	type Session,
	type WebContents,
	type WebPreferences,
} from "electron";
import { createHash, randomUUID } from "node:crypto";
import { once } from "node:events";
import { isAbsolute, join } from "node:path";
import { WebSocket } from "ws";
import type { BrowserHost } from "./local-node.ts";

type Page = {
	readonly page: string;
	readonly thread: string;
	readonly partition: string;
	readonly start: string;
	readonly width: number;
	readonly height: number;
	readonly ready: PromiseWithResolvers<void>;
	readonly popups: Set<BrowserWindow>;
	readonly downloading: Set<DownloadItem>;
	readonly downloadTo: string | undefined;
	local: boolean;
	guest?: WebContents;
};

type Packet = Record<string, unknown>;
const GUEST_PREFERENCES = {
	nodeIntegration: false,
	nodeIntegrationInSubFrames: false,
	sandbox: true,
	contextIsolation: true,
	webSecurity: true,
	allowRunningInsecureContent: false,
	webviewTag: false,
	backgroundThrottling: false,
} satisfies WebPreferences;

function object(value: unknown): Packet {
	if (typeof value !== "object" || value === null || Array.isArray(value)) return {};
	// SAFETY: a non-null record can be inspected by key while each value remains unknown.
	return value as Packet;
}

function allowable(address: string): boolean {
	if (address === "about:blank" || address === "about:srcdoc") return true;
	try {
		const scheme = new URL(address).protocol;
		return scheme === "http:" || scheme === "https:" || scheme === "data:" || scheme === "blob:";
	} catch {
		return false;
	}
}

function popupAddress(address: string): boolean {
	const url = URL.parse(address);
	return address === "about:blank" || url?.protocol === "http:" || url?.protocol === "https:";
}

function guardGuest(guest: WebContents): void {
	guest.on("will-frame-navigate", (event) => {
		if (!allowable(event.url)) event.preventDefault();
	});
	guest.on("will-redirect", (event, url) => {
		if (!allowable(url)) event.preventDefault();
	});
}

/** Applies page-scoped DevTools commands to exactly the guest identified by its session. */
async function command(target: WebContents, method: string, params: Packet, child: string | undefined): Promise<Packet> {
	if (method === "Target.getTargetInfo") return object(await target.debugger.sendCommand(method, {}, child));
	if (method === "Page.reload" && child === undefined) {
		// Chromium's Page.reload can reload the embedder of a guest. Reload the guest's own WebContents.
		target.reload();
		return {};
	}
	if (method === "Target.setAutoAttach") {
		return object(
			await target.debugger.sendCommand(method, { autoAttach: true, waitForDebuggerOnStart: true, flatten: true, filter: [{ type: "iframe" }] }, child),
		);
	}
	if (
		!/^(Accessibility|CSS|DOM|DOMSnapshot|Emulation|Input|Log|Network|Overlay|Page|Runtime)\./.test(method) ||
		method === "DOM.setFileInputFiles" ||
		method === "Input.dispatchDragEvent" ||
		(method === "Page.navigate" && (typeof params["url"] !== "string" || !allowable(params["url"])))
	) {
		throw new Error(`This command is unavailable in the desktop page: ${method}.`);
	}
	return object(await target.debugger.sendCommand(method, params, child));
}

class DesktopPages {
	readonly #pages = new Map<string, Page>();
	readonly #window: BrowserWindow;
	readonly #contents: WebContents;
	readonly #send: (packet: Packet) => void;
	#profile: { readonly partition: string; readonly storage: Session } | undefined;
	readonly #onDownload = (event: Event, item: DownloadItem, source: WebContents): void => this.download(event, item, source);

	constructor(window: BrowserWindow, send: (packet: Packet) => void) {
		this.#window = window;
		this.#contents = window.webContents;
		this.#send = send;
	}

	bindProfile(node: string): void {
		const partition = `persist:octoii-browser-${createHash("sha256").update(node).digest("hex")}`;
		if (this.#profile?.partition === partition) return;
		this.releaseProfile();
		const storage = session.fromPartition(partition);
		storage.setPermissionRequestHandler((_page, _permission, reply) => reply(false));
		storage.setPermissionCheckHandler(() => false);
		storage.on("will-download", this.#onDownload);
		this.#profile = { partition, storage };
	}

	releaseProfile(): void {
		this.#profile?.storage.off("will-download", this.#onDownload);
		this.#profile = undefined;
	}

	publish(): void {
		if (this.#contents.isDestroyed()) return;
		this.#contents.send(
			"octoii:browser-pages",
			[...this.#pages.values()].map(({ page, thread, partition, start, width, height }) => ({ page, thread, partition, start, width, height })),
		);
	}

	allows(partition: string | undefined, start: string | undefined): boolean {
		return [...this.#pages.values()].some((page) => page.partition === partition && page.start === start && page.guest === undefined);
	}

	control(id: string, local: boolean): void {
		const page = this.#pages.get(id);
		if (!page) return;
		page.local = local;
		if (!local) for (const popup of page.popups) if (!popup.isDestroyed()) popup.destroy();
	}

	close(id: string): void {
		const page = this.#pages.get(id);
		if (page === undefined) return;
		this.#pages.delete(id);
		for (const popup of page.popups) if (!popup.isDestroyed()) popup.destroy();
		for (const item of page.downloading) item.cancel();
		page.ready.reject(new Error("The desktop page closed."));
		if (page.guest && !page.guest.isDestroyed()) page.guest.close();
		this.publish();
	}

	closeAll(): void {
		for (const page of this.#pages.keys()) this.close(page);
	}

	async devTools(id: string, action: "open" | "close"): Promise<void> {
		const guest = this.#pages.get(id)?.guest;
		if (!guest || guest.isDestroyed()) throw new Error("The desktop page is gone.");
		if (guest.isDevToolsOpened() === (action === "open")) return;
		const changed = once(guest, action === "open" ? "devtools-opened" : "devtools-closed", { signal: AbortSignal.timeout(4000) });
		if (action === "open") guest.openDevTools({ mode: "detach", title: "Octoii Developer Tools" });
		else guest.closeDevTools();
		await changed;
	}

	attach(guest: WebContents): void {
		guest.once("dom-ready", () => {
			void this.bindGuest(guest).catch(() => {
				if (!guest.isDestroyed()) guest.close();
			});
		});
	}

	async bindGuest(guest: WebContents): Promise<void> {
		const start = guest.getURL();
		const page = [...this.#pages.values()].find((each) => each.guest === undefined && each.start === start && guest.session === this.#profile?.storage);
		if (!page) {
			guest.close();
			return;
		}
		page.guest = guest;
		guardGuest(guest);
		this.popups(page, guest);
		const ended = (): void => {
			if (this.#pages.get(page.page) !== page) return;
			this.close(page.page);
			this.#send({ method: "Octoii.closed", params: { page: page.page } });
		};
		guest.once("destroyed", ended);
		guest.once("render-process-gone", ended);
		const devTools = (open: boolean): void => {
			if (this.#pages.get(page.page) === page) this.#send({ method: "Octoii.devtools", params: { page: page.page, open } });
		};
		guest.on("devtools-opened", () => devTools(true));
		guest.on("devtools-closed", () => devTools(false));
		guest.debugger.on("detach", ended);
		guest.debugger.on("message", (_event, method, value: unknown, child) => {
			if (this.#pages.get(page.page) !== page) return;
			const params = object(value);
			if (method.startsWith("Target.") && typeof params["sessionId"] === "string") {
				params["sessionId"] = `${page.page}/${params["sessionId"]}`;
			}
			this.#send({ method, params, sessionId: child ? `${page.page}/${child}` : page.page });
		});
		try {
			// The bootstrap address only binds the guest. The page starts at a clean blank entry.
			await guest.loadURL("about:blank");
			guest.navigationHistory.clear();
			guest.debugger.attach();
			page.ready.resolve();
		} catch (error) {
			page.ready.reject(error);
			ended();
		}
	}

	popups(page: Page, guest: WebContents): void {
		guest.setWindowOpenHandler(({ url }) => {
			if (!page.local || !popupAddress(url)) return { action: "deny" };
			return {
				action: "allow",
				overrideBrowserWindowOptions: {
					parent: this.#window,
					title: "Octoii Browser",
					webPreferences: { ...GUEST_PREFERENCES, session: guest.session },
				},
			};
		});
		guest.on("did-create-window", (popup) => {
			if (!page.local || this.#pages.get(page.page) !== page) {
				popup.destroy();
				return;
			}
			page.popups.add(popup);
			guardGuest(popup.webContents);
			popup.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
			popup.once("closed", () => page.popups.delete(popup));
		});
	}

	download(event: Event, item: DownloadItem, source: WebContents): void {
		const page = [...this.#pages.values()].find((each) => each.guest === source || [...each.popups].some((popup) => popup.webContents === source));
		if (!page?.downloadTo) {
			event.preventDefault();
			return;
		}
		const id = randomUUID();
		item.setSavePath(join(page.downloadTo, id));
		page.downloading.add(item);
		const report = (state: string): void => {
			if (this.#pages.get(page.page) !== page) return;
			this.#send({
				method: "Octoii.download",
				params: {
					page: page.page,
					id,
					state,
					url: item.getURL(),
					name: item.getFilename(),
					received_bytes: item.getReceivedBytes(),
					total_bytes: item.getTotalBytes(),
				},
			});
		};
		report("inProgress");
		item.once("done", (_event, state) => {
			page.downloading.delete(item);
			report(state === "completed" ? "completed" : "canceled");
		});
	}

	target(sessionId: string): { readonly guest: WebContents; readonly child: string | undefined } {
		const [page, child] = sessionId.split("/", 2);
		const guest = page === undefined ? undefined : this.#pages.get(page)?.guest;
		if (!guest || guest.isDestroyed()) throw new Error("The desktop page is gone.");
		return { guest, child };
	}

	async answer(message: unknown): Promise<Packet | undefined> {
		const { id, method, params: value, sessionId } = object(message);
		if (typeof id !== "number" || typeof method !== "string") return;
		const params = object(value);
		try {
			if (typeof sessionId === "string") {
				const { guest, child } = this.target(sessionId);
				return { id, result: await command(guest, method, params, child) };
			}
			const page = params["page"];
			if (typeof page !== "string") throw new Error("The desktop page identity is missing.");
			if (method === "Octoii.close") this.close(page);
			else if (method === "Octoii.control") this.control(page, params["local"] === true);
			else if (method === "Octoii.close_devtools") await this.devTools(page, "close");
			else if (method === "Octoii.upload") {
				const target = params["target"];
				if (typeof target !== "string" || target.split("/", 1)[0] !== page) throw new Error("The upload target is gone.");
				const { guest, child } = this.target(target);
				await guest.debugger.sendCommand("DOM.setFileInputFiles", object(params["fields"]), child);
			} else if (method === "Octoii.open") {
				const { thread, width, height, downloads } = params;
				const profile = this.#profile;
				if (!profile || this.#pages.has(page) || typeof thread !== "string" || typeof width !== "number" || typeof height !== "number") {
					throw new Error("Invalid desktop page.");
				}
				const ready = Promise.withResolvers<void>();
				this.#pages.set(page, {
					page,
					thread,
					partition: profile.partition,
					start: `about:blank#octoii-${randomUUID()}`,
					width,
					height,
					ready,
					popups: new Set(),
					downloading: new Set(),
					local: false,
					downloadTo: typeof downloads === "string" && isAbsolute(downloads) ? downloads : undefined,
				});
				this.publish();
				await ready.promise;
			} else throw new Error("Unknown desktop browser command.");
			return { id, result: {} };
		} catch (error) {
			return { id, error: { code: -32000, message: error instanceof Error ? error.message : "Desktop browser command failed." } };
		}
	}
}

export function serveBrowser(window: BrowserWindow, host: () => BrowserHost | undefined): void {
	const contents = window.webContents;
	let socket: WebSocket | undefined;
	let workspace: string | null = null;
	let viewer: string | null = null;
	let retry: ReturnType<typeof setTimeout> | undefined;
	const send = (message: Packet): void => {
		if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
	};
	const pages = new DesktopPages(window, send);
	const announce = (): void => send({ method: "Octoii.workspace", params: { thread: workspace, session: viewer } });
	const allowed = (event: IpcMainEvent | IpcMainInvokeEvent): boolean => {
		const target = host();
		return (
			target !== undefined &&
			event.sender === contents &&
			event.senderFrame === contents.mainFrame &&
			URL.parse(event.senderFrame.url)?.origin === target.origin
		);
	};
	const connect = (target: BrowserHost): void => {
		if (socket) {
			pages.publish();
			return;
		}
		clearTimeout(retry);
		pages.bindProfile(target.node);
		const link = new WebSocket(`ws+unix://${target.socket}:/browser`, "octoii.desktop-browser.1");
		socket = link;
		link.on("open", announce);
		link.on("message", (data: Buffer, binary) => {
			if (binary || socket !== link) return;
			try {
				void pages.answer(JSON.parse(data.toString())).then((answer) => {
					if (answer && socket === link) send(answer);
				});
			} catch {
				link.terminate();
			}
		});
		link.on("error", () => link.terminate());
		link.on("close", () => {
			if (socket !== link) return;
			socket = undefined;
			pages.closeAll();
			retry = setTimeout(() => connect(target), 1500);
		});
	};
	const ready = (event: IpcMainEvent): void => {
		const target = host();
		if (target && allowed(event)) connect(target);
	};
	const showing = (event: IpcMainEvent, thread: unknown, session: unknown): void => {
		if (!allowed(event) || (thread !== null && (typeof thread !== "string" || thread.length > 128))) return;
		if (session !== null && (typeof session !== "string" || session.length > 128)) return;
		workspace = thread;
		viewer = session;
		const target = host();
		if (!socket && target && viewer !== null) connect(target);
		announce();
	};
	const disconnected = (): void => {
		const link = socket;
		socket = undefined;
		clearTimeout(retry);
		workspace = null;
		viewer = null;
		link?.terminate();
		pages.closeAll();
	};
	contents.on("will-attach-webview", (event, preferences: WebPreferences, params) => {
		const target = host();
		if (!target || URL.parse(contents.getURL())?.origin !== target.origin || !pages.allows(params["partition"], params["src"])) {
			event.preventDefault();
			return;
		}
		delete preferences.preload;
		Object.assign(preferences, GUEST_PREFERENCES);
	});
	contents.on("did-attach-webview", (_event, guest) => pages.attach(guest));
	contents.on("did-start-navigation", (event) => {
		if (event.isMainFrame && !event.isSameDocument) disconnected();
	});
	contents.on("render-process-gone", disconnected);
	ipcMain.on("octoii:browser-ready", ready);
	ipcMain.on("octoii:browser-thread", showing);
	ipcMain.handle("octoii:browser-devtools", async (event, page: unknown, action: unknown) => {
		if (!allowed(event) || typeof page !== "string" || (action !== "open" && action !== "close")) throw new Error("Developer Tools are unavailable.");
		await pages.devTools(page, action);
	});
	contents.once("destroyed", () => {
		ipcMain.off("octoii:browser-ready", ready);
		ipcMain.off("octoii:browser-thread", showing);
		ipcMain.removeHandler("octoii:browser-devtools");
		disconnected();
		pages.releaseProfile();
	});
}
