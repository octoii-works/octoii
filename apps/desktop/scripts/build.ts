/** Build the existing Web entry into the desktop payload, alongside the Electron main process. */
import { buildWeb } from "../../web/scripts/build.ts";

const root = new URL("../", import.meta.url);
await buildWeb(new URL("dist/web", root).pathname);
const result = await Bun.build({
	entrypoints: [new URL("src/main.ts", root).pathname],
	target: "node",
	external: ["electron"],
	outdir: new URL("dist", root).pathname,
});
if (!result.success) throw new AggregateError(result.logs, "The desktop did not build");
const preload = await Bun.build({
	entrypoints: [new URL("src/browser-preload.ts", root).pathname],
	target: "node",
	format: "cjs",
	external: ["electron"],
	outdir: new URL("dist", root).pathname,
	naming: "browser-preload.cjs",
});
if (!preload.success) throw new AggregateError(preload.logs, "The desktop browser preload did not build");
console.log("Built desktop main process and bundled Web interface");
