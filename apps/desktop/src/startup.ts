/** An entirely local, script-free page for connecting and retrying before the node is available. */
export const STARTUP_URL = "octoii-desktop://startup/";

function escaped(text: string): string {
	return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
}

function page(title: string, message: string, actions: string): string {
	return `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>Octoii</title><style>
:root { color-scheme: light dark; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #fafafa; color: #242424; }
body { margin: 0; min-height: 100vh; display: grid; place-items: center; }
main { width: min(440px, calc(100vw - 64px)); padding: 48px 0; }
.brand { font-size: 16px; font-weight: 650; letter-spacing: -.5px; margin-bottom: 48px; }
h1 { font-size: 26px; font-weight: 550; letter-spacing: -.7px; margin: 0 0 16px; }
p { font-size: 14px; line-height: 1.7; color: #666; overflow-wrap: anywhere; }
.actions { display: flex; gap: 12px; margin-top: 28px; }
a { display: inline-block; padding: 10px 18px; border: 1px solid #ccc; border-radius: 8px; color: inherit; text-decoration: none; font-size: 14px; }
a.primary { background: #242424; color: #fafafa; border-color: #242424; }
a:focus-visible { outline: 2px solid #6388ff; outline-offset: 4px; }
.note { margin-top: 40px; font-size: 12px; }
@media (prefers-color-scheme: dark) { :root { background: #171717; color: #eee; } p { color: #aaa; } a { border-color: #444; } a.primary { background: #eee; color: #171717; border-color: #eee; } }
</style></head><body><main><div class="brand">Octoii</div><h1>${escaped(title)}</h1><p role="status">${escaped(message)}</p>${actions}<p class="note">Your tasks run independently of this window.</p></main></body></html>`;
}

export function connectingPage(): string {
	return page("Connecting to your node", "Preparing your local Octoii connection…", "");
}

export function failedPage(message: string): string {
	return page("Couldn’t connect to Octoii", message, '<div class="actions"><a class="primary" href="/retry">Retry</a><a href="/close">Close window</a></div>');
}
