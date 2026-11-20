/** The desktop renders the node's pages in its own Chromium and relays their DevTools messages. */
import { ipcMain, session, type IpcMainEvent, type IpcMainInvokeEvent, type WebContents, type WebPreferences } from "electron";
import { randomUUID } from "node:crypto";
import { once } from "node:events";
import { WebSocket } from "ws";

type Page = {
	readonly page: string;
	readonly thread: string;
	readonly partition: string;
	readonly width: number;
	readonly height: number;
	readonly ready: PromiseWithResolvers<void>;
	guest?: WebContents;
};

type Host = { readonly origin: string; readonly socket: string };
type Packet = Record<string, unknown>;

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

/** Applies page-scoped DevTools commands to exactly the guest identified by its session. */
async function command(target: WebContents, method: string, params: Packet, child: string | undefined): Promise<Packet> {
	if (method === "Target.getTargetInfo") return object(await target.debugger.sendCommand(method, {}, child));
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
	readonly #contents: WebContents;
	readonly #send: (packet: Packet) => void;

	constructor(contents: WebContents, send: (packet: Packet) => void) {
		this.#contents = contents;
		this.#send = send;
	}

	publish(): void {
		if (this.#contents.isDestroyed()) return;
		this.#contents.send(
			"octoii:browser-pages",
			[...this.#pages.values()].map(({ page, thread, partition, width, height }) => ({ page, thread, partition, width, height })),
		);
	}

	allows(partition: string | undefined): boolean {
		return [...this.#pages.values()].some((page) => page.partition === partition && page.guest === undefined);
	}

	close(id: string): void {
		const page = this.#pages.get(id);
		if (page === undefined) return;
		this.#pages.delete(id);
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
		const page = [...this.#pages.values()].find((each) => each.guest === undefined && guest.session === session.fromPartition(each.partition));
		if (!page) {
			guest.close();
			return;
		}
		page.guest = guest;
		guest.setWindowOpenHandler(() => ({ action: "deny" }));
		guest.session.setPermissionRequestHandler((_page, _permission, reply) => reply(false));
		guest.session.setPermissionCheckHandler(() => false);
		guest.session.on("will-download", (event) => event.preventDefault());
		guest.on("will-frame-navigate", (event) => {
			if (!allowable(event.url)) event.preventDefault();
		});
		guest.on("will-redirect", (event, url) => {
			if (!allowable(url)) event.preventDefault();
		});
		const ended = (): void => {
			if (!this.#pages.has(page.page)) return;
			this.close(page.page);
			this.#send({ method: "Octoii.closed", params: { page: page.page } });
		};
		guest.once("destroyed", ended);
		guest.once("render-process-gone", ended);
		guest.on("devtools-opened", () => this.#send({ method: "Octoii.devtools", params: { page: page.page, open: true } }));
		guest.on("devtools-closed", () => this.#send({ method: "Octoii.devtools", params: { page: page.page, open: false } }));
		guest.debugger.on("detach", ended);
		guest.debugger.on("message", (_event, method, value: unknown, child) => {
			const params = object(value);
			if (method.startsWith("Target.") && typeof params["sessionId"] === "string") {
				params["sessionId"] = `${page.page}/${params["sessionId"]}`;
			}
			this.#send({ method, params, sessionId: child ? `${page.page}/${child}` : page.page });
		});
		try {
			guest.debugger.attach();
			page.ready.resolve();
		} catch (error) {
			page.ready.reject(error);
			ended();
		}
	}

	async answer(message: unknown): Promise<Packet | undefined> {
		const { id, method, params: value, sessionId } = object(message);
		if (typeof id !== "number" || typeof method !== "string") return;
		const params = object(value);
		try {
			if (typeof sessionId === "string") {
				const [page, child] = sessionId.split("/", 2);
				const guest = page === undefined ? undefined : this.#pages.get(page)?.guest;
				if (!guest || guest.isDestroyed()) throw new Error("The desktop page is gone.");
				return { id, result: await command(guest, method, params, child) };
			}
			const page = params["page"];
			if (typeof page !== "string") throw new Error("The desktop page identity is missing.");
			if (method === "Octoii.close") this.close(page);
			else if (method === "Octoii.close_devtools") await this.devTools(page, "close");
			else if (method === "Octoii.open") {
				const { thread, width, height } = params;
				if (this.#pages.has(page) || typeof thread !== "string" || typeof width !== "number" || typeof height !== "number") {
					throw new Error("Invalid desktop page.");
				}
				const ready = Promise.withResolvers<void>();
				this.#pages.set(page, { page, thread, partition: `octoii-browser-${randomUUID()}`, width, height, ready });
				this.publish();
				await ready.promise;
			} else throw new Error("Unknown desktop browser command.");
			return { id, result: {} };
		} catch (error) {
			return { id, error: { code: -32000, message: error instanceof Error ? error.message : "Desktop browser command failed." } };
		}
	}
}

export function serveBrowser(contents: WebContents, host: () => Host | undefined): void {
	let socket: WebSocket | undefined;
	let workspace: string | null = null;
	let retry: ReturnType<typeof setTimeout> | undefined;
	const send = (message: Packet): void => {
		if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
	};
	const pages = new DesktopPages(contents, send);
	const allowed = (event: IpcMainEvent | IpcMainInvokeEvent): boolean => {
		const target = host();
		return (
			target !== undefined &&
			event.sender === contents &&
			event.senderFrame === contents.mainFrame &&
			URL.parse(event.senderFrame.url)?.origin === target.origin
		);
	};
	const connect = (target: Host): void => {
		if (socket) {
			pages.publish();
			return;
		}
		const link = new WebSocket(`ws+unix://${target.socket}:/browser`, "octoii.desktop-browser.1");
		socket = link;
		link.on("open", () => send({ method: "Octoii.workspace", params: { thread: workspace } }));
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
	const showing = (event: IpcMainEvent, thread: unknown): void => {
		if (!allowed(event) || (thread !== null && (typeof thread !== "string" || thread.length > 128))) return;
		workspace = thread;
		send({ method: "Octoii.workspace", params: { thread: workspace } });
	};
	const disconnected = (): void => {
		const link = socket;
		socket = undefined;
		clearTimeout(retry);
		workspace = null;
		link?.terminate();
		pages.closeAll();
	};
	contents.on("will-attach-webview", (event, preferences: WebPreferences, params) => {
		const target = host();
		if (!target || URL.parse(contents.getURL())?.origin !== target.origin || params["src"] !== "about:blank" || !pages.allows(params["partition"])) {
			event.preventDefault();
			return;
		}
		delete preferences.preload;
		Object.assign(preferences, {
			nodeIntegration: false,
			nodeIntegrationInSubFrames: false,
			sandbox: true,
			contextIsolation: true,
			webSecurity: true,
			allowRunningInsecureContent: false,
			webviewTag: false,
			backgroundThrottling: false,
		});
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
	});
}
