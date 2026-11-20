/** Serve bundled interface files at the node's origin, preserving its existing cookie and WebSocket protocol. */
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";

const TYPES: Readonly<Record<string, string>> = {
	".html": "text/html; charset=utf-8",
	".js": "text/javascript; charset=utf-8",
	".css": "text/css; charset=utf-8",
	".svg": "image/svg+xml",
	".png": "image/png",
	".woff2": "font/woff2",
	".woff": "font/woff",
	".wasm": "application/wasm",
};

export async function desktopResponse(
	request: Request,
	directory: string,
	origin: string | undefined,
	forward: (request: Request) => Promise<Response>,
): Promise<Response> {
	const url = new URL(request.url);
	if (url.origin !== origin || (request.method !== "GET" && request.method !== "HEAD")) return forward(request);
	let name: string;
	try {
		name = url.pathname === "/" || url.pathname === "/pair" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
	} catch {
		return forward(request);
	}
	// Bun emits this entry's assets in one directory. Never resolve a URL into an arbitrary filesystem path.
	if (!name || name.startsWith(".") || /[/\\\0]/.test(name)) return forward(request);
	const contentType = TYPES[extname(name)];
	if (!contentType) return forward(request);
	try {
		const bytes = await readFile(join(directory, name));
		// Only the desktop response reserves native window controls; browser assets stay unchanged.
		const body = name === "index.html" && process.platform === "darwin" ? bytes.toString("utf8").replace("<html", '<html data-desktop="mac"') : bytes;
		return new Response(request.method === "HEAD" ? null : body, {
			headers: { "Content-Type": contentType, "Cache-Control": "no-cache", "X-Content-Type-Options": "nosniff" },
		});
	} catch (error) {
		if (error instanceof Error && "code" in error && error.code === "ENOENT") return forward(request);
		throw error;
	}
}
