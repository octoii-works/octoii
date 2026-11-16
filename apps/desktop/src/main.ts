/** A desktop window onto the node's existing interface. The node lives independently of the window. */
import { app, BrowserWindow, dialog, Menu, shell, protocol, session } from "electron";
import { homedir } from "node:os";
import { join } from "node:path";
import { LocalNode, localAddress } from "./local-node.ts";
import { pairLocalNode } from "./pairing.ts";
import { desktopResponse } from "./assets.ts";
import { STARTUP_URL, connectingPage, failedPage } from "./startup.ts";

app.setName("Octoii");
protocol.registerSchemesAsPrivileged([{ scheme: "octoii-desktop", privileges: { standard: true, secure: true } }]);

/** External pages use the system browser, which has no access to the desktop window. */
function openExternal(address: string): void {
	const url = new URL(address);
	if (url.protocol !== "https:" && url.protocol !== "http:") return;
	void shell.openExternal(url.href).catch(() => dialog.showErrorBox("Octoii", "Could not open the link in your browser."));
}

async function start(): Promise<void> {
	const explicit = process.env["OCTOII_DESKTOP_URL"];
	const initial = explicit === undefined ? undefined : localAddress(explicit);
	const node = new LocalNode(process.env["OCTOII_BIN"] ?? join(homedir(), ".local/bin/octoii"), process.env["OCTOII_HOME"] ?? join(homedir(), ".octoii"));
	await app.whenReady();
	let origin: string | undefined;
	let startup = connectingPage();
	const desktopSession = session.defaultSession;
	desktopSession.protocol.handle("octoii-desktop", (request) => {
		if (request.url !== STARTUP_URL) return new Response(null, { status: 404 });
		return new Response(startup, { headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" } });
	});
	desktopSession.protocol.handle("http", (request) =>
		desktopResponse(request, join(app.getAppPath(), "dist/web"), origin, (forwarded) =>
			desktopSession.fetch(forwarded, { bypassCustomProtocolHandlers: true }),
		),
	);
	Menu.setApplicationMenu(
		Menu.buildFromTemplate([
			...(process.platform === "darwin" ? [{ role: "appMenu" as const }] : []),
			{ role: "fileMenu" },
			{ role: "editMenu" },
			{ role: "viewMenu" },
			{ role: "windowMenu" },
		]),
	);

	function createWindow(): void {
		let connecting = false;
		const window = new BrowserWindow({
			title: "Octoii",
			width: 1280,
			height: 860,
			minWidth: 800,
			minHeight: 560,
			backgroundColor: "#171717",
			webPreferences: { nodeIntegration: false, contextIsolation: true, sandbox: true },
		});
		window.webContents.session.setPermissionRequestHandler((_contents, _permission, reply) => reply(false));
		window.webContents.session.setPermissionCheckHandler(() => false);
		window.webContents.setWindowOpenHandler(({ url }) => {
			openExternal(url);
			return { action: "deny" };
		});
		window.webContents.on("will-navigate", (event, url) => {
			if (window.webContents.getURL() === STARTUP_URL && (url === `${STARTUP_URL}retry` || url === `${STARTUP_URL}close`)) {
				event.preventDefault();
				if (url === `${STARTUP_URL}close`) window.close();
				else connect();
				return;
			}
			if (new URL(url).origin === origin) return;
			event.preventDefault();
			openExternal(url);
		});
		window.webContents.on("will-redirect", (event, url) => {
			if (new URL(url).origin !== origin) event.preventDefault();
		});

		async function load(): Promise<void> {
			if (connecting || window.isDestroyed()) return;
			connecting = true;
			try {
				startup = connectingPage();
				await window.loadURL(STARTUP_URL);
				const address = initial ?? (await node.connect());
				if (window.isDestroyed()) return;
				origin = address.origin;
				if (initial === undefined) await pairLocalNode(node, address, desktopSession);
				if (window.isDestroyed()) return;
				await window.loadURL(address.href);
			} catch (error) {
				if (window.isDestroyed()) return;
				startup = failedPage(
					initial === undefined && error instanceof Error ? error.message : "Could not load the selected node. Check that it is running, then retry.",
				);
				await window.loadURL(STARTUP_URL);
			} finally {
				connecting = false;
			}
		}
		function connect(): void {
			void load().catch(() => dialog.showErrorBox("Octoii", "Could not load the desktop window."));
		}
		connect();
	}

	app.on("second-instance", () => {
		const window = BrowserWindow.getAllWindows()[0];
		if (!window) {
			createWindow();
			return;
		}
		if (window.isMinimized()) window.restore();
		window.show();
		window.focus();
	});
	createWindow();
	app.on("activate", () => {
		if (BrowserWindow.getAllWindows().length === 0) createWindow();
	});
}

if (app.requestSingleInstanceLock()) {
	app.on("window-all-closed", () => {
		if (process.platform !== "darwin") app.quit();
	});
	void start().catch((error: unknown) => {
		dialog.showErrorBox("Octoii", error instanceof Error ? error.message : "Could not start Octoii.");
		app.quit();
	});
} else {
	app.quit();
}
