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

/** API paths remain owned by the node, even when the interface is bundled. */
function isNodeRequest(request: Request, path: string): boolean {
	return (
		path === "/ws" || path === "/machine" || path.startsWith("/threads/") || path.startsWith("/sign-in/") || (path === "/pair" && request.method === "POST")
	);
}

export async function desktopResponse(
	request: Request,
	directory: string,
	origin: string | undefined,
	forward: (request: Request) => Promise<Response>,
): Promise<Response> {
	const url = new URL(request.url);
	if (url.origin !== origin || isNodeRequest(request, url.pathname)) return forward(request);
	if (request.method !== "GET" && request.method !== "HEAD") return new Response(null, { status: 405 });
	let name: string;
	try {
		name = url.pathname === "/" || url.pathname === "/pair" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
	} catch {
		return new Response(null, { status: 400 });
	}
	// Bun emits this entry's assets in one directory. Never resolve a URL into an arbitrary filesystem path.
	if (!name || name.startsWith(".") || /[/\\\0]/.test(name)) return new Response(null, { status: 404 });
	const contentType = TYPES[extname(name)];
	if (!contentType) return new Response(null, { status: 404 });
	try {
		const bytes = await readFile(join(directory, name));
		return new Response(request.method === "HEAD" ? null : bytes, {
			headers: { "Content-Type": contentType, "Cache-Control": "no-cache", "X-Content-Type-Options": "nosniff" },
		});
	} catch (error) {
		if (error instanceof Error && "code" in error && error.code === "ENOENT") return new Response(null, { status: 404 });
		throw error;
	}
}
