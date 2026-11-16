import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { LastLocation } from "../src/location.ts";

let folder: string;
let file: string;
beforeEach(() => {
	folder = mkdtempSync(join(tmpdir(), "octoii-location-"));
	file = join(folder, "last-location.json");
});
afterEach(() => rmSync(folder, { recursive: true, force: true }));
const origin = "http://127.0.0.1:7600";

test("restores the same Home's thread across process restarts and port changes", () => {
	new LastLocation(file).remember(new URL(`${origin}/#/thread/test-thread`), origin, "home-a");
	expect(new LastLocation(file).restore(new URL("http://127.0.0.1:8000"), "home-a").href).toBe("http://127.0.0.1:8000/#/thread/test-thread");
	expect(new LastLocation(file).restore(new URL(origin), "home-b").hash).toBe("");
});

test("an explicit new-thread navigation replaces the remembered thread", () => {
	const location = new LastLocation(file);
	location.remember(new URL(`${origin}/#/thread/old`), origin, "home");
	location.remember(new URL(origin), origin, "home");
	expect(location.restore(new URL(origin), "home").hash).toBe("#/");
});

test("never saves pairing credentials, startup pages or external URLs", () => {
	const location = new LastLocation(file);
	location.remember(new URL(`${origin}/#/thread/kept`), origin, "home");
	location.remember(new URL(`${origin}/pair#secret`), origin, "home");
	location.remember(new URL("octoii-desktop://startup/"), origin, "home");
	location.remember(new URL("https://example.com/#secret"), origin, "home");
	expect(readFileSync(file, "utf8")).not.toContain("secret");
	expect(location.restore(new URL(origin), "home").hash).toBe("#/thread/kept");
	expect(location.restore(new URL(`${origin}/pair#new-secret`), "home").hash).toBe("#new-secret");
});

test("damaged persisted navigation does not block startup", () => {
	writeFileSync(file, "{");
	expect(new LastLocation(file).restore(new URL(origin), "home").hash).toBe("");
	writeFileSync(file, JSON.stringify({ node: "home", hash: "#/thread/%zz" }));
	expect(new LastLocation(file).restore(new URL(origin), "home").hash).toBe("");
});
