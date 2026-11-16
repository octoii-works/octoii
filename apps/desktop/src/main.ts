/** A desktop window onto the node's existing interface. The node lives independently of the window. */
import { app, BrowserWindow, dialog, Menu, shell } from "electron";
import { homedir } from "node:os";
import { join } from "node:path";
import { LocalNode, localAddress } from "./local-node.ts";
import { pairLocalNode } from "./pairing.ts";

app.setName("Octoii");

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
		let origin: string | undefined;
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
			if (new URL(url).origin === origin) return;
			event.preventDefault();
			openExternal(url);
		});
		window.webContents.on("will-redirect", (event, url) => {
			if (new URL(url).origin !== origin) event.preventDefault();
		});

		async function load(): Promise<void> {
			try {
				window.setTitle("Octoii — Connecting…");
				const address = initial ?? (await node.connect());
				if (window.isDestroyed()) return;
				origin = address.origin;
				if (initial === undefined) await pairLocalNode(node, address, window.webContents.session);
				if (window.isDestroyed()) return;
				await window.loadURL(address.href);
				window.setTitle("Octoii");
			} catch (error) {
				if (window.isDestroyed()) return;
				const { response } = await dialog.showMessageBox(window, {
					type: "error",
					message: "Cannot connect to Octoii",
					detail: initial === undefined && error instanceof Error ? error.message : "Could not load the selected node. Check that it is running, then retry.",
					buttons: ["Retry", "Close"],
					defaultId: 0,
					cancelId: 1,
				});
				if (window.isDestroyed()) return;
				if (response === 0) await load();
				else window.close();
			}
		}
		void load().catch(() => dialog.showErrorBox("Octoii", "Could not load the desktop window."));
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
