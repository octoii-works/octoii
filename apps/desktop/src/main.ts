/** A desktop window onto the node's existing interface. The node lives independently of the window. */
import { app, BrowserWindow, dialog, Menu, shell } from "electron";

app.setName("Octoii");

/** The initial address may include a one-use pairing link; never log it or keep it for retries. */
function address(): URL {
	const url = new URL(process.env["OCTOII_DESKTOP_URL"] ?? "http://127.0.0.1:7600");
	if (url.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) || url.username || url.password) {
		throw new Error("OCTOII_DESKTOP_URL must be a local Octoii HTTP address.");
	}
	return url;
}

/** External pages use the system browser, which has no access to the desktop window. */
function openExternal(address: string): void {
	const url = new URL(address);
	if (url.protocol !== "https:" && url.protocol !== "http:") return;
	void shell.openExternal(url.href).catch(() => dialog.showErrorBox("Octoii", "Could not open the link in your browser."));
}

async function start(): Promise<void> {
	const initial = address();
	const origin = initial.origin;
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

	function createWindow(url: string): void {
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

		async function load(url: string): Promise<void> {
			try {
				await window.loadURL(url);
			} catch {
				if (window.isDestroyed()) return;
				const { response } = await dialog.showMessageBox(window, {
					type: "error",
					message: "Cannot connect to Octoii",
					detail: `Start your Octoii node at ${origin}, then retry. Closing this window does not stop the node.`,
					buttons: ["Retry", "Close"],
					defaultId: 0,
					cancelId: 1,
				});
				if (window.isDestroyed()) return;
				if (response === 0) await load(origin);
				else window.close();
			}
		}
		void load(url).catch(() => dialog.showErrorBox("Octoii", "Could not load the desktop window."));
	}

	createWindow(initial.href);
	app.on("activate", () => {
		if (BrowserWindow.getAllWindows().length === 0) createWindow(origin);
	});
}

if (app.requestSingleInstanceLock()) {
	app.on("second-instance", () => {
		const window = BrowserWindow.getAllWindows()[0];
		if (!window) return;
		if (window.isMinimized()) window.restore();
		window.show();
		window.focus();
	});
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
