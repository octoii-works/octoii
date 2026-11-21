import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("octoiiBrowser", {
	async devTools(page: string, action: "open" | "close"): Promise<void> {
		await ipcRenderer.invoke("octoii:browser-devtools", page, action);
	},
	thread(thread: string | null, session: string | null): void {
		ipcRenderer.send("octoii:browser-thread", thread, session);
	},
	pages(listener: (pages: unknown) => void): () => void {
		const seen = (_event: Electron.IpcRendererEvent, pages: unknown): void => listener(pages);
		ipcRenderer.on("octoii:browser-pages", seen);
		ipcRenderer.send("octoii:browser-ready");
		return () => ipcRenderer.off("octoii:browser-pages", seen);
	},
});
